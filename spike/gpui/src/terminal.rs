//! Eine Terminal-Kachel: alacritty_terminal hält Zustand und PTY, GPUI zeichnet.
//! Eigener Renderer, weil Zeds `terminal_view` GPL ist.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::grid::{Dimensions, Indexed, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point as TPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};
use gpui::{
    App, Bounds, ClipboardItem, Context, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Font, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, KeyDownEvent, Keystroke, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Point, ScrollEvent, SharedString, Style,
    StrikethroughStyle, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill, font, point,
    prelude::*, px, relative, rgb, size,
};

use crate::boxdraw;
use crate::theme::{FONT, Theme};

pub const FONT_SIZE: f32 = 13.0;
pub const LINE_HEIGHT: f32 = 1.25;
const PAD: f32 = 8.0;

/// Namen von Umgebungsvariablen, die Kinder nicht erben sollen: die Marker einer umgebenden Claude-Code-Session
/// (sonst meldet Claude Code in der Kachel "inherited CLAUDE_CODE_CHILD_SESSION marker") und `AI_AGENT`.
/// Ist `CLAUDECODE` gesetzt, setzt Claude Code für seine Werkzeug-Prozesse auch `NO_COLOR=1`; das erbte
/// jede Shell und jedes Programm in der Kachel und schaltete deren Farben ab. Es fliegt dann ebenfalls raus.
pub fn inherited_env_to_strip(names: &[String]) -> Vec<String> {
    let upper = |n: &str| n.to_ascii_uppercase();
    let in_claude_session = names.iter().any(|n| upper(n) == "CLAUDECODE");
    names
        .iter()
        .filter(|n| {
            let u = upper(n);
            u == "CLAUDECODE"
                || u.starts_with("CLAUDE_CODE_")
                || u == "AI_AGENT"
                || (in_claude_session && u == "NO_COLOR")
        })
        .cloned()
        .collect()
}

/// Entfernt die Variablen aus `inherited_env_to_strip` aus der Umgebung des Prozesses.
/// Nur beim Start aufrufen, bevor Threads laufen.
pub fn strip_inherited_env() {
    let names: Vec<String> = std::env::vars_os().filter_map(|(k, _)| k.into_string().ok()).collect();
    for name in inherited_env_to_strip(&names) {
        // SAFETY: Aufruf am Anfang von `main`, bevor GPUI, der Tokio-/Event-Loop oder eigene Threads
        // starten; in dem Moment liest oder schreibt niemand sonst die Umgebung.
        unsafe { std::env::remove_var(&name) };
    }
}

#[derive(Clone)]
struct Listener(async_channel::Sender<Event>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.0.try_send(event);
    }
}

#[derive(Clone, Copy, PartialEq)]
struct Grid {
    cols: usize,
    lines: usize,
}

impl Dimensions for Grid {
    fn total_lines(&self) -> usize {
        self.lines
    }
    fn screen_lines(&self) -> usize {
        self.lines
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

struct Search {
    query: String,
    hit: Option<Match>,
}

pub struct TerminalPane {
    term: Arc<FairMutex<Term<Listener>>>,
    notifier: Notifier,
    pub focus: FocusHandle,
    grid: Grid,
    cell: gpui::Size<Pixels>,
    origin: Point<Pixels>,
    marked: Option<String>,
    search: Option<Search>,
    selecting: bool,
    scroll_rest: f32,
    pub exited: bool,
    pub title: Option<String>,
}

impl TerminalPane {
    pub fn new(program: &str, args: &[&str], cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let (tx, rx) = async_channel::unbounded();
        let listener = Listener(tx);
        let grid = Grid { cols: 100, lines: 30 };
        let term = Arc::new(FairMutex::new(Term::new(Config::default(), &grid, listener.clone())));

        let mut options = tty::Options {
            shell: Some(tty::Shell::new(program.into(), args.iter().map(|a| a.to_string()).collect())),
            working_directory: Some(cwd),
            env: HashMap::from([
                ("TERM".into(), "xterm-256color".into()),
                ("COLORTERM".into(), "truecolor".into()),
            ]),
            ..Default::default()
        };
        options.drain_on_exit = false;
        let pty = tty::new(&options, window_size(grid, size(px(8.), px(16.))), 0).expect("PTY");
        let event_loop = EventLoop::new(term.clone(), listener, pty, false, false).expect("event loop");
        let notifier = Notifier(event_loop.channel());
        event_loop.spawn();

        cx.spawn(async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                if this.update(cx, |pane, cx| pane.on_event(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            term,
            notifier,
            focus: cx.focus_handle(),
            grid,
            cell: size(px(8.), px(16.)),
            origin: point(px(0.), px(0.)),
            marked: None,
            search: None,
            selecting: false,
            scroll_rest: 0.,
            exited: false,
            title: None,
        }
    }

    fn on_event(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::PtyWrite(text) => self.notifier.notify(text.into_bytes()),
            Event::ClipboardStore(_, text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            Event::ClipboardLoad(_, format) => {
                let text = cx.read_from_clipboard().and_then(|i| i.text()).unwrap_or_default();
                self.notifier.notify(format(&text).into_bytes());
            }
            Event::TextAreaSizeRequest(format) => {
                let text = format(window_size(self.grid, self.cell));
                self.notifier.notify(text.into_bytes());
            }
            Event::ColorRequest(index, format) => {
                let c = self.term.lock().colors()[index].unwrap_or(Rgb { r: 0, g: 0, b: 0 });
                self.notifier.notify(format(c).into_bytes());
            }
            Event::Title(title) => self.title = Some(title),
            Event::ResetTitle => self.title = None,
            Event::ChildExit(_) | Event::Exit => self.exited = true,
            _ => {}
        }
        cx.notify();
    }

    /// Eingabe an den Prozess, springt dabei ans Ende des Verlaufs.
    fn write(&mut self, bytes: impl Into<Cow<'static, [u8]>>) {
        self.term.lock().scroll_display(Scroll::Bottom);
        self.notifier.notify(bytes);
    }

    fn resize(&mut self, grid: Grid, cell: gpui::Size<Pixels>) {
        if grid == self.grid && cell == self.cell {
            return;
        }
        self.grid = grid;
        self.cell = cell;
        self.term.lock().resize(grid);
        self.notifier.on_resize(window_size(grid, cell));
    }

    pub fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) else { return };
        let bracketed = self.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        if bracketed {
            let clean = text.replace('\x1b', "");
            self.write(format!("\x1b[200~{clean}\x1b[201~").into_bytes());
        } else {
            self.write(text.into_bytes());
        }
    }

    fn copy(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.term.lock().selection_to_string() else { return false };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        if self.search.is_some() {
            if self.search_key(ks, cx) {
                cx.stop_propagation();
            }
            return;
        }
        let handled = if is_cmd(ks, "v") {
            self.paste(cx);
            true
        } else if is_cmd(ks, "c") {
            // Auch ohne Auswahl verschlucken: Strg+Shift+C soll nie ein Strg+C an den Prozess schicken.
            self.copy(cx);
            true
        } else if is_cmd(ks, "f") {
            self.search = Some(Search { query: String::new(), hit: None });
            cx.notify();
            true
        } else {
            let app_cursor = self.term.lock().mode().contains(TermMode::APP_CURSOR);
            match key_bytes(ks, app_cursor) {
                Some(bytes) => {
                    self.term.lock().selection = None;
                    self.write(bytes);
                    true
                }
                None => false,
            }
        };
        if handled {
            cx.stop_propagation();
        }
    }

    /// Suche (⌘F / Strg+Shift+F): Tippen sucht, Enter springt zum nächsten älteren Treffer, Esc schließt.
    fn search_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        match ks.key.as_str() {
            "escape" => {
                self.search = None;
                let mut term = self.term.lock();
                term.selection = None;
                term.scroll_display(Scroll::Bottom);
            }
            "enter" => self.find(true),
            "backspace" => {
                if let Some(s) = self.search.as_mut() {
                    s.query.pop();
                    s.hit = None;
                }
                self.find(false);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    fn find(&mut self, next: bool) {
        let Some(search) = self.search.as_mut() else { return };
        let mut term = self.term.lock();
        term.selection = None;
        if search.query.is_empty() {
            return;
        }
        let Ok(mut regex) = RegexSearch::new(&escape_regex(&search.query)) else { return };
        let origin = match (&search.hit, next) {
            (Some(hit), true) => *hit.start(),
            (Some(hit), false) => *hit.end(),
            _ => TPoint::new(Line(term.screen_lines() as i32 - 1), Column(term.columns() - 1)),
        };
        let origin = if next { origin.sub(&*term, Boundary::None, 1) } else { origin };
        let hit = term.search_next(&mut regex, origin, Direction::Left, Side::Left, None);
        if let Some(hit) = &hit {
            let mut sel = Selection::new(SelectionType::Simple, *hit.start(), Side::Left);
            sel.update(*hit.end(), Side::Right);
            term.selection = Some(sel);
            term.scroll_to_point(*hit.start());
        }
        search.hit = hit;
    }

    fn cell_at(&self, pos: Point<Pixels>) -> (TPoint, Side) {
        let term = self.term.lock();
        let x = ((pos.x - self.origin.x) / self.cell.width).max(0.);
        let y = ((pos.y - self.origin.y) / self.cell.height).max(0.);
        let col = (x as usize).min(self.grid.cols.saturating_sub(1));
        let row = (y as usize).min(self.grid.lines.saturating_sub(1));
        let side = if x.fract() < 0.5 { Side::Left } else { Side::Right };
        let offset = term.grid().display_offset();
        (alacritty_terminal::term::viewport_to_point(offset, TPoint::new(row, Column(col))), side)
    }

    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let (p, side) = self.cell_at(e.position);
        let ty = match e.click_count {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        self.term.lock().selection = Some(Selection::new(ty, p, side));
        self.selecting = true;
        cx.notify();
    }

    fn mouse_move(&mut self, e: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || e.pressed_button != Some(MouseButton::Left) {
            self.selecting = false;
            return;
        }
        let (p, side) = self.cell_at(e.position);
        if let Some(sel) = self.term.lock().selection.as_mut() {
            sel.update(p, side);
        }
        cx.notify();
    }

    fn scroll(&mut self, e: &ScrollEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let dy = e.delta.pixel_delta(self.cell.height).y / self.cell.height + self.scroll_rest;
        let lines = dy.trunc() as i32;
        self.scroll_rest = dy.fract();
        if lines == 0 {
            return;
        }
        let mode = *self.term.lock().mode();
        if mode.intersects(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE) {
            let (p, _) = self.cell_at(e.position);
            let button = if lines > 0 { 64 } else { 65 };
            let seq = format!("\x1b[<{button};{};{}M", p.column.0 + 1, p.line.0 + 1);
            for _ in 0..lines.unsigned_abs() {
                self.notifier.notify(seq.clone().into_bytes());
            }
        } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
            let seq: &[u8] = if lines > 0 { b"\x1bOA" } else { b"\x1bOB" };
            for _ in 0..lines.unsigned_abs() {
                self.notifier.notify(seq);
            }
        } else {
            self.term.lock().scroll_display(Scroll::Delta(lines));
        }
        cx.notify();
    }
}

impl gpui::Focusable for TerminalPane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TerminalPane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let theme = cx.global::<crate::ThemeGlobal>().0;
        let search = self.search.as_ref().map(|s| {
            let found = if s.query.is_empty() || s.hit.is_some() { "" } else { "  keine Treffer" };
            format!("Suche: {}▏{found}   Enter weiter · Esc schließen", s.query)
        });
        div()
            .id("terminal")
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_scroll(cx.listener(Self::scroll))
            .relative()
            .child(TerminalElement { pane: entity })
            .children(search.map(|text| {
                div()
                    .absolute()
                    .top(px(6.))
                    .right(px(10.))
                    .px_2()
                    .py_1()
                    .rounded(px(5.))
                    .bg(theme.line)
                    .text_color(theme.fg)
                    .text_size(px(12.))
                    .child(text)
            }))
    }
}

// ---------- Tasten ----------

/// ⌘ auf dem Mac, Strg+Shift unter Windows (Strg allein gehört dem Terminal).
fn is_cmd(ks: &Keystroke, key: &str) -> bool {
    let m = ks.modifiers;
    let primary = if cfg!(target_os = "macos") {
        m.platform && !m.control && !m.alt
    } else {
        m.control && m.shift && !m.alt
    };
    primary && ks.key == key
}

/// Bytes für Sondertasten und Strg-Kombinationen. `None`: normaler Text, den der Input-Handler bekommt
/// (dort landen auch Tottasten, AltGr und ⌥-Zeichen wie @ auf der deutschen Mac-Tastatur).
fn key_bytes(ks: &Keystroke, app_cursor: bool) -> Option<Vec<u8>> {
    let m = ks.modifiers;
    if m.platform {
        return None;
    }
    let altgr = m.control && m.alt;
    let modcode = 1 + (m.shift as u8) + 2 * (m.alt as u8) + 4 * (m.control as u8);
    let csi = |c: &str| -> Vec<u8> {
        if modcode > 1 {
            format!("\x1b[1;{modcode}{c}").into_bytes()
        } else if app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if modcode > 1 { format!("\x1b[{n};{modcode}~") } else { format!("\x1b[{n}~") }.into_bytes()
    };
    let alt_prefix = |b: &[u8]| -> Vec<u8> {
        if m.alt && !altgr { [b"\x1b", b].concat() } else { b.to_vec() }
    };
    let bytes = match ks.key.as_str() {
        "enter" if m.shift => b"\x1b\r".to_vec(),
        "enter" => alt_prefix(b"\r"),
        "backspace" if m.control && !altgr => b"\x08".to_vec(),
        "backspace" => alt_prefix(b"\x7f"),
        "tab" if m.shift => b"\x1b[Z".to_vec(),
        "tab" => b"\t".to_vec(),
        "escape" => b"\x1b".to_vec(),
        "up" => csi("A"),
        "down" => csi("B"),
        "right" => csi("C"),
        "left" => csi("D"),
        "home" => csi("H"),
        "end" => csi("F"),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" => b"\x1bOP".to_vec(),
        "f2" => b"\x1bOQ".to_vec(),
        "f3" => b"\x1bOR".to_vec(),
        "f4" => b"\x1bOS".to_vec(),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        "space" if m.control && !altgr => b"\x00".to_vec(),
        key if m.control && !altgr && key.len() == 1 => {
            let c = key.as_bytes()[0].to_ascii_lowercase();
            let code = match c {
                b'a'..=b'z' => c - b'a' + 1,
                b'[' => 0x1b,
                b'\\' => 0x1c,
                b']' => 0x1d,
                b'6' => 0x1e,
                b'-' | b'/' => 0x1f,
                _ => return None,
            };
            alt_prefix(&[code])
        }
        // Windows: Alt+Taste als Meta (ESC davor). Auf dem Mac tippt ⌥ Zeichen, das macht der Input-Handler.
        _ if m.alt && !altgr && !cfg!(target_os = "macos") => {
            let c = ks.key_char.clone().unwrap_or_else(|| ks.key.clone());
            alt_prefix(c.as_bytes())
        }
        _ => return None,
    };
    Some(bytes)
}

fn escape_regex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn window_size(grid: Grid, cell: gpui::Size<Pixels>) -> WindowSize {
    WindowSize {
        num_lines: grid.lines as u16,
        num_cols: grid.cols as u16,
        cell_width: f32::from(cell.width) as u16,
        cell_height: f32::from(cell.height) as u16,
    }
}

// ---------- Texteingabe (IME, Tottasten, AltGr) ----------

impl EntityInputHandler for TerminalPane {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted: &mut Option<Range<usize>>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled: bool,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _w: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|m| 0..m.encode_utf16().count())
    }

    fn unmark_text(&mut self, _w: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = None;
        if let Some(search) = self.search.as_mut() {
            search.query.push_str(text);
            search.hit = None;
            self.find(false);
        } else if !text.is_empty() {
            self.term.lock().selection = None;
            self.write(text.as_bytes().to_vec());
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = (!text.is_empty()).then(|| text.to_string());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _bounds: Bounds<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let cursor = self.term.lock().grid().cursor.point;
        let origin = point(
            self.origin.x + self.cell.width * cursor.column.0 as f32,
            self.origin.y + self.cell.height * cursor.line.0 as f32,
        );
        Some(Bounds::new(origin, self.cell))
    }

    fn character_index_for_point(
        &mut self,
        _p: Point<Pixels>,
        _w: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

// ---------- Zeichnen ----------

struct TerminalElement {
    pane: Entity<TerminalPane>,
}


/// Zusammenhängende Zellen einer Zeile mit gleichem Stil, ein `shape_line`-Aufruf.
struct Run {
    col: usize,
    /// Belegte Zellen (Zeichen mit Breite 1 oder 2, kombinierende Zeichen zählen nicht).
    cells: usize,
    text: String,
    fg: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    wide: bool,
}

/// Stilmerkmale einer Zelle, die einen Lauf bestimmen.
#[derive(Clone, Copy, PartialEq)]
struct CellStyle {
    fg: Hsla,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

impl Run {
    fn continues(&self, col: usize, s: &CellStyle) -> bool {
        !self.wide
            && self.col + self.cells == col
            && self.fg == s.fg
            && self.bold == s.bold
            && self.italic == s.italic
            && self.underline == s.underline
            && self.strike == s.strike
    }
}

/// Selbst gezeichnetes Zeichen (Box-Drawing, Blöcke, Powerline) an seiner Zellposition.
struct Glyph {
    origin: Point<Pixels>,
    shapes: Vec<boxdraw::Shape>,
    color: Hsla,
}

#[derive(Default)]
pub struct Frame {
    rects: Vec<(Bounds<Pixels>, Hsla)>,
    glyphs: Vec<Glyph>,
    /// Ein Eintrag pro sichtbarer Zeile.
    rows: Vec<Vec<Run>>,
    cursor: Option<(Bounds<Pixels>, CursorShape)>,
    marked: Option<(Point<Pixels>, String)>,
    bg: Hsla,
    fg: Hsla,
    focused: bool,
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = Frame;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _iid: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _iid: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _rl: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Frame {
        let theme = cx.global::<crate::ThemeGlobal>().0;
        let font_size = px(FONT_SIZE);
        let base = font(FONT);
        let ts = window.text_system();
        let font_id = ts.resolve_font(&base);
        let cell_w = ts.advance(font_id, font_size, 'm').map(|s| s.width).unwrap_or(px(8.));
        let cell = size(cell_w, (font_size * LINE_HEIGHT).round());
        let origin = point(bounds.origin.x + px(PAD), bounds.origin.y + px(PAD / 2.));
        let cols = ((bounds.size.width - px(2. * PAD)) / cell.width).floor().max(2.) as usize;
        let lines = ((bounds.size.height - px(PAD)) / cell.height).floor().max(1.) as usize;

        let focused = self.pane.read(cx).focus.is_focused(window);
        self.pane.update(cx, |pane, _| {
            pane.resize(Grid { cols, lines }, cell);
            pane.origin = origin;
        });

        let pane = self.pane.read(cx);
        let term = pane.term.lock();
        let content = term.renderable_content();
        let offset = content.display_offset;
        let selection = content.selection;
        let mut frame = Frame { bg: theme.bg, fg: theme.fg, focused, ..Default::default() };
        let mut row_of: Vec<Vec<Run>> = (0..lines).map(|_| Vec::new()).collect();
        // Unter einem gefüllten Block-Cursor steht das Zeichen in Hintergrundfarbe, sonst wäre es unlesbar.
        let block_cursor = (focused && content.cursor.shape == CursorShape::Block)
            .then(|| alacritty_terminal::term::point_to_viewport(offset, content.cursor.point))
            .flatten()
            .map(|vp| (vp.line, vp.column.0));

        for item in content.display_iter {
            let flags = item.flags;
            if flags.contains(Flags::WIDE_CHAR_SPACER) || flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let Some(vp) = alacritty_terminal::term::point_to_viewport(offset, item.point) else { continue };
            let (row, col) = (vp.line, vp.column.0);
            if row >= lines {
                continue;
            }
            let selected = selection.is_some_and(|s| s.contains(item.point));
            let (mut fg, bg) = cell_colors(&term, &item, &theme);
            if block_cursor == Some((row, col)) && !selected {
                fg = theme.bg;
            }
            let wide = flags.contains(Flags::WIDE_CHAR);
            let at = point(origin.x + cell.width * col as f32, origin.y + cell.height * row as f32);
            let cell_box = Bounds::new(at, size(if wide { cell.width * 2. } else { cell.width }, cell.height));
            if bg != theme.bg {
                frame.rects.push((cell_box, bg));
            }
            // Auswahl liegt halbtransparent über dem eigenen Hintergrund der Zelle.
            if selected {
                frame.rects.push((cell_box, theme.accent.opacity(0.35)));
            }
            if flags.contains(Flags::HIDDEN) {
                continue;
            }
            if boxdraw::is_builtin(item.c) {
                let shapes = boxdraw::shapes(item.c, f32::from(cell.width), f32::from(cell.height));
                frame.glyphs.push(Glyph { origin: at, shapes, color: fg });
                continue;
            }
            let style = CellStyle {
                fg,
                bold: flags.contains(Flags::BOLD),
                italic: flags.contains(Flags::ITALIC),
                underline: flags.intersects(Flags::ALL_UNDERLINES),
                strike: flags.contains(Flags::STRIKEOUT),
            };
            if item.c == ' ' && !style.underline && !style.strike {
                continue;
            }
            push_char(&mut row_of[row], &item, col, wide, style);
        }
        frame.rows = row_of;

        let c = content.cursor;
        if c.shape != CursorShape::Hidden
            && let Some(vp) = alacritty_terminal::term::point_to_viewport(offset, c.point)
        {
            let at = point(origin.x + cell.width * vp.column.0 as f32, origin.y + cell.height * vp.line as f32);
            // Über einem breiten Zeichen (日本) deckt der Cursor beide Zellen ab.
            let wide = term.grid()[c.point].flags.contains(Flags::WIDE_CHAR);
            let cursor_cell = if wide { size(cell.width * 2., cell.height) } else { cell };
            frame.cursor = Some((Bounds::new(at, cursor_cell), c.shape));
            if let Some(m) = &pane.marked {
                frame.marked = Some((at, m.clone()));
            }
        }
        frame.rects.insert(0, (bounds, theme.bg));
        let _ = cols;
        frame
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _iid: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _rl: &mut (),
        frame: &mut Frame,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.pane.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.pane.clone()), cx);
        let font_size = px(FONT_SIZE);
        let cell_w = self.pane.read(cx).cell.width;
        let line_h = self.pane.read(cx).cell.height;
        let origin = self.pane.read(cx).origin;

        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for (b, c) in &frame.rects {
                window.paint_quad(fill(*b, *c));
            }
            if let Some((b, shape)) = frame.cursor {
                let c = frame.fg;
                let quad = match (frame.focused, shape) {
                    (true, CursorShape::Beam) => fill(Bounds::new(b.origin, size(px(2.), b.size.height)), c),
                    (true, CursorShape::Underline) => fill(
                        Bounds::new(point(b.origin.x, b.origin.y + b.size.height - px(2.)), size(b.size.width, px(2.))),
                        c,
                    ),
                    (true, CursorShape::HollowBlock) => gpui::outline(b, c, gpui::BorderStyle::Solid),
                    (true, _) => fill(b, c),
                    (false, _) => gpui::outline(b, c.opacity(0.6), gpui::BorderStyle::Solid),
                };
                window.paint_quad(quad);
            }
            for g in &frame.glyphs {
                boxdraw::paint(window, &g.shapes, g.origin, g.color);
            }
            for (row, runs) in frame.rows.iter().enumerate() {
                let y = origin.y + line_h * row as f32;
                for run in runs {
                    let f = Font {
                        weight: if run.bold { FontWeight::BOLD } else { FontWeight::NORMAL },
                        style: if run.italic { FontStyle::Italic } else { FontStyle::Normal },
                        ..font(FONT)
                    };
                    let tr = TextRun {
                        len: run.text.len(),
                        font: f,
                        color: run.fg,
                        background_color: None,
                        underline: run.underline.then_some(UnderlineStyle {
                            color: Some(run.fg),
                            thickness: px(1.),
                            wavy: false,
                        }),
                        strikethrough: run.strike.then_some(StrikethroughStyle { color: Some(run.fg), thickness: px(1.) }),
                    };
                    let text: SharedString = run.text.clone().into();
                    let advance = if run.wide { cell_w * 2. } else { cell_w };
                    let line = window.text_system().shape_line(text, font_size, &[tr], Some(advance));
                    let at = point(origin.x + cell_w * run.col as f32, y);
                    let _ = line.paint(at, line_h, gpui::TextAlign::Left, None, window, cx);
                }
            }
            // Vorschau einer IME-/Tottasten-Eingabe am Cursor
            if let Some((at, text)) = &frame.marked {
                let tr = TextRun {
                    len: text.len(),
                    font: font(FONT),
                    color: frame.fg,
                    background_color: Some(frame.bg),
                    underline: Some(UnderlineStyle { color: Some(frame.fg), thickness: px(1.), wavy: false }),
                    strikethrough: None,
                };
                let line = window.text_system().shape_line(text.clone().into(), font_size, &[tr], None);
                let _ = line.paint(*at, line_h, gpui::TextAlign::Left, None, window, cx);
            }
        });
    }
}

/// Vorder- und Hintergrundfarbe einer Zelle nach INVERSE und DIM.
fn cell_colors(term: &Term<Listener>, item: &Indexed<&Cell>, t: &Theme) -> (Hsla, Hsla) {
    let mut fg = color(term, item.fg, t);
    let mut bg = color(term, item.bg, t);
    if item.flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if item.flags.contains(Flags::DIM) {
        fg.a *= 0.66;
    }
    (fg, bg)
}

/// Hängt das Zeichen an den letzten Lauf der Zeile oder beginnt einen neuen.
fn push_char(runs: &mut Vec<Run>, item: &Indexed<&Cell>, col: usize, wide: bool, style: CellStyle) {
    let mut text = item.c.to_string();
    if let Some(zw) = item.zerowidth() {
        text.extend(zw);
    }
    match runs.last_mut() {
        Some(r) if !wide && r.continues(col, &style) => {
            r.text.push_str(&text);
            r.cells += 1;
        }
        _ => runs.push(Run {
            col,
            cells: if wide { 2 } else { 1 },
            text,
            fg: style.fg,
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
            strike: style.strike,
            wide,
        }),
    }
}

fn color(term: &Term<Listener>, c: Color, t: &Theme) -> Hsla {
    match c {
        Color::Spec(rgb_) => rgb_hsla(rgb_),
        Color::Indexed(i) => indexed(term, i as usize, t),
        Color::Named(n) => named(term, n, t),
    }
}

fn rgb_hsla(c: Rgb) -> Hsla {
    rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32).into()
}

fn named(term: &Term<Listener>, n: NamedColor, t: &Theme) -> Hsla {
    if let Some(c) = term.colors()[n] {
        return rgb_hsla(c);
    }
    let i = n as usize;
    match n {
        NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => t.fg,
        NamedColor::Background => t.bg,
        NamedColor::DimForeground => t.sub,
        _ if i < 16 => t.ansi[i % 8],
        _ => {
            // Dim-Varianten folgen nach Foreground/Background/Cursor (259…266)
            let base = i.saturating_sub(259) % 8;
            t.ansi[base].opacity(0.7)
        }
    }
}

fn indexed(term: &Term<Listener>, i: usize, t: &Theme) -> Hsla {
    if let Some(c) = term.colors()[i] {
        return rgb_hsla(c);
    }
    match i {
        0..=15 => t.ansi[i % 8],
        16..=231 => {
            let i = i - 16;
            let step = |v: usize| if v == 0 { 0 } else { (v * 40 + 55) as u8 };
            rgb_hsla(Rgb { r: step(i / 36), g: step((i / 6) % 6), b: step(i % 6) })
        }
        _ => {
            let v = (8 + (i - 232) * 10) as u8;
            rgb_hsla(Rgb { r: v, g: v, b: v })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(s: &'static str, app_cursor: bool) -> Option<Vec<u8>> {
        key_bytes(&Keystroke::parse(s).expect("gültiger Tastenname"), app_cursor)
    }

    #[test]
    fn enter_und_ctrl_c() {
        assert_eq!(keys("enter", false), Some(b"\r".to_vec()));
        assert_eq!(keys("ctrl-c", false), Some(vec![0x03]));
        assert_eq!(keys("ctrl-a", false), Some(vec![0x01]));
        assert_eq!(keys("ctrl-space", false), Some(vec![0x00]));
    }

    #[test]
    fn pfeile_normal_und_app_cursor() {
        assert_eq!(keys("up", false), Some(b"\x1b[A".to_vec()));
        assert_eq!(keys("up", true), Some(b"\x1bOA".to_vec()));
        assert_eq!(keys("left", false), Some(b"\x1b[D".to_vec()));
        // Mit Modifikator immer die CSI-Form
        assert_eq!(keys("ctrl-right", true), Some(b"\x1b[1;5C".to_vec()));
        assert_eq!(keys("delete", false), Some(b"\x1b[3~".to_vec()));
    }

    #[test]
    fn shift_tab() {
        assert_eq!(keys("shift-tab", false), Some(b"\x1b[Z".to_vec()));
        assert_eq!(keys("tab", false), Some(b"\t".to_vec()));
    }

    #[test]
    fn altgr_und_text_gehen_an_den_input_handler() {
        // AltGr+Q (@) meldet Windows als Strg+Alt: kein Steuercode, der Text kommt über WM_CHAR.
        assert_eq!(keys("ctrl-alt-q", false), None);
        assert_eq!(keys("a", false), None);
        assert_eq!(keys("shift-a", false), None);
    }

    #[test]
    fn plattform_taste_bleibt_der_app() {
        assert_eq!(keys("cmd-c", false), None);
    }

    #[test]
    fn regex_wird_maskiert() {
        assert_eq!(escape_regex("a.b*c"), "a\\.b\\*c");
        assert_eq!(escape_regex("(x)[y]"), "\\(x\\)\\[y\\]");
        assert_eq!(escape_regex("plain äö"), "plain äö");
        assert!(RegexSearch::new(&escape_regex("a+b|c{1}$^#&-~")).is_ok());
    }

    fn feed(input: &str) -> Term<Listener> {
        let mut t = Term::new(Config::default(), &Grid { cols: 20, lines: 2 }, Listener(async_channel::unbounded().0));
        let mut parser = alacritty_terminal::vte::ansi::Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new();
        parser.advance(&mut t, input.as_bytes());
        t
    }

    fn cell_at(t: &Term<Listener>, col: usize) -> alacritty_terminal::term::cell::Cell {
        t.grid()[TPoint::new(Line(0), Column(col))].clone()
    }

    #[test]
    fn escape_farben_landen_in_den_zellen() {
        let t = feed("\x1b[31mr\x1b[0m\x1b[38;2;215;119;87mt\x1b[0m\x1b[48;5;24mb");
        let theme = crate::theme::Theme::mocha();
        let (r, tc, b) = (cell_at(&t, 0), cell_at(&t, 1), cell_at(&t, 2));
        println!("{:?} {:?} {:?}", r.fg, tc.fg, b.bg);
        assert_eq!(color(&t, r.fg, &theme).to_rgb(), theme.ansi[1].to_rgb());
        assert_eq!(color(&t, tc.fg, &theme).to_rgb(), rgb_hsla(Rgb { r: 215, g: 119, b: 87 }).to_rgb());
        assert_eq!(color(&t, b.bg, &theme).to_rgb(), rgb_hsla(Rgb { r: 0, g: 95, b: 135 }).to_rgb());
    }

    #[test]
    fn claude_marker_und_no_color_werden_entfernt() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let in_claude = names(&["PATH", "CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "claude_code_entrypoint", "NO_COLOR", "AI_AGENT", "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"]);
        assert_eq!(
            inherited_env_to_strip(&in_claude),
            names(&["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "claude_code_entrypoint", "NO_COLOR", "AI_AGENT"])
        );
        // Ein NO_COLOR, das der Nutzer selbst gesetzt hat (keine Claude-Session), bleibt.
        assert_eq!(inherited_env_to_strip(&names(&["NO_COLOR", "PATH"])), Vec::<String>::new());
    }

    #[test]
    fn strich_dim_inverse_und_breite_zeichen() {
        let t = feed("\x1b[9ma\x1b[0m\x1b[2mb\x1b[0m\x1b[4mc\x1b[0m日");
        assert!(cell_at(&t, 0).flags.contains(Flags::STRIKEOUT));
        assert!(cell_at(&t, 1).flags.contains(Flags::DIM));
        assert!(cell_at(&t, 2).flags.contains(Flags::UNDERLINE));
        assert!(cell_at(&t, 3).flags.contains(Flags::WIDE_CHAR));
        assert!(cell_at(&t, 4).flags.contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn farbwuerfel_ecken() {
        let t = Term::new(Config::default(), &Grid { cols: 4, lines: 2 }, Listener(async_channel::unbounded().0));
        let theme = crate::theme::Theme::mocha();
        let same = |i: usize, r: u8, g: u8, b: u8| {
            assert_eq!(indexed(&t, i, &theme).to_rgb(), rgb_hsla(Rgb { r, g, b }).to_rgb(), "Index {i}");
        };
        same(16, 0, 0, 0);
        same(231, 255, 255, 255);
        same(196, 255, 0, 0);
        same(232, 8, 8, 8);
        same(255, 238, 238, 238);
        assert_eq!(indexed(&t, 1, &theme).to_rgb(), theme.ansi[1].to_rgb());
    }
}