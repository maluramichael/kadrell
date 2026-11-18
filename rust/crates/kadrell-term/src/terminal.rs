//! Eine Terminal-Kachel: alacritty_terminal hält Zustand und PTY, GPUI zeichnet.
//! Eigener Renderer, weil Zeds `terminal_view` GPL ist.

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg, Notifier};
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
    EntityInputHandler, EventEmitter, FocusHandle, Font, FontStyle, FontWeight, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, KeyDownEvent, Keystroke, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Point, ScrollEvent, SharedString, Style,
    StrikethroughStyle, TextRun, UTF16Selection, UnderlineStyle, Window, div, fill, font, point,
    prelude::*, px, relative, rgb, size,
};

use crate::boxdraw;
use crate::theme::{FONT, TermTheme, TermThemeGlobal};

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
struct Listener {
    tx: async_channel::Sender<Event>,
    /// Vom PTY-Thread gesetzt, sobald der Kindprozess endet: von jedem Thread lesbar (`TermHandle::exited`).
    exited: Arc<AtomicBool>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        if matches!(event, Event::ChildExit(_) | Event::Exit) {
            self.exited.store(true, Ordering::SeqCst);
        }
        let _ = self.tx.try_send(event);
    }
}

/// Ereignisse einer Kachel für den, der sie hält (`cx.subscribe`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalEvent {
    /// Der Kindprozess hat sich beendet (`/exit`, Absturz, Kill).
    Exited,
}

/// Was eine Kachel startet: Programm, Argumente, Ordner und Umgebung (zusätzlich zur geerbten).
#[derive(Debug, Clone, Default)]
pub struct TermSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

/// Zugriff auf eine Kachel ohne GPUI und von jedem Thread: schreiben, lesen, beenden. Das ist, was Steuerkanal
/// (`send`, `capture`, `stop`) und Statuspoll brauchen; die Oberfläche selbst bleibt beim `TerminalPane`.
#[derive(Clone)]
pub struct TermHandle {
    term: Arc<FairMutex<Term<Listener>>>,
    sender: EventLoopSender,
    exited: Arc<AtomicBool>,
    pid: Option<u32>,
}

impl TermHandle {
    /// pid des Kindprozesses (Windows: des Prozesses hinter der ConPTY).
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Der Kindprozess ist beendet.
    pub fn exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Rohe Bytes an den Prozess, springt dabei ans Ende des Verlaufs.
    pub fn write(&self, bytes: &[u8]) {
        self.term.lock().scroll_display(Scroll::Bottom);
        let _ = self.sender.send(Msg::Input(Cow::Owned(bytes.to_vec())));
    }

    /// Hat das Programm bracketed paste eingeschaltet?
    pub fn bracketed_paste(&self) -> bool {
        self.term.lock().mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Sichtbare Zeilen ohne Leerraum am Ende.
    pub fn visible_rows(&self) -> Vec<String> {
        let term = self.term.lock();
        (0..term.screen_lines() as i32).map(|l| TerminalPane::line_text(&term, Line(l)).0).collect()
    }

    /// Ganzer Puffer samt Verlauf, Zeilen mit `\n` getrennt.
    pub fn buffer_text(&self) -> String {
        let term = self.term.lock();
        (term.topmost_line().0..=term.bottommost_line().0).map(|l| TerminalPane::line_text(&term, Line(l)).0).collect::<Vec<_>>().join("\n")
    }

    /// Beendet den Prozess (samt Kindern) und die Schleife der Kachel.
    pub fn shutdown(&self) {
        let _ = self.sender.send(Msg::Shutdown);
        if let Some(pid) = self.pid.filter(|_| !self.exited()) {
            kill_process_tree(pid);
        }
    }
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("taskkill");
    cmd.args(["/F", "/T", "/PID", &pid.to_string()]).creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let _ = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
}

#[cfg(unix)]
fn kill_process_tree(pid: u32) {
    // SAFETY: `kill` mit einer pid und einem Signal hat keine weiteren Voraussetzungen.
    unsafe { libc::kill(pid as libc::pid_t, libc::SIGHUP) };
}

#[cfg(windows)]
fn pty_pid(pty: &tty::Pty) -> Option<u32> {
    pty.child_watcher().pid().map(|p| p.get())
}

#[cfg(unix)]
fn pty_pid(pty: &tty::Pty) -> Option<u32> {
    Some(pty.child().id())
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
    sender: EventLoopSender,
    exited_flag: Arc<AtomicBool>,
    pid: Option<u32>,
    pub focus: FocusHandle,
    grid: Grid,
    cell: gpui::Size<Pixels>,
    origin: Point<Pixels>,
    marked: Option<String>,
    search: Option<Search>,
    /// Zuletzt angezeigter Treffer (Begriff, Gridzeile): wird nach einer Größenänderung neu markiert, denn die
    /// Auswahl geht dabei verloren. Endet mit der nächsten Eingabe.
    reveal: Option<(String, i32)>,
    selecting: bool,
    scroll_rest: f32,
    pub exited: bool,
    pub title: Option<String>,
}

impl EventEmitter<TerminalEvent> for TerminalPane {}

impl TerminalPane {
    pub fn new(program: &str, args: &[&str], cwd: PathBuf, cx: &mut Context<Self>) -> Self {
        let spec = TermSpec { program: program.into(), args: args.iter().map(|a| a.to_string()).collect(), cwd, env: Vec::new() };
        Self::from_parts(start_pty(spec).expect("PTY"), cx)
    }

    /// Kachel zu einem schon gestarteten Prozess (`start_pty`). Der Start ist von GPUI getrennt, damit ein
    /// gescheiterter Start (Programm fehlt) ein Fehler bleibt statt einer Kachel.
    pub fn from_parts(parts: PtyParts, cx: &mut Context<Self>) -> Self {
        let PtyParts { term, notifier, sender, exited_flag, pid, rx, grid } = parts;
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
            sender,
            exited_flag,
            pid,
            focus: cx.focus_handle(),
            grid,
            cell: size(px(8.), px(16.)),
            origin: point(px(0.), px(0.)),
            marked: None,
            search: None,
            reveal: None,
            selecting: false,
            scroll_rest: 0.,
            exited: false,
            title: None,
        }
    }
}

/// Ein gestarteter Prozess samt Terminal-Zustand, noch ohne Oberfläche.
pub struct PtyParts {
    term: Arc<FairMutex<Term<Listener>>>,
    notifier: Notifier,
    sender: EventLoopSender,
    exited_flag: Arc<AtomicBool>,
    pid: Option<u32>,
    rx: async_channel::Receiver<Event>,
    grid: Grid,
}

impl PtyParts {
    /// Zugriff auf die Kachel ohne GPUI, von jedem Thread (Steuerkanal, Statuspoll).
    pub fn handle(&self) -> TermHandle {
        TermHandle { term: self.term.clone(), sender: self.sender.clone(), exited: self.exited_flag.clone(), pid: self.pid }
    }
}

/// Startet das Programm der Kachel. `spec.env` kommt zur geerbten Umgebung dazu (`TERM` und `COLORTERM` gelten,
/// wenn sie fehlen); unter Windows werden die Argumente für die Kommandozeile maskiert (Anführungszeichen in
/// `--settings {...}`).
pub fn start_pty(spec: TermSpec) -> std::io::Result<PtyParts> {
    let (tx, rx) = async_channel::unbounded();
    let exited_flag = Arc::new(AtomicBool::new(false));
    let listener = Listener { tx, exited: exited_flag.clone() };
    let grid = Grid { cols: 100, lines: 30 };
    let term = Arc::new(FairMutex::new(Term::new(Config::default(), &grid, listener.clone())));

    let mut env = HashMap::from([("TERM".to_string(), "xterm-256color".to_string()), ("COLORTERM".to_string(), "truecolor".to_string())]);
    env.extend(spec.env);
    let mut options = tty::Options { shell: Some(tty::Shell::new(spec.program, spec.args)), working_directory: Some(spec.cwd), env, ..Default::default() };
    options.drain_on_exit = false;
    #[cfg(windows)]
    {
        options.escape_args = true;
    }
    let pty = tty::new(&options, window_size(grid, size(px(8.), px(16.))), 0)?;
    let pid = pty_pid(&pty);
    let event_loop = EventLoop::new(term.clone(), listener, pty, false, false)?;
    let sender = event_loop.channel();
    let notifier = Notifier(sender.clone());
    event_loop.spawn();
    Ok(PtyParts { term, notifier, sender, exited_flag, pid, rx, grid })
}

impl TerminalPane {
    /// Zugriff auf die Kachel ohne GPUI, von jedem Thread (Steuerkanal, Statuspoll).
    pub fn handle(&self) -> TermHandle {
        TermHandle { term: self.term.clone(), sender: self.sender.clone(), exited: self.exited_flag.clone(), pid: self.pid }
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
            Event::ChildExit(_) | Event::Exit => {
                if !self.exited {
                    self.exited = true;
                    cx.emit(TerminalEvent::Exited);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Eingabe an den Prozess, springt dabei ans Ende des Verlaufs.
    fn write(&mut self, bytes: impl Into<Cow<'static, [u8]>>) {
        self.reveal = None;
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
        self.apply_reveal();
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
                self.reveal = None;
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

    /// Text einer Gridzeile ohne Leerraum am Ende, dazu die Spalte jedes Zeichens (breite Zeichen belegen zwei).
    fn line_text(term: &Term<Listener>, line: Line) -> (String, Vec<usize>) {
        let row = &term.grid()[line];
        let (mut text, mut cols) = (String::new(), Vec::new());
        for col in 0..term.columns() {
            let cell = &row[Column(col)];
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            text.push(cell.c);
            cols.push(col);
        }
        let keep = text.trim_end().chars().count();
        text.truncate(text.trim_end().len());
        cols.truncate(keep);
        (text, cols)
    }

    fn matching_lines(term: &Term<Listener>, query: &str, limit: usize) -> Vec<(i32, String)> {
        let mut out = Vec::new();
        for l in term.topmost_line().0..=term.bottommost_line().0 {
            let (text, _) = Self::line_text(term, Line(l));
            if !crate::textsearch::find_char_ranges(&text, query).is_empty() {
                out.push((l, text));
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }

    /// Alle Zeilen samt Verlauf, die `query` enthalten (ohne Groß-/Kleinschreibung), als (Gridzeile, Text), älteste
    /// zuerst, höchstens `limit`. Für die Suche in allen Terminals; `reveal_line` springt zu einer solchen Zeile.
    pub fn search_lines(&self, query: &str, limit: usize) -> Vec<(i32, String)> {
        Self::matching_lines(&self.term.lock(), query, limit)
    }

    /// Zeigt den ersten Treffer von `query` in der Gridzeile `line`: markiert ihn, scrollt hin und öffnet die
    /// Suchleiste mit dem Begriff, Enter blättert von dort weiter. Ändert sich danach die Größe der Kachel,
    /// wird der Treffer (die nächstliegende Zeile) neu markiert.
    pub fn reveal_line(&mut self, query: &str, line: i32, cx: &mut Context<Self>) {
        self.reveal = Some((query.to_string(), line));
        self.apply_reveal();
        cx.notify();
    }

    fn apply_reveal(&mut self) {
        let Some((query, wanted)) = self.reveal.clone() else { return };
        let mut term = self.term.lock();
        let lines = Self::matching_lines(&term, &query, usize::MAX);
        let Some((line, _)) = lines.into_iter().min_by_key(|(l, _)| l.abs_diff(wanted)) else { return };
        let (text, cols) = Self::line_text(&term, Line(line));
        let Some(hit) = crate::textsearch::find_char_ranges(&text, &query).into_iter().next() else { return };
        let start = TPoint::new(Line(line), Column(cols[hit.start]));
        let end = TPoint::new(Line(line), Column(cols[hit.end - 1]));
        let mut sel = Selection::new(SelectionType::Simple, start, Side::Left);
        sel.update(end, Side::Right);
        term.selection = Some(sel);
        term.scroll_to_point(start);
        drop(term);
        self.search = Some(Search { query: query.clone(), hit: Some(start..=end) });
        self.reveal = Some((query, line));
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
        self.reveal = None;
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
        let theme = cx.global::<TermThemeGlobal>().0;
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

/// Zellraster in ganzen Gerätepixeln. Zellbreite und -höhe sind ganzzahlig und der Ursprung liegt auf einem
/// Gerätepixel, damit Nachbarzellen exakt dieselbe Kante teilen: Ein gebrochener Zellabstand (7,8 px) lässt
/// Hintergrundflächen und Linienzeichen an den Nahtstellen per Anti-Aliasing als Haarlinien aufblitzen.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DevGrid {
    sf: f32,
    cw: f32,
    ch: f32,
    ox: f32,
    oy: f32,
}

impl DevGrid {
    /// Raster aus der Zeichenbreite (logische Pixel), der Zeilenhöhe und dem Ursprung des Bereichs.
    fn new(sf: f32, advance: f32, line_h: f32, origin: (f32, f32)) -> Self {
        Self {
            sf,
            cw: (advance * sf).round().max(1.),
            ch: (line_h * sf).round().max(1.),
            ox: (origin.0 * sf).round(),
            oy: (origin.1 * sf).round(),
        }
    }

    /// Zellgröße in logischen Pixeln (ganze Gerätepixel geteilt durch den Skalierungsfaktor).
    fn cell(&self) -> gpui::Size<Pixels> {
        size(px(self.cw / self.sf), px(self.ch / self.sf))
    }

    fn origin(&self) -> Point<Pixels> {
        point(px(self.ox / self.sf), px(self.oy / self.sf))
    }

    /// Obere linke Ecke einer Zelle in Gerätepixeln.
    fn cell_dev(&self, col: usize, row: usize) -> (f32, f32) {
        (self.ox + self.cw * col as f32, self.oy + self.ch * row as f32)
    }

    /// Fläche über `cells` Zellen ab (`row`, `col`) in logischen Pixeln.
    fn bounds(&self, row: usize, col: usize, cells: usize) -> Bounds<Pixels> {
        let (x, y) = self.cell_dev(col, row);
        Bounds::new(
            point(px(x / self.sf), px(y / self.sf)),
            size(px(self.cw * cells as f32 / self.sf), px(self.ch / self.sf)),
        )
    }
}

/// Waagerechte Fläche gleicher Farbe in einer Zeile.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    row: usize,
    col: usize,
    cells: usize,
    color: Hsla,
}

/// Fügt `span` an, verschmolzen mit dem letzten Eintrag, wenn der in derselben Zeile lückenlos davor endet
/// und dieselbe Farbe hat. Eine Fläche statt vieler Zellen: keine Nähte zwischen Nachbarzellen.
fn push_span(spans: &mut Vec<Span>, span: Span) {
    match spans.last_mut() {
        Some(l) if l.row == span.row && l.col + l.cells == span.col && l.color == span.color => {
            l.cells += span.cells;
        }
        _ => spans.push(span),
    }
}

/// Selbst gezeichnetes Zeichen (Box-Drawing, Blöcke, Powerline) an seiner Zellposition.
struct Glyph {
    /// Linke obere Zellecke in Gerätepixeln.
    origin: (f32, f32),
    shapes: Vec<boxdraw::Shape>,
    color: Hsla,
}

#[derive(Default)]
pub struct Frame {
    rects: Vec<(Bounds<Pixels>, Hsla)>,
    sf: f32,
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
        let theme = cx.global::<TermThemeGlobal>().0;
        let font_size = px(FONT_SIZE);
        let base = font(FONT);
        let ts = window.text_system();
        let font_id = ts.resolve_font(&base);
        let advance = ts.advance(font_id, font_size, 'm').map(|s| s.width).unwrap_or(px(8.));
        let sf = window.scale_factor();
        let line_h = (font_size * LINE_HEIGHT).round();
        let dev = DevGrid::new(
            sf,
            f32::from(advance),
            f32::from(line_h),
            (f32::from(bounds.origin.x) + PAD, f32::from(bounds.origin.y) + PAD / 2.),
        );
        let cell = dev.cell();
        let origin = dev.origin();
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
        let mut frame = Frame { bg: theme.bg, fg: theme.fg, focused, sf, ..Default::default() };
        let (mut bg_spans, mut sel_spans): (Vec<Span>, Vec<Span>) = (Vec::new(), Vec::new());
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
            let cells = if wide { 2 } else { 1 };
            if bg != theme.bg {
                push_span(&mut bg_spans, Span { row, col, cells, color: bg });
            }
            // Auswahl liegt halbtransparent über dem eigenen Hintergrund der Zelle.
            if selected {
                push_span(&mut sel_spans, Span { row, col, cells, color: theme.accent.opacity(0.35) });
            }
            if flags.contains(Flags::HIDDEN) {
                continue;
            }
            if boxdraw::is_builtin(item.c) {
                let shapes = boxdraw::shapes(item.c, dev.cw, dev.ch);
                frame.glyphs.push(Glyph { origin: dev.cell_dev(col, row), shapes, color: fg });
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
            let at = dev.bounds(vp.line, vp.column.0, 1).origin;
            // Über einem breiten Zeichen (日本) deckt der Cursor beide Zellen ab.
            let wide = term.grid()[c.point].flags.contains(Flags::WIDE_CHAR);
            frame.cursor = Some((dev.bounds(vp.line, vp.column.0, if wide { 2 } else { 1 }), c.shape));
            if let Some(m) = &pane.marked {
                frame.marked = Some((at, m.clone()));
            }
        }
        frame.rects.push((bounds, theme.bg));
        for s in bg_spans.iter().chain(&sel_spans) {
            frame.rects.push((dev.bounds(s.row, s.col, s.cells), s.color));
        }
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
                boxdraw::paint(window, &g.shapes, g.origin, frame.sf, g.color);
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
fn cell_colors(term: &Term<Listener>, item: &Indexed<&Cell>, t: &TermTheme) -> (Hsla, Hsla) {
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

fn color(term: &Term<Listener>, c: Color, t: &TermTheme) -> Hsla {
    match c {
        Color::Spec(rgb_) => rgb_hsla(rgb_),
        Color::Indexed(i) => indexed(term, i as usize, t),
        Color::Named(n) => named(term, n, t),
    }
}

fn rgb_hsla(c: Rgb) -> Hsla {
    rgb(((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32).into()
}

fn named(term: &Term<Listener>, n: NamedColor, t: &TermTheme) -> Hsla {
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

fn indexed(term: &Term<Listener>, i: usize, t: &TermTheme) -> Hsla {
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

    /// Catppuccin Mocha, wie die App es liefert.
    fn test_theme() -> TermTheme {
        let c = |v: u32| -> Hsla { rgb(v).into() };
        TermTheme {
            bg: c(0x11111b),
            fg: c(0xcdd6f4),
            sub: c(0xa6adc8),
            line: c(0x313244),
            accent: c(0x89b4fa),
            ansi: [0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de].map(c),
        }
    }

    fn span(row: usize, col: usize, cells: usize, color: Hsla) -> Span {
        Span { row, col, cells, color }
    }

    #[test]
    fn flaechen_gleicher_farbe_werden_verschmolzen() {
        let (a, b) = (rgb(0xff0000).into(), rgb(0x00ff00).into());
        let mut v = Vec::new();
        for col in [3, 4, 5] {
            push_span(&mut v, span(0, col, 1, a));
        }
        push_span(&mut v, span(0, 6, 2, b)); // andere Farbe
        push_span(&mut v, span(0, 9, 1, b)); // Lücke
        push_span(&mut v, span(1, 10, 1, b)); // andere Zeile
        assert_eq!(v, vec![span(0, 3, 3, a), span(0, 6, 2, b), span(0, 9, 1, b), span(1, 10, 1, b)]);
    }

    #[test]
    fn raster_liegt_auf_ganzen_geraetepixeln() {
        // Skalierung 1,25 und eine gebrochene Zeichenbreite wie bei 13 px Schrift.
        let g = DevGrid::new(1.25, 7.8, 16., (10.3, 4.1));
        assert_eq!((g.cw, g.ch, g.ox, g.oy), (10., 20., 13., 5.));
        let (x0, _) = g.cell_dev(29, 0);
        let (x1, _) = g.cell_dev(30, 0);
        assert_eq!(x1 - x0, g.cw);
        let b = g.bounds(2, 30, 2);
        assert_eq!(f32::from(b.size.width) * g.sf, 2. * g.cw);
        assert_eq!(f32::from(b.size.height) * g.sf, g.ch);
        // Eine zweizellige Fläche endet exakt dort, wo die nächste Zelle beginnt.
        let right = f32::from(b.origin.x + b.size.width) * g.sf;
        assert!((right - g.cell_dev(32, 2).0).abs() < 1e-3);
    }

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
        let theme = test_theme();
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
        let theme = test_theme();
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