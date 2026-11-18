//! Dialog „NEUE SESSION“ wie `NewSessionSheet.swift`: Suchfeld, Kandidatenliste, Startzeile. Kandidaten sind die
//! Ordner der Gruppen plus Git-Repos unter `~/development` (Hintergrund-Scan); getippt wird per Wortsuche oder als
//! Pfadkürzel (`~/d/p/kad`). Dazu die Anbindung an `Shell` (Öffnen, Schließen, Session anlegen).

use std::collections::{HashMap, HashSet};

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window,
    actions, div, prelude::*, px,
};

use super::chrome::h_seam;
use super::folder_row::{FolderRow, folder_row};
use super::folders::{self, Entry};
use super::model::Group;
use super::overlay::{backdrop, dialog_foot, dialog_frame, dialog_title, overlay_height, overlay_width};
use super::text_input::{TextInput, TextInputEvent};
use super::theme;
use crate::Shell;
use crate::theme::Theme;

actions!(kadrell_new_session, [MoveUp, MoveDown, Confirm, Cancel, Complete]);

const CONTEXT: &str = "KadrellNewSession";
const MAX_ROWS: usize = 60;
const SCAN_DEPTH: usize = 3;
const GROUP_COLORS: [u32; 6] = [0x89b4fa, 0xa6e3a1, 0xf9e2af, 0xf5c2e7, 0x94e2d5, 0xfab387];

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("escape", Cancel, c),
        KeyBinding::new("tab", Complete, c),
    ]);
}

pub enum NewSessionEvent {
    Close,
    Start(String),
}

impl EventEmitter<NewSessionEvent> for NewSession {}

pub struct NewSession {
    input: Entity<TextInput>,
    home: String,
    /// Kleingeschriebener Pfad zu Anzahl der Sessions dort.
    known: HashMap<String, usize>,
    base: Vec<Entry>,
    repo_count: usize,
    scanning: bool,
    shown: Vec<Entry>,
    selected: usize,
    scroll: ScrollHandle,
    _typing: Subscription,
}

/// Ordner einer Gruppe, mit `/`; `None` bei der Sammelgruppe ohne Gruppierung.
pub fn group_cwd(g: &Group) -> Option<String> {
    (!g.cwd.is_empty()).then(|| g.cwd.replace('\\', "/"))
}

fn folder_name(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_string()
}

impl NewSession {
    pub fn new(groups: &[Group], query: Option<&str>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new("Projekt suchen oder Pfad tippen (~/d/p/kad)", cx));
        let typing = cx.subscribe(&input, |this, _, _: &TextInputEvent, cx| this.refilter(cx));
        let mut known = HashMap::new();
        let mut base = Vec::new();
        for g in groups {
            if let Some(cwd) = group_cwd(g) {
                *known.entry(cwd.to_lowercase()).or_insert(0) += g.sessions.len();
            }
        }
        let mut seen = HashSet::new();
        for g in groups {
            let Some(path) = group_cwd(g) else { continue };
            if seen.insert(path.to_lowercase()) {
                base.push(Entry { sessions: known[&path.to_lowercase()], path });
            }
        }
        let mut this = Self {
            input,
            home: folders::home_dir(),
            known,
            base,
            repo_count: 0,
            scanning: true,
            shown: vec![],
            selected: 0,
            scroll: ScrollHandle::new(),
            _typing: typing,
        };
        if let Some(q) = query {
            this.input.update(cx, |i, cx| i.set_text(q, window, cx));
        }
        this.refilter(cx);
        this.start_scan(cx);
        let focus = this.input.read(cx).focus.clone();
        focus.focus(window, cx);
        this
    }

    fn start_scan(&self, cx: &mut Context<Self>) {
        let roots = scan_roots(&self.home);
        cx.spawn(async move |this, cx| {
            let repos = cx.background_executor().spawn(async move { roots.iter().flat_map(|r| folders::scan_repos(r, SCAN_DEPTH)).collect::<Vec<_>>() }).await;
            let _ = this.update(cx, |this, cx| this.add_repos(repos, cx));
        })
        .detach();
    }

    /// Scan fertig: neue Repos hinter die Gruppen-Ordner, doppelte (auch in anderer Schreibweise) weglassen.
    fn add_repos(&mut self, mut repos: Vec<String>, cx: &mut Context<Self>) {
        repos.sort_by_key(|p| p.to_lowercase());
        let mut seen: HashSet<String> = self.base.iter().map(|e| e.path.to_lowercase()).collect();
        for p in repos {
            if seen.insert(p.to_lowercase()) {
                self.repo_count += 1;
                self.base.push(Entry { sessions: self.sessions_at(&p), path: p });
            }
        }
        self.scanning = false;
        if self.selected == 0 {
            self.refilter(cx);
        }
        cx.notify();
    }

    fn sessions_at(&self, path: &str) -> usize {
        self.known.get(&path.to_lowercase()).copied().unwrap_or(0)
    }

    fn query(&self, cx: &App) -> String {
        self.input.read(cx).text().trim().to_string()
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let q = self.query(cx);
        self.shown = if folders::is_path_query(&q) {
            let found = folders::expand_abbreviated(&q, &self.home, MAX_ROWS, &folders::list_dirs);
            found.into_iter().map(|p| Entry { sessions: self.sessions_at(&p), path: p }).collect()
        } else {
            folders::filter(&self.base, &q, MAX_ROWS)
        };
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    fn move_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        let last = self.shown.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    /// Ziel von Enter: die markierte Zeile, sonst ein ausgeschrieben getippter vorhandener Ordner.
    fn target(&self, cx: &App) -> Option<String> {
        if let Some(e) = self.shown.get(self.selected) {
            return Some(e.path.clone());
        }
        let dir = folders::normalize(&self.query(cx), &self.home);
        (!dir.is_empty() && std::path::Path::new(&dir).is_dir()).then_some(dir)
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.target(cx) {
            cx.emit(NewSessionEvent::Start(path));
        }
    }

    fn complete(&mut self, _: &Complete, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.shown.get(self.selected) else { return };
        let short = folders::short_path(&e.path, &self.home);
        self.input.update(cx, |i, cx| i.set_text(&short, window, cx));
    }

    fn status_line(&self, t: &Theme) -> Option<gpui::Div> {
        let text = if self.scanning {
            "Durchsuche Git-Repos unter ~ …".to_string()
        } else {
            format!("{} Git-Repos unter ~ gefunden", self.repo_count)
        };
        let line = div().flex_none().px(px(16.)).pt(px(8.)).pb(px(2.)).text_size(px(10.)).text_color(t.muted);
        Some(line.child(text))
    }

    fn row(&self, i: usize, e: &Entry, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let on = i == self.selected;
        let path = e.path.clone();
        let right = if e.sessions > 0 { vec![format!("{} Sessions", e.sessions)] } else { Vec::new() };
        let row = FolderRow { name: folder_name(&e.path), path: folders::short_path(&e.path, &self.home), right };
        folder_row(("ns-row", i), row, on, t, cx.listener(move |_, _, _, cx| cx.emit(NewSessionEvent::Start(path.clone()))))
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows: Vec<_> = self.shown.iter().enumerate().map(|(i, e)| self.row(i, e, t, cx)).collect();
        let empty = self.shown.is_empty().then(|| {
            div().px(px(16.)).py(px(16.)).text_color(t.muted).child(format!("Kein Ordner gefunden. {}+O wählt im Explorer.", mod_name()))
        });
        div()
            .id("ns-list")
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .children(self.status_line(t))
            .children(empty)
            .children(rows)
    }
}

fn mod_name() -> &'static str {
    if cfg!(target_os = "macos") { "⌘" } else { "Strg" }
}

fn starts_with_line(t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .px(px(16.))
        .py(px(6.))
        .text_size(px(10.5))
        .text_color(t.muted)
        .child("Startet mit: sonnet · bypassPermissions · high")
        .child(div().id("ns-change").cursor_pointer().text_color(t.accent).hover(|s| s.underline()).child("ändern"))
}

impl Render for NewSession {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let hint = format!("⏎ starten · Tab ergänzt · {}+O Explorer · Ordner hineinziehen", mod_name());
        let foot = dialog_foot(hint, "Starten ⏎", &t, cx.listener(|this, _, w, cx| this.confirm(&Confirm, w, cx)));
        let dialog = overlay_height(overlay_width(dialog_frame(&t), 560.), 600.)
            .key_context(CONTEXT)
            .text_color(t.fg)
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_by(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.move_by(1, cx)))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::complete))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(NewSessionEvent::Close)))
            .child(dialog_title("NEUE SESSION", &t))
            .child(div().flex_none().p(px(14.)).child(self.input.clone()))
            .child(h_seam(&t))
            .child(self.list(&t, cx))
            .child(h_seam(&t))
            .child(starts_with_line(&t))
            .child(foot);
        backdrop(cx.listener(|_, _, _, cx| cx.emit(NewSessionEvent::Close))).child(dialog)
    }
}

impl Focusable for NewSession {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus.clone()
    }
}

// Anbindung an die Shell.

impl Shell {
    /// Aktion `NewSession` (Menü, Strg+N / ⌘N): öffnet den Dialog, solange er nicht schon offen ist.
    pub fn new_session_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var_os("KADRELL_DEV_KEYLOG").is_some() {
            eprintln!("action NewSession (Dialog offen: {})", self.overlay.is_some());
        }
        if self.overlay.is_none() {
            self.open_new_session(None, window, cx);
        }
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=new`, Suchtext aus `KADRELL_DEV_QUERY`.
    pub fn open_overlay_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() == Ok("new") {
            let query = std::env::var("KADRELL_DEV_QUERY").ok();
            self.open_new_session(query.as_deref(), window, cx);
        }
    }

    pub fn open_new_session(&mut self, query: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| NewSession::new(&self.groups, query, window, cx));
        cx.subscribe_in(&dialog, window, |this, _, ev: &NewSessionEvent, window, cx| match ev {
            NewSessionEvent::Close => this.close_overlay(window, cx),
            NewSessionEvent::Start(path) => {
                this.close_overlay(window, cx);
                this.start_in(path, window, cx);
            }
        })
        .detach();
        self.overlay = Some(dialog);
        cx.notify();
    }

    /// Schließt den Dialog; der Fokus geht zurück an das Terminal der gewählten Session.
    fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        self.sync(window, cx);
    }

    /// Neue Claude-Session in `path`: in die Gruppe mit diesem Ordner, sonst in eine neue Gruppe; dann wählen.
    fn start_in(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.state.new_claude_session(path, None, true);
        self.sync(window, cx);
    }

    /// Ordner der fokussierten Session (Strg+Enter, Strg+T): die Gruppe, sonst der Startordner der Einstellungen.
    fn focused_folder(&self) -> String {
        let focused = self.state.focused.as_deref().and_then(|k| self.state.session(k));
        match focused {
            Some(s) => s.cwd.clone(),
            None => self.state.settings.start_folder(&self.state.paths.home).trim_end_matches('/').to_string(),
        }
    }

    /// Strg+Enter / ⌘↩: neue Claude-Session im Ordner der fokussierten, ohne Dialog.
    pub fn new_session_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cwd = self.focused_folder();
        let group = self.state.focused.as_deref().and_then(|k| self.state.store.groups.group_for_session(k)).map(|g| g.id.clone());
        self.state.new_claude_session(&cwd, group.as_deref(), true);
        self.sync(window, cx);
    }

    /// Strg+T / ⌘T: Terminal ohne Claude im Ordner der fokussierten Session.
    pub fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cwd = self.focused_folder();
        self.state.new_shell_session(&cwd, true);
        self.sync(window, cx);
    }
}

/// Wurzeln des Repo-Scans: `~/development`, unter Windows zusätzlich `C:/development`, falls es dort liegt.
pub(crate) fn scan_roots(home: &str) -> Vec<String> {
    let mut roots = vec![format!("{home}/development")];
    if cfg!(windows) && !home.to_lowercase().starts_with("c:/development") {
        roots.push("C:/development".to_string());
    }
    roots
}
