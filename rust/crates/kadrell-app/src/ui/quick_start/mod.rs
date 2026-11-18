//! Schnellstart (⌘⇧I / Strg+Umschalt+I) wie `QuickStartView.swift`: Idee eintippen, Kadrell erkennt das Projekt am Namen
//! im Text und zeigt es vorausgewählt, ⏎ startet dort eine Session mit der Idee als erstem Prompt. ↑↓ oder Tippen wählt ein
//! anderes Projekt, Tab schaltet zwischen Originaltext und Text ohne Projektverweis. Ablauf und Regeln: `QuickStartFlow` im
//! Kern, Texte und Tastenwirkung in `model`.
//!
//! Ohne lokales Modell: die Rust-App hat noch keinen Ollama-/llama.cpp-Client (`kadrell_claude::local_model::LocalModel` ist
//! nur das Trait), also gilt immer der Weg ohne Modell, die Namenssuche. TODO: „Starten mit“ (Claude oder Pi), sobald Pi
//! verdrahtet ist (`pi_available`).

mod model;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding, ParentElement,
    Render, ScrollHandle, Styled, Subscription, Window, actions, div, prelude::*, px,
};
use kadrell_claude::folder_index::FolderIndex;
use kadrell_claude::quick_start::{Cancelled, GroupInfo, Phase, QuickStartCandidate, QuickStartFlow, SessionInfo};
use kadrell_core::Session;

use self::model::{Confirmed, Inputs};
use super::chrome::h_seam;
use super::folder_row::{FolderRow, ROW_HEIGHT, folder_row};
use super::folders;
use super::overlay::{backdrop, dialog_foot, dialog_frame, dialog_title, overlay_height, overlay_width};
use super::palette::relative_de;
use super::text_input::{TextInput, TextInputEvent};
use super::theme;
use crate::Shell;
use crate::state::{AppState, now_secs};
use crate::theme::Theme;

actions!(kadrell_quick_start, [MoveUp, MoveDown, Confirm, Cancel, ToggleCleaned]);

const CONTEXT: &str = "KadrellQuickStart";
const WIDTH: f32 = 640.;
const VISIBLE_ROWS: f32 = 6.;

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("escape", Cancel, c),
        KeyBinding::new("tab", ToggleCleaned, c),
    ]);
}

pub enum QuickStartEvent {
    Close,
    /// Session in `path` starten (in der Gruppe `group_id`, falls das Projekt eine hat) mit `prompt` als erster Nachricht.
    Start { path: String, group_id: Option<String>, prompt: String },
}

impl EventEmitter<QuickStartEvent> for QuickStart {}

pub struct QuickStart {
    flow: QuickStartFlow,
    idea: Entity<TextInput>,
    filter: Entity<TextInput>,
    inputs: Inputs,
    /// Der Index, solange der Repo-Scan ihn nicht gerade im Hintergrund hat.
    index: Option<FolderIndex>,
    scanning: bool,
    error: Option<String>,
    /// Nur für Screenshots: nach dem Scan gleich ⏎ (`KADRELL_DEV_SUBMIT`).
    submit_after_scan: bool,
    scroll: ScrollHandle,
    _subs: [Subscription; 2],
}

impl QuickStart {
    pub fn new(inputs: Inputs, index: FolderIndex, roots: Vec<String>, idea: Option<&str>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let candidates = model::candidates(&inputs, &index, now_secs());
        let flow = QuickStartFlow::new(candidates, None, &inputs.home);
        let idea_input = cx.new(|cx| TextInput::new(&model::idea_placeholder(), cx));
        let filter_input = cx.new(|cx| TextInput::new(&model::filter_placeholder(), cx));
        let typing_idea = cx.subscribe(&idea_input, |this, input, _: &TextInputEvent, cx| this.idea_typed(&input, cx));
        let typing_filter = cx.subscribe(&filter_input, |this, input, _: &TextInputEvent, cx| this.filter_typed(&input, cx));
        let mut this = Self {
            flow,
            idea: idea_input,
            filter: filter_input,
            inputs,
            index: Some(index),
            scanning: true,
            error: None,
            submit_after_scan: std::env::var_os("KADRELL_DEV_SUBMIT").is_some(),
            scroll: ScrollHandle::new(),
            _subs: [typing_idea, typing_filter],
        };
        if let Some(text) = idea {
            this.idea.update(cx, |i, cx| i.set_text(text, window, cx));
        }
        this.start_scan(roots, cx);
        this.focus_input(window, cx);
        this
    }

    /// Repos neben bekannten Projekten nachladen wie bei Strg+N; bis der Scan steht, gilt der gespeicherte Stand.
    fn start_scan(&mut self, roots: Vec<String>, cx: &mut Context<Self>) {
        let Some(mut index) = self.index.take() else { return };
        cx.spawn(async move |this, cx| {
            let index = cx
                .background_executor()
                .spawn(async move {
                    index.refresh(&roots);
                    index
                })
                .await;
            let _ = this.update(cx, |this, cx| this.scan_done(index, cx));
        })
        .detach();
    }

    fn scan_done(&mut self, index: FolderIndex, cx: &mut Context<Self>) {
        self.flow.update_candidates(model::candidates(&self.inputs, &index, now_secs()));
        self.index = Some(index);
        self.scanning = false;
        if self.submit_after_scan {
            self.submit_after_scan = false;
            let home = self.inputs.home.clone();
            let outcome = model::confirm(&mut self.flow, &home);
            self.apply(outcome, None, cx);
        }
        cx.notify();
    }

    fn idea_typed(&mut self, input: &Entity<TextInput>, cx: &mut Context<Self>) {
        let text = input.read(cx).text().to_string();
        self.flow.set_idea(&text);
        cx.notify();
    }

    fn filter_typed(&mut self, input: &Entity<TextInput>, cx: &mut Context<Self>) {
        let text = input.read(cx).text().to_string();
        self.flow.set_filter(&text);
        self.error = None;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    /// Das Feld der Phase bekommt die Tastatur.
    fn focus_input(&self, window: &mut Window, cx: &mut Context<Self>) {
        let field = if self.flow.phase == Phase::Pick { &self.filter } else { &self.idea };
        let focus = field.read(cx).focus.clone();
        focus.focus(window, cx);
    }

    /// Wirkung von ⏎ oder Klick umsetzen.
    fn apply(&mut self, outcome: Confirmed, window: Option<&mut Window>, cx: &mut Context<Self>) {
        match outcome {
            Confirmed::Nothing => {}
            Confirmed::ShowPick => {
                self.error = None;
                self.scroll.scroll_to_item(0);
                if let Some(window) = window {
                    self.focus_input(window, cx);
                }
            }
            Confirmed::Missing(message) => self.error = Some(message),
            Confirmed::Start { path, group_id, prompt } => cx.emit(QuickStartEvent::Start { path, group_id, prompt }),
        }
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, window: &mut Window, cx: &mut Context<Self>) {
        let outcome = model::confirm(&mut self.flow, &self.inputs.home);
        self.apply(outcome, Some(window), cx);
    }

    fn pick(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let outcome = model::pick(&mut self.flow, i, &self.inputs.home);
        self.apply(outcome, Some(window), cx);
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        match model::cancel(&mut self.flow) {
            Cancelled::Close => cx.emit(QuickStartEvent::Close),
            Cancelled::Back => {
                self.error = None;
                self.filter.update(cx, |f, cx| f.set_text("", window, cx));
                self.focus_input(window, cx);
                cx.notify();
            }
        }
    }

    fn move_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.flow.phase != Phase::Pick {
            return;
        }
        self.flow.move_selection(delta);
        self.scroll.scroll_to_item(self.flow.selected);
        cx.notify();
    }

    fn toggle_cleaned(&mut self, _: &ToggleCleaned, _: &mut Window, cx: &mut Context<Self>) {
        if self.flow.phase == Phase::Pick && self.flow.toggle_cleaned() {
            cx.notify();
        }
    }

    // --- Zeichnen ---

    fn idea_phase(&self, t: &Theme) -> gpui::Div {
        let status = if self.flow.phase == Phase::Thinking {
            model::thinking_text(self.flow.model_name.as_deref().unwrap_or_default())
        } else {
            model::live_text(&self.flow, &self.inputs.home)
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(14.))
            .child(self.idea.clone())
            .child(div().text_size(px(11.)).text_color(t.muted).overflow_hidden().whitespace_nowrap().text_ellipsis().child(status))
    }

    fn prompt_block(&self, t: &Theme) -> gpui::Div {
        let mut head = div().flex().items_center().gap(px(8.)).child(div().text_size(px(11.)).text_color(t.muted).child(model::first_prompt_label()));
        if self.flow.cleaned.is_some() {
            head = head.child(div().text_size(px(10.5)).text_color(t.accent).child(model::cleaned_label(self.flow.use_cleaned)));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .px(px(16.))
            .pt(px(12.))
            .child(head)
            .child(div().max_h(px(48.)).overflow_hidden().text_color(t.fg).child(self.flow.prompt()))
    }

    fn origin_line(&self, t: &Theme) -> gpui::Div {
        let (text, color) = match (&self.error, &self.flow.origin) {
            (Some(e), _) => (e.clone(), t.error),
            (None, Some(o)) => (model::origin_text(o), t.muted),
            (None, None) => (String::new(), t.muted),
        };
        div().px(px(16.)).pb(px(6.)).text_size(px(10.5)).text_color(color).overflow_hidden().whitespace_nowrap().text_ellipsis().child(text)
    }

    fn row(&self, i: usize, c: &QuickStartCandidate, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut right = vec![model::source_text(c)];
        if c.last_used > 0.0 {
            right.push(relative_de((now_secs() - c.last_used).max(0.0) as u64));
        }
        let row = FolderRow { name: c.name.clone(), path: folders::short_path(&c.path, &self.inputs.home), right };
        folder_row(("qs-row", i), row, i == self.flow.selected, t, cx.listener(move |this, _, window, cx| this.pick(i, window, cx)))
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows: Vec<_> = self.flow.list.iter().enumerate().map(|(i, c)| self.row(i, c, t, cx)).collect();
        let empty = rows.is_empty().then(|| div().px(px(16.)).py(px(16.)).text_color(t.muted).child(model::empty_list()));
        div()
            .id("qs-list")
            .flex()
            .flex_col()
            .flex_none()
            .h(px(VISIBLE_ROWS * ROW_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(empty)
            .children(rows)
    }

    fn pick_phase(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .child(self.prompt_block(t))
            .child(div().flex_none().p(px(14.)).child(self.filter.clone()))
            .child(self.origin_line(t))
            .child(h_seam(t))
            .child(self.list(t, cx))
    }
}

impl Render for QuickStart {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let phase = self.flow.phase;
        let foot = dialog_foot(model::hint(phase), model::button(phase), &t, cx.listener(|this, _, w, cx| this.confirm(&Confirm, w, cx)));
        let body = if phase == Phase::Pick { self.pick_phase(&t, cx) } else { self.idea_phase(&t) };
        let dialog = overlay_height(overlay_width(dialog_frame(&t), WIDTH), 640.)
            .flex_none()
            .key_context(CONTEXT)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_by(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.move_by(1, cx)))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(Self::toggle_cleaned))
            .child(dialog_title(model::title(), &t))
            .child(body)
            .child(foot);
        backdrop(cx.listener(|_, _, _, cx| cx.emit(QuickStartEvent::Close))).child(dialog)
    }
}

impl Focusable for QuickStart {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        let field = if self.flow.phase == Phase::Pick { &self.filter } else { &self.idea };
        field.read(cx).focus.clone()
    }
}

// Anbindung an die Shell.

/// Gruppen und Sessions (offene und geschlossene) für den Kandidatenindex.
fn inputs_of(state: &AppState) -> Inputs {
    let groups = state
        .store
        .groups
        .groups
        .iter()
        .map(|g| GroupInfo { id: g.id.clone(), name: g.name.clone(), cwd: g.cwd.clone(), host: g.host.clone() })
        .collect();
    let info = |s: &Session| SessionInfo { cwd: s.cwd.clone(), started_at_ms: s.started_at, is_remote: s.is_remote(), is_shell: s.is_shell(), title: s.title() };
    let open = state.store.sessions.sessions.iter().map(info);
    let closed = state.store.sessions.closed.iter().map(|c| info(&c.session));
    Inputs { groups, sessions: open.chain(closed).collect(), home: state.home() }
}

/// Wurzeln des Repo-Scans: der Startordner, die Ordner über bekannten Projekten und die Wurzeln von Strg+N (unter Windows
/// liegen Projekte oft außerhalb von `~`).
fn scan_roots(state: &AppState, index: &FolderIndex, inputs: &Inputs) -> Vec<String> {
    let mut roots = super::new_session::scan_roots(&inputs.home);
    roots.push(state.settings.start_folder(&state.paths.home).trim_end_matches('/').to_string());
    let known = inputs.groups.iter().map(|g| g.cwd.clone()).chain(index.uses.keys().cloned());
    roots.extend(known.filter_map(|p| p.trim_end_matches('/').rsplit_once('/').map(|(parent, _)| parent.to_string())).filter(|p| !p.is_empty()));
    roots.sort();
    roots.dedup();
    roots
}

impl Shell {
    /// Aktion `QuickStart` (Menü, Strg+Umschalt+I): öffnet den Dialog, solange kein anderes Overlay offen ist.
    pub fn open_quick_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quick_start.is_some() || self.overlay.is_some() || self.palette.is_some() || self.settings.is_some() {
            return;
        }
        self.show_quick_start(None, window, cx);
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=quickstart`, Idee aus `KADRELL_DEV_QUERY`, mit `KADRELL_DEV_SUBMIT`
    /// gleich ⏎ nach dem Repo-Scan.
    pub fn open_quick_start_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() == Ok("quickstart") {
            let idea = std::env::var("KADRELL_DEV_QUERY").ok();
            self.show_quick_start(idea.as_deref(), window, cx);
        }
    }

    fn show_quick_start(&mut self, idea: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let inputs = inputs_of(&self.state);
        let index = FolderIndex::load(&self.state.profile.dir.to_string_lossy().replace('\\', "/"), &inputs.home);
        let roots = scan_roots(&self.state, &index, &inputs);
        let dialog = cx.new(|cx| QuickStart::new(inputs, index, roots, idea, window, cx));
        cx.subscribe_in(&dialog, window, |this, _, ev: &QuickStartEvent, window, cx| match ev {
            QuickStartEvent::Close => this.close_quick_start(window, cx),
            QuickStartEvent::Start { path, group_id, prompt } => {
                this.close_quick_start(window, cx);
                this.start_quick(path, group_id.as_deref(), prompt, window, cx);
            }
        })
        .detach();
        self.quick_start = Some(dialog);
        cx.notify();
    }

    /// Schließt den Dialog; die Tastatur geht zurück an das Terminal der gewählten Session.
    fn close_quick_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quick_start = None;
        self.sync(window, cx);
    }

    /// Neue Claude-Session im Ordner des Projekts mit der Idee als erstem Prompt (`startNewSession(…, prompt:)`).
    fn start_quick(&mut self, path: &str, group_id: Option<&str>, prompt: &str, window: &mut Window, cx: &mut Context<Self>) {
        // Der Ordner wird beim Anlegen der Session als benutzt gemerkt (`AppState::add_session`).
        let prompt = (!prompt.is_empty()).then(|| prompt.to_string());
        self.start_new_session(group_id.map(String::from), path, prompt, window, cx);
    }
}
