//! Statistik (⌥F3 / Alt+F3) wie `StatsView.swift`, `StatsModel.swift` und `StatsCharts.swift`: Limits je Konto als
//! Linien, Prompts im Zeitverlauf als gestapelte Balken mit Projektliste, darunter die Lebenszeit-Zähler.
//! 1 bis 4 oder ←/→ wählen den Zeitraum, ↑/↓, Leertaste, ⌫ und s bedienen die Projektliste, Esc, ⏎ oder das Kürzel
//! schließen. Die Reihen rechnet `kadrell_accounts::StatsSnapshot` auf dem Hintergrund-Executor; solange das Fenster
//! offen ist, zieht die Seite alle 15 Sekunden nach.

mod draw;
pub mod format;
mod sections;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent, ParentElement, Render,
    ScrollHandle, Styled, Task, Window, div, prelude::*, px,
};
use kadrell_accounts::{PromptEvent, PromptSource, PromptStats, ProjectSort, StatsInput, StatsListAction, StatsRange, StatsSnapshot, StatsView as Keys, UsageSample};
use kadrell_claude::history::SessionHistory;
use kadrell_core::Settings;
use kadrell_core::stats::{self, StatKey};
use kadrell_i18n::trd;

use super::chrome::h_seam;
use super::overlay::{backdrop_full, dialog_frame};
use super::overlay_anim::slide;
use super::settings::controls::Geometry;
use super::theme;
use crate::state::now_secs;
use crate::usage::hub::Hub;
use draw::Hover;

pub const CONTEXT: &str = "KadrellStats";
/// Wie oft die Seite nachzieht, solange sie offen ist.
const REFRESH: Duration = Duration::from_secs(15);

pub enum StatsEvent {
    Close,
}

impl EventEmitter<StatsEvent> for StatsView {}

/// Die Lebenszeit-Zähler des Profils (`Stats` in Swift, hier `kadrell_core::stats`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counters {
    pub messages: i64,
    pub sessions: i64,
    pub terminals: i64,
    pub switches: i64,
    pub record: i64,
    pub record_date: Option<f64>,
    pub since: Option<f64>,
}

impl Counters {
    pub fn from_settings(s: &Settings) -> Counters {
        let (record, record_date) = stats::record(s);
        Counters {
            messages: stats::count(s, StatKey::Messages),
            sessions: stats::count(s, StatKey::Sessions),
            terminals: stats::count(s, StatKey::Terminals),
            switches: stats::count(s, StatKey::FocusSwitches),
            record,
            record_date,
            since: stats::since(s),
        }
    }
}

/// Woher die Seite ihre Daten holt. Alles hier blockiert und läuft auf dem Hintergrund-Executor.
#[derive(Clone)]
pub struct Loader {
    pub hub: Arc<Hub>,
    pub config_dir: String,
    pub cache_file: String,
    pub settings_path: PathBuf,
    pub history: Arc<Mutex<SessionHistory>>,
    /// Fußhinweis zum Schließen, aus dem Kürzel der Statistik (`Hotkeys.closeHint(.showStats)`).
    pub close_hint: String,
}

impl Loader {
    /// Alle Eingaben aus den Transcripts (bei erfundenen Daten die erfundenen). Ein fortgesetztes Transcript kann
    /// ältere Zeilen kopieren, `PromptStats::events` zählt jeden Zeitstempel nur einmal.
    fn events(&self) -> Vec<PromptEvent> {
        if let Some(fake) = self.hub.fake_events() {
            return fake;
        }
        let mut history = self.history.lock().unwrap_or_else(PoisonError::into_inner);
        history.refresh(&self.config_dir, &self.cache_file);
        let sources: Vec<PromptSource> = history.sessions().iter().map(|s| PromptSource { cwd: s.cwd.clone(), prompt_times: s.prompt_times.clone() }).collect();
        PromptStats::events(&sources)
    }

    fn counters(&self) -> Counters {
        Counters::from_settings(&Settings::load(&self.settings_path))
    }
}

pub struct StatsView {
    focus: FocusHandle,
    loader: Loader,
    pub(crate) range: StatsRange,
    filter: BTreeSet<String>,
    sort: ProjectSort,
    cursor: Option<String>,
    snapshot: Option<Arc<StatsSnapshot>>,
    /// Die Transcripts wurden seit dem Öffnen noch nicht gelesen (beim ersten Mal dauert das einige Sekunden).
    reading: bool,
    events: Arc<Vec<PromptEvent>>,
    samples: Arc<Vec<UsageSample>>,
    counters: Counters,
    hover: Option<Hover>,
    generation: u64,
    list_scroll: ScrollHandle,
    /// Gemessene Breiten der Zähler (eine Reihe oder zwei Spalten wie `ViewThatFits`).
    geometry: Geometry,
    counters_basis: Option<(f32, f32)>,
    _task: Option<Task<()>>,
}

impl StatsView {
    pub fn new(loader: Loader, counters: Counters, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut view = StatsView {
            focus,
            loader,
            range: StatsRange::Week,
            filter: BTreeSet::new(),
            sort: ProjectSort::Count,
            cursor: None,
            snapshot: None,
            reading: true,
            events: Arc::new(Vec::new()),
            samples: Arc::new(Vec::new()),
            counters,
            hover: None,
            generation: 0,
            list_scroll: ScrollHandle::new(),
            geometry: Geometry::default(),
            counters_basis: None,
            _task: None,
        };
        view._task = Some(view.start(cx));
        view
    }

    /// Erst die Limits zeigen: das erste Lesen der Transcripts kann dauern, und solange bleibt die Seite nicht leer.
    fn start(&self, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(loader) = this.update(cx, |s, _| s.loader.clone()) else { break };
                let l = loader.clone();
                let samples = cx.background_executor().spawn(async move { (l.hub.samples(), l.counters()) }).await;
                let ok = this.update(cx, |s, cx| {
                    s.samples = Arc::new(samples.0);
                    s.counters = samples.1;
                    s.rebuild(cx);
                });
                let events = cx.background_executor().spawn(async move { loader.events() }).await;
                let ok2 = this.update(cx, |s, cx| {
                    s.events = Arc::new(events);
                    s.reading = false;
                    s.rebuild(cx);
                });
                if ok.is_err() || ok2.is_err() {
                    break;
                }
                cx.background_executor().timer(REFRESH).await;
            }
        })
    }

    /// Rechnet auf dem Hintergrund-Executor. Hat sich unterdessen etwas geändert, gilt nur noch der neuere Lauf.
    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let mine = self.generation;
        let (range, filter, sort) = (self.range, self.filter.clone(), self.sort);
        let (samples, events) = (self.samples.clone(), self.events.clone());
        let sources = self.loader.hub.view().sources;
        let lang = ui_lang();
        cx.spawn(async move |this, cx| {
            let snapshot = cx
                .background_executor()
                .spawn(async move { StatsSnapshot::make(range, now_secs(), &samples, &sources, &events, &filter, sort, &format::cal(lang)) })
                .await;
            let _ = this.update(cx, |s, cx| {
                if s.generation == mine {
                    s.snapshot = Some(Arc::new(snapshot));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn set_range(&mut self, range: StatsRange, cx: &mut Context<Self>) {
        if range != self.range {
            self.range = range;
            self.rebuild(cx);
            cx.notify();
        }
    }

    pub fn toggle(&mut self, project: &str, cx: &mut Context<Self>) {
        if !self.filter.remove(project) {
            self.filter.insert(project.to_string());
        }
        self.rebuild(cx);
    }

    fn project_names(&self) -> Vec<String> {
        self.snapshot.as_ref().map(|s| s.projects.iter().map(|p| p.name.clone()).collect()).unwrap_or_default()
    }

    pub fn apply(&mut self, action: StatsListAction, cx: &mut Context<Self>) {
        match action {
            StatsListAction::Up | StatsListAction::Down => {
                let names = self.project_names();
                self.cursor = Keys::moved(self.cursor.as_deref(), &names, action == StatsListAction::Down);
                if let Some(i) = self.cursor.as_ref().and_then(|c| names.iter().position(|n| n == c)) {
                    self.list_scroll.scroll_to_item(i);
                }
            }
            StatsListAction::Toggle => {
                if let Some(c) = self.cursor.clone() {
                    self.toggle(&c, cx);
                }
            }
            StatsListAction::Sort => {
                self.sort = self.sort.next();
                self.rebuild(cx);
            }
            StatsListAction::Clear => {
                self.filter.clear();
                self.rebuild(cx);
            }
        }
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        if m.control || m.alt || m.platform {
            return;
        }
        let key = ev.keystroke.key.as_str();
        if matches!(key, "escape" | "enter") {
            cx.emit(StatsEvent::Close);
            return;
        }
        let Some(input) = input_of(key) else { return };
        if let Some(next) = Keys::range_after(self.range, input) {
            self.set_range(next, cx);
        } else if let Some(action) = StatsListAction::from_input(input) {
            self.apply(action, cx);
        }
        cx.stop_propagation();
    }

    pub(crate) fn set_hover(&mut self, hover: Option<Hover>, cx: &mut Context<Self>) {
        if self.hover != hover {
            self.hover = hover;
            cx.notify();
        }
    }
}

/// Taste der Seite als Eingabe der Tastenlogik in `kadrell-accounts`.
pub fn input_of(key: &str) -> Option<StatsInput> {
    match key {
        "left" => Some(StatsInput::Left),
        "right" => Some(StatsInput::Right),
        "up" => Some(StatsInput::Up),
        "down" => Some(StatsInput::Down),
        "backspace" | "delete" => Some(StatsInput::Delete),
        "space" => Some(StatsInput::Char(' ')),
        k if k.chars().count() == 1 => k.chars().next().map(StatsInput::Char),
        _ => None,
    }
}

/// Sprache der Konten-Texte aus der der Oberfläche.
pub fn ui_lang() -> kadrell_accounts::Lang {
    crate::usage::lang()
}

impl Focusable for StatsView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for StatsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let lang = ui_lang();
        let one_row = self.counters_in_one_row(window);
        let body = div()
            .id("stats-body")
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_y_scroll()
            .p(px(16.))
            .gap(px(20.))
            .children(self.snapshot.clone().map(|s| self.limits_section(&s, window, cx)))
            .children(self.snapshot.clone().map(|s| self.prompts_section(&s, window, cx)))
            .child(self.counters_section(lang, one_row, cx));
        let dialog = dialog_frame(&t)
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .text_color(t.fg)
            .child(self.header(cx))
            .child(h_seam(&t))
            .child(body)
            .child(h_seam(&t))
            .child(self.footer(cx));
        let travel = super::overlay::area().slide_travel();
        let reduce = crate::appearance::breathing::reduce_motion();
        // Die Fläche schmiegt sich an den Inhalt (Fußzeile direkt darunter, wie in Swift); `slide` füllt nur den Rahmen.
        let hug = div().flex().flex_col().size_full().child(dialog.max_h_full());
        backdrop_full(cx.listener(|_, _, _, cx| cx.emit(StatsEvent::Close))).child(slide(hug, "stats-slide", travel, reduce))
    }
}

/// Hinweis zum Schließen mit dem gebundenen Kürzel der Statistik, ohne Belegung nur „Esc schließt“.
pub fn close_hint(settings: &Settings, mac: bool) -> String {
    match crate::ui::ext::keys::binding(settings, kadrell_core::hotkeys::HotkeyAction::ShowStats, mac) {
        Some(k) => trd!("Esc oder %@ schließen", crate::ui::ext::keys::label(&k, mac)),
        None => trd!("Esc schließt"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_hint_names_the_bound_shortcut_or_none() {
        let mut s = Settings::default();
        assert!(close_hint(&s, false).contains("Alt+F3"));
        assert!(close_hint(&s, true).contains("F3"));
        crate::ui::settings::shortcut::assign(&mut s, kadrell_core::hotkeys::HotkeyAction::ShowStats, None);
        assert_eq!(close_hint(&s, false), trd!("Esc schließt"));
    }

    #[test]
    fn keys_map_to_inputs() {
        assert_eq!(input_of("left"), Some(StatsInput::Left));
        assert_eq!(input_of("space"), Some(StatsInput::Char(' ')));
        assert_eq!(input_of("backspace"), Some(StatsInput::Delete));
        assert_eq!(input_of("3"), Some(StatsInput::Char('3')));
        assert_eq!(input_of("f3"), None);
    }

    #[test]
    fn digits_pick_the_range_and_arrows_step() {
        assert_eq!(Keys::range_after(StatsRange::Week, StatsInput::Char('4')), Some(StatsRange::Year));
        assert_eq!(Keys::range_after(StatsRange::Year, StatsInput::Right), Some(StatsRange::Year));
        assert_eq!(Keys::range_after(StatsRange::Day, StatsInput::Left), Some(StatsRange::Day));
        assert_eq!(Keys::range_after(StatsRange::Week, StatsInput::Char('s')), None);
    }

    #[test]
    fn counters_read_the_profile_settings() {
        let mut s = Settings::default();
        stats::bump(&mut s, StatKey::Messages, 5, 1_800_000_000.);
        stats::note_concurrent(&mut s, 7, 1_800_000_000.);
        let c = Counters::from_settings(&s);
        assert_eq!((c.messages, c.record), (5, 7));
        assert!(c.since.is_some() && c.record_date.is_some());
    }
}
