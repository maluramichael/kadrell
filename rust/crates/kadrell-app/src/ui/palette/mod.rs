//! Suche (⌘P / Strg+P) wie `PaletteWindow.swift`: ein Feld, darunter die Trefferliste. Ohne Präfix springt sie zu
//! Sessions und Gruppen, `>` führt Kommandos aus, `/` sucht Text in allen laufenden Terminals, `#` durchsucht den
//! Verlauf aller Claude-Sessions. Logik in `model`, Verlaufsdaten hinter `history::HistoryProvider`, die Anbindung
//! an die Shell in `shell`.

mod history;
mod resume;
mod model;
mod shell;

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, HighlightStyle, Hsla, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled, StyledText,
    Subscription, Window, actions, div, prelude::*, px, relative,
};

use kadrell_term::TerminalPane;

use self::history::HistoryProvider;
pub use self::model::relative_de;
pub use self::history::{HistoryStore, provider as history_provider};
pub use self::resume::{HistoryAction, Known, Pick, decide as decide_history, problem_title};
use self::model::{Command, Item, Mode, Run, SessionHits, Snapshot, Target};
use super::chrome::h_seam;
use super::layout::{Id, LayoutMode};
use super::overlay::{backdrop, dialog_frame, overlay_height, overlay_width};
use super::text_input::{TextInput, TextInputEvent};
use super::theme;
use crate::theme::Theme;

actions!(kadrell_palette, [MoveUp, MoveDown, Confirm, Cancel]);

const CONTEXT: &str = "KadrellPalette";
const WIDTH: f32 = 640.;
const HEIGHT: f32 = 420.;
/// Oberkante bei 12 % der Fensterhöhe (`PaletteWindow.follow`).
const TOP: f32 = 0.12;
const ROW_HEIGHT: f32 = 44.;
const COMPACT_ROW_HEIGHT: f32 = 30.;
/// Wartezeit nach dem letzten Tastendruck, bevor die Textsuche über alle Terminals läuft.
const TEXT_DEBOUNCE: Duration = Duration::from_millis(50);

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("escape", Cancel, c),
    ]);
}

/// Was die Shell nach der Auswahl tun soll. Die Palette schließt sich nicht selbst.
pub enum PaletteEvent {
    Close,
    Session(Id),
    Group(usize),
    MoveFocused(usize),
    Layout(LayoutMode),
    Menu(Box<dyn gpui::Action>),
    /// Zeile in einem Terminal anzeigen (Gridzeile, Suchbegriff).
    Hit { id: Id, line: i32, query: String },
    /// ⏎ auf einem Treffer des Verlaufs: springen oder fortsetzen entscheidet die Shell (`resume::decide`).
    Resume(Pick),
}

impl EventEmitter<PaletteEvent> for Palette {}

pub struct Palette {
    input: Entity<TextInput>,
    snapshot: Snapshot,
    panes: HashMap<Id, Entity<TerminalPane>>,
    commands: Vec<Command>,
    history: Rc<dyn HistoryProvider>,
    items: Vec<Item>,
    selected: usize,
    scroll: ScrollHandle,
    /// Zählt Eingaben, damit eine verzögerte Textsuche nur für die letzte läuft.
    generation: u64,
    _typing: Subscription,
}

impl Palette {
    pub fn new(
        snapshot: Snapshot,
        panes: HashMap<Id, Entity<TerminalPane>>,
        history: Rc<dyn HistoryProvider>,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hint = "Session, Gruppe, Pfad suchen …  ( > Kommandos, / in allen Terminals, # alle Claude-Sessions )";
        let input = cx.new(|cx| TextInput::new(hint, cx));
        let typing = cx.subscribe(&input, |this, _, _: &TextInputEvent, cx| this.on_typed(cx));
        let commands = model::build_commands(&snapshot);
        let mut this = Self {
            input,
            snapshot,
            panes,
            commands,
            history,
            items: Vec::new(),
            selected: 0,
            scroll: ScrollHandle::new(),
            generation: 0,
            _typing: typing,
        };
        this.input.update(cx, |i, cx| i.set_text(initial, window, cx));
        this.refilter(cx);
        let focus = this.input.read(cx).focus.clone();
        focus.focus(window, cx);
        this
    }

    /// Ersetzt den Eingabetext (Strg+P bei offener Palette schaltet auf `>`, Strg+Umschalt+F auf `/`).
    pub fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |i, cx| i.set_text(text, window, cx));
    }

    /// Der Verlaufsindex hat sich geändert (Nachlesen fertig): in der Verlaufssuche neu filtern. Die Auswahl bleibt,
    /// solange die Liste nicht kürzer ist.
    pub fn refresh_history(&mut self, cx: &mut Context<Self>) {
        if model::parse(&self.raw(cx)).mode != Mode::History {
            return;
        }
        let keep = self.selected;
        self.refilter(cx);
        self.selected = keep.min(self.items.len().saturating_sub(1));
    }

    /// Zeigt statt der Treffer eine Meldung (z. B. warum sich eine Session nicht fortsetzen lässt); die nächste Eingabe
    /// räumt sie weg.
    pub fn show_notice(&mut self, title: &str, message: &str, cx: &mut Context<Self>) {
        self.items = vec![model::notice_item(title, message)];
        self.selected = 0;
        cx.notify();
    }

    fn raw(&self, cx: &App) -> String {
        self.input.read(cx).text().to_string()
    }

    /// Eingabe geändert: die Textsuche läuft leicht verzögert, alles andere sofort.
    fn on_typed(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        if model::parse(&self.raw(cx)).mode != Mode::Text {
            self.refilter(cx);
            return;
        }
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TEXT_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.refilter(cx);
                }
            });
        })
        .detach();
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let q = model::parse(&self.raw(cx));
        self.items = match q.mode {
            Mode::Sessions => model::session_items(&self.snapshot, &q.term),
            Mode::Commands => model::command_items(&self.commands, &q.term),
            Mode::Text => self.text_items(&q.term, cx),
            Mode::History => self.history_items(&q.term),
        };
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// Terminal für Terminal, solange das Limit nicht erreicht ist; erst ab zwei Zeichen.
    fn text_items(&self, term: &str, cx: &App) -> Vec<Item> {
        if term.chars().count() < model::MIN_TEXT_QUERY {
            return Vec::new();
        }
        let mut hits = Vec::new();
        let mut left = model::MAX_TEXT_HITS;
        for (_, s) in self.snapshot.groups.iter().flat_map(|g| g.sessions.iter().map(move |s| (g, s))) {
            let Some(pane) = self.panes.get(&s.id).filter(|_| left > 0) else { continue };
            let lines = pane.read(cx).search_lines(term, left);
            left -= lines.len();
            hits.push(SessionHits { id: s.id, lines });
        }
        model::text_items(&self.snapshot, &hits, term)
    }

    fn history_items(&self, term: &str) -> Vec<Item> {
        if !self.history.ready() {
            return model::history_placeholder(false);
        }
        let now = history::now_secs();
        self.history.search(term).iter().map(|h| model::history_item(h, now, self.snapshot.live.get(&h.session_id).copied())).collect()
    }

    fn move_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        let last = self.items.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        // Ein noch wartender Suchlauf darf ⏎ nicht auf veralteten Treffern ausführen.
        self.generation += 1;
        self.refilter(cx);
        self.activate(self.selected, window, cx);
    }

    fn activate(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.items.get(i) else { return };
        match item.target.clone() {
            Target::Session(id) => cx.emit(PaletteEvent::Session(id)),
            Target::Group(g) => cx.emit(PaletteEvent::Group(g)),
            Target::Command(c) => self.run_command(c, window, cx),
            Target::Hit { id, line, query } => cx.emit(PaletteEvent::Hit { id, line, query }),
            Target::History { session_id, cwd, title } => cx.emit(PaletteEvent::Resume(Pick { session_id, cwd, title })),
            Target::Nothing => {}
        }
    }

    fn run_command(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = self.commands.get(index) else { return };
        match &command.run {
            Run::Menu(action) => cx.emit(PaletteEvent::Menu(action.boxed_clone())),
            Run::Layout(mode) => cx.emit(PaletteEvent::Layout(*mode)),
            Run::Group(g) => cx.emit(PaletteEvent::Group(*g)),
            Run::MoveFocused(g) => cx.emit(PaletteEvent::MoveFocused(*g)),
            Run::Mode(mode) => self.set_input(mode.prefix(), window, cx),
            Run::Unwired => {}
        }
    }

    // --- Zeichnen ---

    fn dot(&self, item: &Item, t: &Theme) -> Option<gpui::Div> {
        let color = item.status?.color(t);
        Some(div().flex_none().size(px(8.)).rounded_full().bg(color))
    }

    fn label(&self, item: &Item, t: &Theme) -> impl IntoElement + use<> {
        let color = if item.dim { t.muted } else { t.fg };
        let mark = HighlightStyle {
            color: Some(t.fg),
            font_weight: Some(FontWeight::BOLD),
            background_color: Some(Hsla { a: 0.35, ..t.accent }),
            ..Default::default()
        };
        let text = StyledText::new(item.label.clone()).with_highlights(item.highlights.iter().map(|r| (r.clone(), mark)));
        div().text_size(px(12.5)).text_color(color).overflow_hidden().whitespace_nowrap().text_ellipsis().child(text)
    }

    fn row(&self, i: usize, item: &Item, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let on = i == self.selected;
        let outline = if on { t.fg } else { gpui::transparent_black() };
        let height = if item.compact { COMPACT_ROW_HEIGHT } else { ROW_HEIGHT };
        let mut text = div().flex().flex_col().flex_1().min_w_0().gap(px(2.)).child(self.label(item, t));
        if !item.sub.is_empty() {
            text = text.child(
                div().text_size(px(11.)).text_color(t.muted).overflow_hidden().whitespace_nowrap().text_ellipsis().child(item.sub.clone()),
            );
        }
        let right = item.right.clone().map(|r| div().flex_none().text_size(px(11.)).text_color(t.muted).child(r));
        let inner = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .size_full()
            .pl(px(10. + 16. * f32::from(item.indent)))
            .pr(px(10.))
            .rounded(px(6.))
            .border_2()
            .border_color(outline)
            .when(on, |d| d.bg(t.bg))
            .children(self.dot(item, t))
            .child(text)
            .children(right);
        div()
            .id(("palette-row", i))
            .flex_none()
            .h(px(height))
            .px(px(6.))
            .py(px(1.))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selected = i;
                this.activate(i, window, cx);
            }))
            .child(inner)
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows: Vec<_> = self.items.iter().enumerate().map(|(i, it)| self.row(i, it, t, cx)).collect();
        let empty = self.items.is_empty().then(|| div().px(px(16.)).py(px(14.)).text_color(t.muted).child("Keine Treffer"));
        div()
            .id("palette-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .py(px(4.))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(empty)
            .children(rows)
    }
}

fn foot(t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(h_seam(t))
        .child(
            div()
                .flex()
                .justify_end()
                .px(px(16.))
                .py(px(9.))
                .text_size(px(11.))
                .text_color(t.muted)
                .child("↑↓ wählen · ⏎ öffnen · Esc schließen"),
        )
}



impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let dialog = overlay_height(overlay_width(dialog_frame(&t), WIDTH), HEIGHT)
            .flex_none()
            .key_context(CONTEXT)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_by(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.move_by(1, cx)))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(PaletteEvent::Close)))
            .child(div().flex_none().p(px(12.)).child(self.input.clone()))
            .child(h_seam(&t))
            .child(self.list(&t, cx))
            .child(foot(&t));
        // Oben bei 12 % der Fensterhöhe statt mittig; die Abstandsfläche rechnet jeden Frame neu.
        backdrop(cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Close)))
            .flex_col()
            .justify_start()
            .child(div().flex_none().h(relative(TOP)))
            .child(dialog)
    }
}

impl Focusable for Palette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus.clone()
    }
}
