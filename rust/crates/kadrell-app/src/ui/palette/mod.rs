//! Suche (⌘P / Strg+P) wie `PaletteWindow.swift`: ein Feld, darunter die Trefferliste. Ohne Präfix springt sie zu
//! Sessions und Gruppen, `>` führt Kommandos aus, `/` sucht Text in allen laufenden Terminals, `#` durchsucht den
//! Verlauf aller Claude-Sessions. Logik in `model`, Verlaufsdaten hinter `history::HistoryProvider`, die Anbindung
//! an die Shell in `shell`.

mod history;
mod resume;
mod model;
mod remote;
mod shell;

use kadrell_i18n::trd;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, Render, Role, ScrollHandle, StatefulInteractiveElement, Styled,
    Subscription, Window, actions, div, prelude::*, px,
};

use kadrell_term::TerminalPane;

use self::history::HistoryProvider;
pub use self::history::{HistoryStore, provider as history_provider};
pub use self::resume::{HistoryAction, Known, Pick, decide as decide_history, problem_title};
pub use self::model::{AccountRun, Command, Run};
use self::model::{Item, Mode, SessionHits, Snapshot, Target};
use self::remote::HostSource;
use crate::remote::dialog::{Connect, RemoteState, Sessions};
use super::chrome::h_seam;
use super::layout::{Id, LayoutMode};
use super::overlay::{self, backdrop, dialog_frame};
use super::text_input::{TextInput, TextInputEvent};
use super::theme;
use crate::theme::Theme;

actions!(kadrell_palette, [MoveUp, MoveDown, Confirm, Cancel]);

const CONTEXT: &str = "KadrellPalette";
const ROW_HEIGHT: f32 = 44.;
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
    /// Eintrag einer Extension (Statusleiste mit Aktion): `ui.action` an die Extension.
    Ext { name: String, id: String },
    /// ⏎ im `@`-Modus: mit dem Host verbinden.
    Connect(Connect),
    ResetRatios,
    Reload,
    Account(AccountRun),
    /// Sessions, die die Trefferliste gerade hervorhebt (`onHighlight`); `None` hebt nichts hervor.
    Highlight(Option<HashSet<Id>>),
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
    /// Hosts des `@`-Modus, erst beim ersten `@` gelesen; `remote` behält die tmux-Sessions je Host für diese Öffnung.
    hosts: HostSource,
    remote: Option<RemoteState>,
    /// Zuletzt gemeldete Hervorhebung (`PaletteEvent::Highlight`).
    highlight: Option<HashSet<Id>>,
    _typing: Subscription,
}

impl Palette {
    pub fn new(
        snapshot: Snapshot,
        panes: HashMap<Id, Entity<TerminalPane>>,
        history: Rc<dyn HistoryProvider>,
        hosts: HostSource,
        initial: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hint = trd!("Session, Gruppe, Pfad suchen …  ( > Kommandos, / in allen Terminals, # alle Claude-Sessions, @ Remote )");
        let input = cx.new(|cx| TextInput::new(&hint, cx).large());
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
            hosts,
            remote: None,
            highlight: None,
            _typing: typing,
        };
        this.input.update(cx, |i, cx| i.set_text(initial, window, cx));
        this.refilter(cx);
        let focus = this.input.read(cx).focus.clone();
        focus.focus(window, cx);
        this
    }

    /// Hängt Kommandos an die Liste von `>` (Einträge der Extensions, siehe `model::Run::Ext`).
    pub fn add_commands(&mut self, extra: Vec<Command>, cx: &mut Context<Self>) {
        self.commands.extend(extra);
        self.refilter(cx);
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
            Mode::Remote => self.remote_items(&q.term, cx),
        };
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        self.report_highlight(&q, cx);
        cx.notify();
    }

    /// Meldet der Shell, welche Sessions die Liste gerade zeigt (nur bei Änderung).
    fn report_highlight(&mut self, q: &model::Query, cx: &mut Context<Self>) {
        let now = model::highlight(q, &self.items);
        if now != self.highlight {
            self.highlight = now.clone();
            cx.emit(PaletteEvent::Highlight(now));
        }
    }

    /// Hervorhebung der Trefferliste beim Öffnen (die Shell liest sie, weil die Meldung vor dem Abonnieren fällt).
    pub fn highlight(&self) -> Option<HashSet<Id>> {
        self.highlight.clone()
    }

    /// Strg+P bei offener Palette ohne Präfix: wechselt in den Kommandomodus, ohne schon Getipptes zu löschen, wenn
    /// sie bereits dort ist (`switchToCommandMode`).
    pub fn switch_to_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.raw(cx).starts_with('>') {
            self.set_input(">", window, cx);
        }
    }

    /// `@`: Hosts, mit `host:` die tmux-Sessions des Hosts (einmal je Host und Öffnung geholt, bis dahin „lädt …“).
    fn remote_items(&mut self, term: &str, cx: &mut Context<Self>) -> Vec<Item> {
        let hosts = self.hosts.clone();
        let state = self.remote.get_or_insert_with(|| remote::state_from(&hosts));
        state.set_query(term);
        let wanted = state.wanted_sessions();
        let items = remote::items(state);
        if let Some(host) = wanted {
            self.load_sessions(host, cx);
        }
        items
    }

    fn load_sessions(&mut self, host: String, cx: &mut Context<Self>) {
        if let Some(state) = self.remote.as_mut() {
            state.sessions.insert(host.clone(), Sessions::Loading);
        }
        cx.spawn(async move |this, cx| {
            let ask = host.clone();
            let found = cx.background_executor().spawn(async move { crate::remote::tmux_sessions(&crate::remote::system_ssh(), &ask) }).await;
            let _ = this.update(cx, |this, cx| this.sessions_loaded(host, found, cx));
        })
        .detach();
    }

    /// Die Antwort von `tmux ls` ist da: in den Zustand und, wenn der `@`-Modus noch offen ist, die Liste neu bauen.
    fn sessions_loaded(&mut self, host: String, found: Option<Vec<String>>, cx: &mut Context<Self>) {
        if let Some(state) = self.remote.as_mut() {
            state.sessions.insert(host, found.map_or(Sessions::Unreachable, Sessions::Found));
        }
        if model::parse(&self.raw(cx)).mode == Mode::Remote {
            self.refilter(cx);
        }
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
            Target::Connect(connect) => cx.emit(PaletteEvent::Connect(connect)),
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
            Run::Ext { name, id } => cx.emit(PaletteEvent::Ext { name: name.clone(), id: id.clone() }),
            Run::Mode(mode) => self.set_input(mode.prefix(), window, cx),
            Run::ResetRatios => cx.emit(PaletteEvent::ResetRatios),
            Run::Reload => cx.emit(PaletteEvent::Reload),
            Run::Account(run) => cx.emit(PaletteEvent::Account(run.clone())),
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
        div().h(px(16.)).line_height(px(16.)).text_size(px(12.5)).text_color(color).overflow_hidden().whitespace_nowrap().text_ellipsis().child(item.label.clone())
    }

    fn sub(&self, item: &Item, t: &Theme) -> impl IntoElement + use<> {
        div().h(px(14.)).line_height(px(14.)).text_size(px(11.)).text_color(t.muted).overflow_hidden().whitespace_nowrap().text_ellipsis().child(item.sub.clone())
    }

    /// Zeile wie `PaletteRow.drawRow` (Maße relativ zur 44 pt hohen Zeile; der 1,5 pt breite Umriss und der Rand von
    /// 6/1 pt sind schon abgezogen): Punkt bei y 13, Titel bei y 7, Zusatzzeile bei y 24, rechts Gruppe bei y 8.
    fn row(&self, i: usize, item: &Item, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let on = i == self.selected;
        let outline = if on { t.fg } else { gpui::transparent_black() };
        let text = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .mt(px(4.5))
            .gap(px(1.))
            .child(self.label(item, t))
            .child(self.sub(item, t));
        let right = item.right.clone().map(|r| div().flex_none().mt(px(5.5)).h(px(14.)).line_height(px(14.)).text_size(px(11.)).text_color(t.muted).child(r));
        let inner = div()
            .flex()
            .items_start()
            .gap(px(10.))
            .size_full()
            .pl(px(8.5 + 16. * f32::from(item.indent)))
            .pr(px(8.5))
            .rounded(px(6.))
            .border(px(1.5))
            .border_color(outline)
            .when(on, |d| d.bg(t.bg))
            .children(self.dot(item, t).map(|d| d.mt(px(10.5))))
            .child(text)
            .children(right);
        div()
            .id(("palette-row", i))
            .role(Role::Label)
            .aria_label(item.spoken())
            .when(on, |d| d.aria_active_descendant())
            .flex_none()
            .h(px(ROW_HEIGHT))
            .px(px(6.))
            .py(px(1.))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| {
                this.selected = i;
                this.activate(i, window, cx);
            }))
            .child(inner)
    }

    fn empty_row(&self, t: &Theme) -> impl IntoElement + use<> {
        div()
            .id("palette-empty")
            .role(Role::Label)
            .aria_label(trd!("Keine Treffer"))
            .px(px(16.))
            .pt(px(14.))
            .text_size(px(12.))
            .text_color(t.muted)
            .child(trd!("Keine Treffer"))
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows: Vec<_> = self.items.iter().enumerate().map(|(i, it)| self.row(i, it, t, cx)).collect();
        let empty = self.items.is_empty().then(|| self.empty_row(t));
        div()
            .id("palette-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
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
                .items_center()
                .justify_end()
                .h(px(34.))
                .px(px(16.))
                .text_size(px(11.))
                .text_color(t.muted)
                .child(trd!("↑↓ wählen · ⏎ öffnen · Esc schließen")),
        )
}



impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        // Größe und Lage aus der Fläche, jeden Frame neu (`PaletteWindow.follow`): oben bei 12 %, mittig, im kleinen
        // Fenster Fläche minus Rand. Die Fensterhöhe enthält die Schattentiefe unter der Fläche.
        let frame = overlay::area().palette();
        let surface = frame.height - (overlay::SHADOW_DEPTH * overlay::scale()).round();
        let dialog = dialog_frame(&t)
            .w(px(frame.width))
            .h(px(surface))
            .flex_none()
            .key_context(CONTEXT)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_by(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.move_by(1, cx)))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(PaletteEvent::Close)))
            .child(div().flex_none().px(px(10.)).pt(px(16.)).pb(px(6.)).child(self.input.clone()))
            .child(h_seam(&t))
            .child(self.list(&t, cx))
            .child(foot(&t));
        // Oben bei 12 % der Fensterhöhe statt mittig; die Abstandsfläche rechnet jeden Frame neu.
        backdrop(cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Close)))
            .flex_col()
            .justify_start()
            .child(div().flex_none().h(px(frame.top)))
            .child(dialog)
    }
}

impl Focusable for Palette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus.clone()
    }
}
