//! Dialog „NEUE SESSION“ wie `NewSessionSheet.swift`: Suchfeld, Kandidatenliste, Startzeile. Kandidaten, Rang, „Ordner
//! anlegen“ und die Wirkung von ⏎ stehen im Modell (`model`), hier die Oberfläche und die Anbindung an `Shell` (Öffnen,
//! Schließen, Session anlegen, Extension-Einträge, Einstellungen).

mod model;

use kadrell_claude::folder_index::{self as fi, FolderIndex};
use kadrell_i18n::trd;

use crate::lang::{file_manager, tx};
use crate::state::now_secs;

use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyBinding,
    ParentElement, Render, ScrollHandle, StatefulInteractiveElement, Styled, Subscription, Window,
    actions, div, prelude::*, px, relative,
};

use self::model::{Candidate, GroupInfo, Inputs, Model, Outcome, Select};
use super::chrome::h_seam;
use super::folder_row::{FolderRow, folder_row_titled};
use super::groups::files;
use super::dialog_keys;
use super::instrument::KeyRole;
use super::overlay::{backdrop, dialog_foot_in, dialog_frame, dialog_title, overlay_width};
use super::text_input::{TextInput, TextInputEvent};
use super::theme;
use crate::Shell;
use crate::theme::Theme;

actions!(kadrell_new_session, [MoveUp, MoveDown, Confirm, Cancel, Complete, Browse, CreateAndStart]);

const CONTEXT: &str = "KadrellNewSession";
/// Breite des Dialogs (`dialogFrame` der Mac-App).
const WIDTH: f32 = 640.;
/// Sichtbare Zeilen der Liste: 8 mal 46 (`visibleRows` mal `rowHeight`).
const VISIBLE_ROWS: f32 = 8.;

/// Höhe der Liste mal UI-Größe (`visibleRows * rowHeight * Theme.scale`).
fn list_height(scale: f32) -> f32 {
    VISIBLE_ROWS * super::folder_row::ROW_HEIGHT * scale
}

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    dialog_keys::bind_keys(cx);
    let (browse, create) = (crate::keys::platform("cmd-o"), crate::keys::platform("cmd-enter"));
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("enter", Confirm, c),
        KeyBinding::new("escape", Cancel, c),
        KeyBinding::new("tab", Complete, c),
        KeyBinding::new(browse, Browse, c),
        KeyBinding::new(create, CreateAndStart, c),
    ]);
}

pub enum NewSessionEvent {
    Close,
    Start { group: Option<String>, path: String },
    /// Eintrag einer Extension (Repo zum Klonen): der Dialog schließt, die Extension übernimmt.
    Extension { name: String, id: String },
    /// „ändern“ in der Zeile „Startet mit“.
    OpenSettings,
}

impl EventEmitter<NewSessionEvent> for NewSession {}

pub struct NewSession {
    input: Entity<TextInput>,
    model: Model,
    summary: String,
    scroll: ScrollHandle,
    _typing: Subscription,
}

/// Was `NewSession::new` zum Aufbau braucht.
pub struct Setup {
    pub inputs: Inputs,
    pub index: FolderIndex,
    /// Wurzeln des Repo-Scans im Hintergrund.
    pub roots: Vec<String>,
    /// Zusammenfassung der Startflags für „Startet mit“.
    pub summary: String,
    pub query: Option<String>,
}

impl NewSession {
    pub fn new(setup: Setup, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(&trd!("Projekt suchen oder Pfad tippen (~/d/p/kad)"), cx).path_field());
        let typing = cx.subscribe(&input, |this, _, _: &TextInputEvent, cx| this.query_changed(cx));
        let home = setup.inputs.home.clone();
        let clipboard = clipboard_folder(cx, &home);
        let mut model = Model::new(setup.inputs, setup.index, clipboard);
        model.scanning = true;
        let this = Self { input, model, summary: setup.summary, scroll: ScrollHandle::new(), _typing: typing };
        if let Some(q) = setup.query {
            this.input.update(cx, |i, cx| i.set_text(&q, window, cx));
        }
        this.start_scan(setup.roots, cx);
        this.ask_file_manager(home, cx);
        let focus = this.input.read(cx).focus.clone();
        focus.focus(window, cx);
        this
    }

    /// Repo-Scan im Hintergrund auf einer Kopie des Index; fertig wird der Stand übernommen (und `folders.json` ist neu).
    fn start_scan(&self, roots: Vec<String>, cx: &mut Context<Self>) {
        let mut index = self.model.index().clone();
        cx.spawn(async move |this, cx| {
            let repos = cx
                .background_executor()
                .spawn(async move {
                    index.refresh(&roots);
                    index.repos
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.model.set_repos(repos);
                this.scroll_to_selected();
                cx.notify();
            });
        })
        .detach();
    }

    /// Ordner des vordersten Dateimanager-Fensters, asynchron: kommt er, steht er vorn, solange nichts getippt ist.
    fn ask_file_manager(&self, home: String, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let folder = cx.background_executor().spawn(async move { fi::finder_folder(&home) }).await;
            if let Some(folder) = folder {
                let _ = this.update(cx, |this, cx| {
                    this.model.set_finder(&folder);
                    this.scroll_to_selected();
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn query_changed(&mut self, cx: &mut Context<Self>) {
        let q = self.input.read(cx).text().to_string();
        self.model.set_query(&q);
        self.scroll_to_selected();
        cx.notify();
    }

    /// Die Liste beginnt mit einem Kopf (Hinweise), danach folgen die Zeilen.
    fn scroll_to_selected(&self) {
        let at = self.model.selected;
        self.scroll.scroll_to_item(if at == 0 { 0 } else { at + 1 });
    }

    fn move_by(&mut self, delta: isize, cx: &mut Context<Self>) {
        self.model.move_by(delta);
        self.scroll_to_selected();
        cx.notify();
    }

    fn apply(&mut self, outcome: Outcome, cx: &mut Context<Self>) {
        match outcome {
            Outcome::Nothing => {}
            Outcome::Beep => kadrell_tray::beep(),
            Outcome::Start { group, path } => cx.emit(NewSessionEvent::Start { group, path }),
            Outcome::Extension { name, id } => cx.emit(NewSessionEvent::Extension { name, id }),
        }
        cx.notify();
    }

    fn confirm(&mut self, _: &Confirm, _: &mut Window, cx: &mut Context<Self>) {
        let outcome = self.model.start();
        self.apply(outcome, cx);
    }

    fn create_and_start(&mut self, _: &CreateAndStart, _: &mut Window, cx: &mut Context<Self>) {
        let outcome = self.model.create_and_start();
        self.apply(outcome, cx);
    }

    fn pick(&mut self, i: usize, cx: &mut Context<Self>) {
        let outcome = self.model.pick(i);
        self.apply(outcome, cx);
    }

    fn complete(&mut self, _: &Complete, window: &mut Window, cx: &mut Context<Self>) {
        // Tab im Suchfeld vervollständigt; steht der Fokus woanders (Haupttaste, „ändern“), geht Tab weiter.
        if !self.input.read(cx).focus.is_focused(window) {
            return window.focus_next(cx);
        }
        if let Some(short) = self.model.tab() {
            self.input.update(cx, |i, cx| i.set_text(&short, window, cx));
        } else {
            window.focus_next(cx);
        }
    }

    /// ⌘O / Strg+O: Ordnerdialog des Systems; der gewählte Ordner startet sofort. GPUI kennt keinen Startordner des
    /// Dialogs, `pick::choose_in` setzt ihn unter Windows selbst, am Mac bleibt es beim Systemverhalten.
    /// Dialogs, er öffnet beim markierten Eintrag, sonst im Startordner (`browse()`).
    fn browse(&mut self, _: &Browse, window: &mut Window, cx: &mut Context<Self>) {
        let start = self.model.browse_start();
        super::pick::choose_in(Some(&start), false, window, cx, |_, path, _, cx| {
            if let Some(dir) = files::folder_for(std::path::Path::new(&path)) {
                cx.emit(NewSessionEvent::Start { group: None, path: dir });
            }
        });
    }

    // MARK: Zeichnen

    /// Hinweise über den Zeilen: Scan läuft, Anlegen gescheitert, Ordner existiert nicht, nichts gefunden.
    fn header(&self, t: &Theme) -> gpui::Div {
        let line = |text: String, color| div().w_full().px(px(16.)).py(px(16.)).text_color(color).child(text);
        let mut head = div().flex().flex_col().flex_none();
        if self.model.scanning {
            let scan = div().w_full().px(px(16.)).pt(px(8.)).text_size(px(10.)).text_color(t.muted).child(trd!("Suche Git-Repos unter ~ …"));
            head = head.child(scan);
        }
        if let Some(error) = &self.model.create_error {
            head = head.child(line(trd!("Ordner nicht angelegt: %@", error), t.error_text));
        }
        match (&self.model.not_found_path, self.model.candidates.is_empty()) {
            (Some(bad), true) => head.child(line(trd!("%@ existiert nicht · anlegen geht nur im Home-Ordner", self.model.short(bad)), t.error_text)),
            (None, true) => head.child(line(tx(&trd!("Kein Ordner gefunden. ⌘O wählt im %@.", file_manager())), t.muted)),
            _ => head,
        }
    }

    fn row(&self, i: usize, c: &Candidate, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let right = self.model.right(c);
        let row = FolderRow { name: self.model.title(c), path: self.model.detail(c), right: if right.is_empty() { vec![] } else { vec![right] } };
        let color = if c.is_create() { t.accent_text } else { t.fg };
        folder_row_titled(("ns-row", i), row, i == self.model.selected, color, t, cx.listener(move |this, _, _, cx| this.pick(i, cx)))
    }

    fn list(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows: Vec<_> = self.model.candidates.iter().enumerate().map(|(i, c)| self.row(i, c, t, cx)).collect();
        div()
            .id("ns-list")
            .flex()
            .flex_col()
            .flex_none()
            .h(px(list_height(super::overlay::scale())))
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(self.header(t))
            .children(rows)
    }

    fn starts_with_line(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let change = div()
            .id("ns-change")
            .cursor_pointer()
            .text_color(t.accent_text)
            .hover(|s| s.underline())
            .on_click(cx.listener(|_, _, _, cx| cx.emit(NewSessionEvent::OpenSettings)))
            .child(trd!("ändern"));
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .gap(px(6.))
            .px(px(16.))
            .py(px(6.))
            .text_size(px(10.5))
            .text_color(t.muted)
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().child(trd!("Startet mit: %@", self.summary.as_str())))
            .child(change)
    }
}

impl Render for NewSession {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let hint = tx(&trd!("⏎ starten · Tab übernehmen · ⌘O Finder · Ordner hineinziehen").replace("Finder", file_manager()));
        let on_foot = cx.listener(|this, _, w, cx| this.confirm(&Confirm, w, cx));
        let foot = dialog_foot_in(hint, format!("{}  ⏎", trd!("Starten")), KeyRole::Primary, &t, window, cx, on_foot);
        let ring = t.accent;
        let dialog = overlay_width(dialog_frame(&t), WIDTH)
            .max_h(relative(0.86))
            .key_context(CONTEXT)
            .text_color(t.fg)
            .text_size(px(12.))
            .on_action(cx.listener(|this, _: &MoveUp, _, cx| this.move_by(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveDown, _, cx| this.move_by(1, cx)))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(Self::complete))
            .on_action(cx.listener(Self::browse))
            .on_action(cx.listener(Self::create_and_start))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(NewSessionEvent::Close)))
            // Ordner aus dem Explorer/Finder hineingezogen: sofort dort starten.
            .on_drop(cx.listener(|_, paths: &gpui::ExternalPaths, _, cx| {
                if let Some(dir) = files::first_folder(paths) {
                    cx.emit(NewSessionEvent::Start { group: None, path: dir });
                }
            }))
            // Beim Darüberziehen 2 pt Akzentrand ums ganze Fenster des Dialogs.
            .drag_over::<gpui::ExternalPaths>(move |s, _, _, _| s.border_2().border_color(ring))
            .child(dialog_title(trd!("NEUE SESSION"), &t))
            .child(div().flex_none().p(px(14.)).child(self.input.clone()))
            .child(h_seam(&t))
            .child(self.list(&t, cx))
            .child(h_seam(&t))
            .child(self.starts_with_line(&t, cx))
            .child(foot);
        dialog_keys::nav(backdrop(cx.listener(|_, _, _, cx| cx.emit(NewSessionEvent::Close)))).child(dialog)
    }
}

impl Focusable for NewSession {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus.clone()
    }
}

/// Ordner aus der Zwischenablage: eine Datei oder ein Ordner aus dem Dateimanager, sonst ein Pfad als Text
/// (Swift: `FolderIndex.clipboardFolder`).
fn clipboard_folder(cx: &App, home: &str) -> Option<String> {
    let item = cx.read_from_clipboard()?;
    let dropped = item.entries().iter().find_map(|e| match e {
        gpui::ClipboardEntry::ExternalPaths(paths) => files::first_folder(paths),
        _ => None,
    });
    dropped.or_else(|| item.text().and_then(|text| fi::clipboard_folder(&text, home)))
}

// Anbindung an die Shell.

impl Shell {
    /// Aktion `NewSession` (Menü, Mac ⌘N, Windows Strg+N): öffnet den Dialog, solange er nicht schon offen ist.
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
        // `KADRELL_DEV_OVERLAY=rename`: Umbenennen der ersten Session; `confirm1`: Rückfrage des Presenters.
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() == Ok("rename") {
            let key = self.groups.first().and_then(|g| g.sessions.first()).map(|s| s.key.clone());
            if let Some(key) = key {
                self.open_rename(super::rename::Target::Session(key), window, cx);
            }
        }
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() == Ok("new") {
            let query = std::env::var("KADRELL_DEV_QUERY").ok();
            self.open_new_session(query.as_deref(), window, cx);
        }
    }

    /// Für Screenshots: `KADRELL_DEV_FOCUS=<n>` springt nach dem Öffnen eines Dialogs `n` Tab-Stopps weiter (Index 1 ist das
    /// zweite Element), ohne Tastendruck; der Ring steht dann trotzdem (`dialog_keys::shows_ring`).
    pub fn dev_focus_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(steps) = std::env::var("KADRELL_DEV_FOCUS").ok().and_then(|v| v.parse::<usize>().ok()) else { return };
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(2500)).await;
            let _ = this.update_in(cx, |_, window, cx| {
                for _ in 0..steps {
                    window.focus_next(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Öffnet den Dialog; bei offenem Panel erst nach dessen Schließen (`SheetPresenter.present`).
    pub fn open_new_session(&mut self, query: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.map(str::to_string);
        self.present_overlay(Box::new(move |shell, window, cx| shell.show_new_session(query.as_deref(), window, cx)), window, cx);
    }

    fn show_new_session(&mut self, query: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let setup = self.new_session_setup(query);
        let dialog = cx.new(|cx| NewSession::new(setup, window, cx));
        cx.subscribe_in(&dialog, window, |this, _, ev: &NewSessionEvent, window, cx| match ev {
            NewSessionEvent::Close => this.close_overlay(window, cx),
            NewSessionEvent::Start { group, path } => {
                this.close_overlay(window, cx);
                this.start_new_session(group.clone(), path, None, window, cx);
            }
            NewSessionEvent::Extension { name, id } => {
                this.close_overlay(window, cx);
                this.ext.palette_select(name, id);
            }
            NewSessionEvent::OpenSettings => {
                this.close_overlay(window, cx);
                this.open_settings_action(window, cx);
            }
        })
        .detach();
        self.overlay = Some(dialog);
        cx.notify();
    }

    /// Eingaben des Dialogs aus dem Stand der App: Gruppen mit lebenden Sessions, Index ohne verschwundene Ordner, Wurzeln
    /// des Scans (Startordner plus die Ordner über allen bekannten Projekten) und die Einträge der Extensions.
    fn new_session_setup(&self, query: Option<&str>) -> Setup {
        let state = &self.state;
        let home = state.home();
        let mut index = FolderIndex::load(&state.profile.dir.to_string_lossy().replace('\\', "/"), &home);
        index.drop_missing(now_secs());
        let groups: Vec<GroupInfo> = state.store.groups.groups.iter().map(|g| GroupInfo { id: g.id.clone(), name: g.name.clone(), cwd: g.cwd.replace('\\', "/"), host: g.host.clone() }).collect();
        let counts = state.store.groups.groups.iter().map(|g| (g.id.clone(), g.session_ids.iter().filter(|k| state.session(k).is_some()).count())).collect();
        let start_folder = state.settings.start_folder(&state.paths.home).replace('\\', "/");
        let roots = scan_roots_for(&home, &start_folder, &groups, &index);
        let inputs = Inputs { groups, counts, clones: self.extension_clones(), start_folder, home, now: now_secs() };
        Setup { inputs, index, roots, summary: crate::models::agent_pick::claude_summary(&state.settings), query: query.map(String::from) }
    }

    /// Von Extensions beigesteuerte Einträge (Repos zum Klonen), Pfad `ext:<name>:<id>`.
    fn extension_clones(&self) -> Vec<Candidate> {
        self.ext
            .palette_entries()
            .into_iter()
            .map(|(name, item)| Candidate {
                path: format!("ext:{name}:{}", item.id),
                group: None,
                tag: item.group.clone().or_else(|| item.detail.clone()).unwrap_or_default(),
                title: Some(item.title),
                detail: item.detail,
                select: Some(Select::Extension { name, id: item.id }),
            })
            .collect()
    }

    /// Klang „Pop“ einer neuen Session (Swift: `Feedback.play(.open)`), wenn die Einstellung Klänge erlaubt.
    pub(crate) fn play_open_sound(&self) {
        let level = crate::tray_wire::rules::sound_level(self.state.settings.sounds());
        kadrell_tray::play(level, kadrell_tray::model::Sound::Open);
    }

    /// Schließt den Dialog; der Fokus geht zurück an das Terminal der gewählten Session.
    fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        self.sync(window, cx);
    }

    /// Ordner der fokussierten Session (Strg+Enter, Strg+T): die Gruppe, sonst der Startordner der Einstellungen.
    fn focused_folder(&self) -> String {
        let focused = self.state.cur_focus().as_deref().and_then(|k| self.state.session(k));
        match focused {
            Some(s) => s.cwd.clone(),
            None => self.state.settings.start_folder(&self.state.paths.home).trim_end_matches('/').to_string(),
        }
    }

    /// Strg+Enter / ⌘↩: neue Claude-Session im Ordner der fokussierten, ohne Dialog.
    pub fn new_session_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cwd = self.focused_folder();
        let group = self.state.cur_focus().as_deref().and_then(|k| self.state.store.groups.group_for_session(k)).map(|g| g.id.clone());
        self.start_new_session(group, &cwd, None, window, cx);
    }

    /// Strg+T / ⌘T: Terminal ohne Claude im Ordner der fokussierten Session.
    pub fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cwd = self.focused_folder();
        self.state.new_shell_session(&cwd, true);
        self.play_open_sound();
        self.sync(window, cx);
    }
}

/// Wurzeln des Repo-Scans: der Startordner, die Wurzeln von Strg+N (unter Windows liegen Projekte oft außerhalb von `~`)
/// und die Ordner über allen Gruppen und benutzten Ordnern (Swift: `openNewSession`).
fn scan_roots_for(home: &str, start_folder: &str, groups: &[GroupInfo], index: &FolderIndex) -> Vec<String> {
    let mut roots = scan_roots(home);
    roots.push(start_folder.trim_end_matches('/').to_string());
    let known = groups.iter().map(|g| g.cwd.clone()).chain(index.uses.keys().cloned());
    roots.extend(known.filter_map(|p| p.trim_end_matches('/').rsplit_once('/').map(|(parent, _)| parent.to_string())).filter(|p| !p.is_empty()));
    roots.sort();
    roots.dedup();
    roots
}

/// Wurzeln des Repo-Scans: `~/development`, unter Windows zusätzlich `C:/development`, falls es dort liegt.
pub(crate) fn scan_roots(home: &str) -> Vec<String> {
    let mut roots = vec![format!("{home}/development")];
    if cfg!(windows) && !home.to_lowercase().starts_with("c:/development") {
        roots.push("C:/development".to_string());
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_shows_eight_rows_at_any_ui_size() {
        assert_eq!(list_height(1.), 368.);
        assert_eq!(list_height(1.5), 552.);
        assert_eq!(list_height(1.5) / (super::super::folder_row::ROW_HEIGHT * 1.5), VISIBLE_ROWS);
    }

    #[test]
    fn scan_roots_cover_start_folder_and_parents_of_known_projects() {
        let groups = vec![GroupInfo { id: "a".into(), name: "x".into(), cwd: "/p/work/alpha".into(), host: None }];
        let index = FolderIndex::load("/gibt/es/nicht", "/h");
        let roots = scan_roots_for("/h", "/h/src/", &groups, &index);
        assert!(roots.contains(&"/h/src".to_string()));
        assert!(roots.contains(&"/p/work".to_string()));
        assert!(roots.contains(&"/h/development".to_string()));
    }
}
