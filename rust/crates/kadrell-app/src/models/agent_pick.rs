//! „Starten mit“ (Swift: `UI/AgentPickView.swift`, `App/AppDelegate+AgentPick.swift`): Auswahl zwischen Claude und Pi vor
//! jeder neuen Session, sobald Pi nutzbar ist. Der zuletzt gewählte Agent steht oben und ist markiert, ⏎ startet ihn.
//! Tippen filtert unscharf über die Namen, ↑↓ wählt. Alle interaktiven Wege zu einer neuen Session laufen hier durch
//! (⌘N, ⌘⏎, Kontextmenüs, Schnellstart); `kadrell new`, Terminal, Remote und Fortsetzen starten direkt.

use gpui::{
    App, ClickEvent, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement,
    Render, StatefulInteractiveElement, Styled, Subscription, Window, actions, div, prelude::*, px,
};
use kadrell_core::{AgentKind, Settings};
use kadrell_i18n::trd;
use kadrell_models::types::display;

use crate::Shell;
use crate::lang::tx;
use crate::state::AppState;
use crate::theme::Theme;
use crate::ui::chrome::h_seam;
use crate::ui::instrument::selected_row;
use crate::ui::folders::{fuzzy, short_path};
use crate::ui::dialog_keys;
use crate::ui::instrument::KeyRole;
use crate::ui::overlay::{backdrop, dialog_foot_in, dialog_frame, dialog_title, overlay_width};
use crate::ui::text_input::{TextInput, TextInputEvent};
use crate::ui::theme;

actions!(kadrell_agent_pick, [MoveUp, MoveDown, Confirm, Cancel]);

const CONTEXT: &str = "KadrellAgentPick";
const ROW_HEIGHT: f32 = 36.;

/// Wie viele Zeichen einer Festbreiten-Schrift (Vorschub 0,6 em) in `width` Pixel passen.
pub fn max_chars(width: f32, font_px: f32) -> usize {
    (width / (font_px * 0.6)).floor().max(0.) as usize
}

/// Kürzt am Anfang mit „…“, wenn der Text nicht in `max` Zeichen passt (`truncationMode(.head)`).
pub fn truncate_head(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max || max == 0 {
        return text.to_string();
    }
    let keep: String = text.chars().skip(count - (max - 1)).collect();
    format!("…{keep}")
}

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    dialog_keys::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("escape", Cancel, c),
    ]);
}

/// Eine Zeile der Auswahl: der Agent und rechts Startflags (Claude) oder das lokale Modell (Pi).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickOption {
    pub kind: AgentKind,
    pub detail: String,
}

/// Zustand der Auswahl, ohne Oberfläche.
#[derive(Clone, Debug)]
pub struct AgentPickState {
    /// Ordner, in dem die Session startet.
    pub folder: String,
    options: Vec<PickOption>,
    pub query: String,
    pub list: Vec<PickOption>,
    pub selected: usize,
}

impl AgentPickState {
    pub fn new(folder: &str, options: Vec<PickOption>, last: AgentKind) -> AgentPickState {
        let options = order(options, last);
        AgentPickState { folder: folder.to_string(), list: options.clone(), options, query: String::new(), selected: 0 }
    }

    /// Nur über den Namen: die Startflags („bypassPermissions“) träfen sonst fast jede Buchstabenfolge.
    pub fn set_query(&mut self, query: &str) {
        if query == self.query {
            return;
        }
        self.query = query.to_string();
        let q = query.trim();
        self.list = self.options.iter().filter(|o| q.is_empty() || fuzzy(q, o.kind.display_name())).cloned().collect();
        self.selected = 0;
    }

    pub fn move_by(&mut self, delta: isize) {
        let last = self.list.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }

    /// Wählt die Zeile `i` und startet sie.
    pub fn pick(&mut self, i: usize) -> Option<AgentKind> {
        self.list.get(i)?;
        self.selected = i;
        self.start()
    }

    /// Der markierte Agent, `None` bei leerer Liste.
    pub fn start(&self) -> Option<AgentKind> {
        self.list.get(self.selected).map(|o| o.kind)
    }
}

/// Zuletzt gewählter Agent zuerst, die übrigen in ihrer festen Reihenfolge.
pub fn order(options: Vec<PickOption>, last: AgentKind) -> Vec<PickOption> {
    let (first, rest): (Vec<_>, Vec<_>) = options.into_iter().partition(|o| o.kind == last);
    first.into_iter().chain(rest).collect()
}

/// Kompakte Zeile mit den Startflags einer neuen Claude-Session (Swift: `Settings.claudeSummary`).
pub fn claude_summary(s: &Settings) -> String {
    let or = |value: String, fallback: String| if value.is_empty() { fallback } else { value };
    let mut parts = vec![or(s.claude_model(), trd!("Standardmodell")), or(s.claude_mode(), trd!("fragt nach Rechten"))];
    let effort = s.claude_effort();
    if !effort.is_empty() {
        parts.push(effort);
    }
    if s.claude_allow_bypass() {
        parts.push(trd!("Bypass"));
    }
    parts.join(" · ")
}

/// Zeile unter „Pi“: das lokale Modell oder Pis eigener Standard aus dessen Einstellungen.
pub fn pi_detail(model: &kadrell_claude::pi::PiModel, home: &str) -> String {
    match model.local() {
        Some(r) => r.name.clone(),
        None => kadrell_claude::pi::default_model(home).unwrap_or_else(|| trd!("Standard von Pi")),
    }
}

/// Die Zeilen der Auswahl für den Stand der App (Claude und Pi).
pub fn options_for(state: &AppState, pi: &kadrell_claude::pi::PiModel) -> Vec<PickOption> {
    vec![
        PickOption { kind: AgentKind::Claude, detail: claude_summary(&state.settings) },
        PickOption { kind: AgentKind::Pi, detail: pi_detail(pi, &state.home()) },
    ]
}

/// Anzeigetext des Pi-Modells für Meldungen.
pub fn pi_label(model: &kadrell_claude::pi::PiModel) -> String {
    model.local().map_or_else(|| "Pi".to_string(), display)
}

// MARK: Oberfläche

pub enum AgentPickEvent {
    Close,
    Start(AgentKind),
}

impl EventEmitter<AgentPickEvent> for AgentPick {}

pub struct AgentPick {
    input: Entity<TextInput>,
    state: AgentPickState,
    home: String,
    _typing: Subscription,
}

impl AgentPick {
    pub fn new(state: AgentPickState, home: &str, window: &mut Window, cx: &mut Context<Self>) -> AgentPick {
        let input = cx.new(|cx| TextInput::new(&trd!("Tippen filtert"), cx));
        let typing = cx.subscribe(&input, |this, input, _: &TextInputEvent, cx| {
            let text = input.read(cx).text().to_string();
            this.state.set_query(&text);
            cx.notify();
        });
        let focus = input.read(cx).focus.clone();
        focus.focus(window, cx);
        AgentPick { input, state, home: home.to_string(), _typing: typing }
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(kind) = self.state.start() {
            cx.emit(AgentPickEvent::Start(kind));
        }
    }

    fn row(&self, i: usize, o: &PickOption, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let on = i == self.state.selected;
        let s = crate::ui::overlay::scale();
        // Name 12, Detail 11 gedämpft rechts; Auswahl als Instrument-Zeile (Fläche `bg`, 1,5-pt-Umriss in `fg`, kein Ring).
        let inner = selected_row(div().flex().items_center().gap(px(10. * s)).size_full().px(px(10. * s)), on, t)
            .child(div().flex_none().text_size(px(12. * s)).text_color(t.fg).child(o.kind.display_name()))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_align(gpui::TextAlign::Right).text_size(px(11. * s)).text_color(t.muted).child(o.detail.clone()));
        let kind = o.kind;
        div().id(("agent-row", i)).flex_none().h(px(ROW_HEIGHT * s)).px(px(6. * s)).py(px(s)).cursor_pointer().child(inner).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(AgentPickEvent::Start(kind))))
    }
}

impl Render for AgentPick {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let s = crate::ui::overlay::scale();
        // Eine Zeile, Kürzung am Anfang (`truncationMode(.head)`): GPUI kürzt nur am Ende, deshalb nach Zeichenzahl.
        let room = crate::ui::overlay::area().dialog_width(440.) - 32. * s;
        let path = truncate_head(&short_path(&self.state.folder, &self.home), max_chars(room, 11. * s));
        let folder = div().flex_none().px(px(16. * s)).pt(px(4. * s)).text_size(px(11. * s)).text_color(t.muted).overflow_hidden().whitespace_nowrap().child(path);
        let rows: Vec<_> = self.state.list.iter().enumerate().map(|(i, o)| self.row(i, o, &t, cx)).collect();
        let empty = self.state.list.is_empty().then(|| div().px(px(16.)).py(px(16.)).text_color(t.muted).child(tx(&trd!("Nichts gefunden."))));
        let hint = trd!("⏎ starten · ↑↓ wählt · Esc abbrechen");
        let on_foot = cx.listener(|this, _, w, cx| this.confirm(&Confirm, w, cx));
        let foot = dialog_foot_in(hint, trd!("Starten"), KeyRole::Primary, &t, window, cx, on_foot);
        let dialog = overlay_width(dialog_frame(&t), 440.)
            .key_context(CONTEXT)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.state.move_by(-1);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.state.move_by(1);
                cx.notify();
            }))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(AgentPickEvent::Close)))
            .child(dialog_title(trd!("STARTEN MIT"), &t))
            .child(folder)
            .child(div().flex_none().p(px(14.)).child(self.input.clone()))
            .child(h_seam(&t))
            .child(div().flex().flex_col().pb(px(4.)).children(empty).children(rows))
            .child(foot);
        dialog_keys::nav(backdrop(cx.listener(|_, _, _, cx| cx.emit(AgentPickEvent::Close)))).child(dialog)
    }
}

impl Focusable for AgentPick {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus.clone()
    }
}

// MARK: Anbindung an die Shell

impl Shell {
    /// Neue Session im Ordner `cwd`: mit nutzbarem Pi erst die Auswahl „Starten mit“, sonst startet Claude sofort wie
    /// bisher (Swift: `startNewSession`).
    pub fn start_new_session(&mut self, group: Option<String>, cwd: &str, prompt: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let cwd = cwd.replace('\\', "/");
        let Some(pi) = self.state.pi_model() else {
            return self.start_agent(AgentKind::Claude, group, &cwd, prompt, window, cx);
        };
        let state = AgentPickState::new(&cwd, options_for(&self.state, &pi), self.state.settings.last_agent());
        if state.list.first().is_some_and(|o| o.kind == AgentKind::Pi) {
            self.state.models.warm_up(kadrell_models::Feature::PiSession);
        }
        let home = self.state.home();
        let dialog = cx.new(|cx| AgentPick::new(state, &home, window, cx));
        cx.subscribe_in(&dialog, window, move |this, _, ev: &AgentPickEvent, window, cx| {
            this.agent_pick = None;
            match ev {
                AgentPickEvent::Close => this.sync(window, cx),
                AgentPickEvent::Start(kind) => {
                    this.state.set_last_agent(*kind);
                    this.start_agent(*kind, group.clone(), &cwd, prompt.clone(), window, cx);
                }
            }
        })
        .detach();
        self.agent_pick = Some(dialog);
        cx.notify();
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=agentpick` zeigt die Auswahl im Ordner `KADRELL_DEV_QUERY` (sonst
    /// im Startordner), auch ohne installiertes Pi.
    pub fn open_agent_pick_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() != Ok("agentpick") {
            return;
        }
        let folder = std::env::var("KADRELL_DEV_QUERY").unwrap_or_else(|_| self.state.home());
        let pi = self.state.pi_model().unwrap_or(kadrell_claude::pi::PiModel::PiDefault);
        let state = AgentPickState::new(&folder, options_for(&self.state, &pi), self.state.settings.last_agent());
        let home = self.state.home();
        self.agent_pick = Some(cx.new(|cx| AgentPick::new(state, &home, window, cx)));
        cx.notify();
    }

    fn start_agent(&mut self, kind: AgentKind, group: Option<String>, cwd: &str, prompt: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        match kind {
            AgentKind::Claude => {
                let key = kadrell_core::Session::fresh(cwd, crate::state::now_ms());
                self.state.add_session(key, group.as_deref(), true, prompt);
            }
            AgentKind::Pi => {
                if let Some(model) = self.state.pi_model() {
                    self.state.new_pi_session(cwd, group.as_deref(), true, prompt, &model);
                }
            }
        }
        self.play_open_sound();
        self.sync(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_folder_lines_are_cut_at_the_head() {
        assert_eq!(truncate_head("~/a/b", 10), "~/a/b");
        assert_eq!(truncate_head("~/development/projects/kadrell", 12), "…cts/kadrell");
        assert_eq!(truncate_head("abc", 0), "abc");
        assert_eq!(max_chars(400., 10.), 66);
    }

    fn options() -> Vec<PickOption> {
        vec![PickOption { kind: AgentKind::Claude, detail: "sonnet · bypassPermissions · high".into() }, PickOption { kind: AgentKind::Pi, detail: "qwen3:4b".into() }]
    }

    #[test]
    fn last_choice_comes_first_and_is_selected() {
        let s = AgentPickState::new("/p", options(), AgentKind::Pi);
        assert_eq!(s.list.iter().map(|o| o.kind).collect::<Vec<_>>(), [AgentKind::Pi, AgentKind::Claude]);
        assert_eq!((s.selected, s.start()), (0, Some(AgentKind::Pi)));
        let claude = AgentPickState::new("/p", options(), AgentKind::Claude);
        assert_eq!(claude.start(), Some(AgentKind::Claude), "die feste Reihenfolge bleibt, wenn der letzte vorn steht");
        assert_eq!(claude.list[1].kind, AgentKind::Pi);
    }

    #[test]
    fn typing_filters_by_name_only() {
        let mut s = AgentPickState::new("/p", options(), AgentKind::Claude);
        s.set_query("p");
        assert_eq!(s.list.iter().map(|o| o.kind).collect::<Vec<_>>(), [AgentKind::Pi]);
        s.set_query("bypass");
        assert!(s.list.is_empty(), "Startflags zählen nicht");
        assert_eq!(s.start(), None);
        s.set_query("  ");
        assert_eq!(s.list.len(), 2);
        s.set_query("CL");
        assert_eq!(s.start(), Some(AgentKind::Claude));
    }

    #[test]
    fn selection_is_clamped_and_filtering_resets_it() {
        let mut s = AgentPickState::new("/p", options(), AgentKind::Claude);
        s.move_by(5);
        assert_eq!(s.selected, 1);
        s.move_by(-9);
        assert_eq!(s.selected, 0);
        s.move_by(1);
        s.set_query("c");
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn picking_a_row_starts_it_and_ignores_missing_rows() {
        let mut s = AgentPickState::new("/p", options(), AgentKind::Claude);
        assert_eq!(s.pick(1), Some(AgentKind::Pi));
        assert_eq!(s.selected, 1);
        assert_eq!(s.pick(7), None);
    }

    #[test]
    fn claude_summary_lists_flags_in_order() {
        let mut s = Settings::default();
        assert_eq!(claude_summary(&s), "Standardmodell · fragt nach Rechten");
        s.set_claude_model("sonnet");
        s.set_claude_effort("high");
        let summary = claude_summary(&s);
        assert!(summary.starts_with("sonnet · "), "{summary}");
        assert!(summary.contains("high"));
    }

    #[test]
    fn pi_detail_names_the_model_or_pi_default() {
        use kadrell_claude::pi::{LocalProviderKind, PiModel};
        let local = PiModel::Local(kadrell_claude::pi::LocalModelRef { provider: LocalProviderKind::Ollama, name: "qwen3:4b".into() });
        assert_eq!(pi_detail(&local, "/gibt/es/nicht"), "qwen3:4b");
        assert_eq!(pi_detail(&PiModel::PiDefault, "/gibt/es/nicht"), "Standard von Pi");
        assert_eq!(pi_label(&local), "Ollama · qwen3:4b");
    }
}
