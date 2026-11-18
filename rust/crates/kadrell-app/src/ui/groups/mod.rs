//! Gruppen, Menüs, Ziehen und Ablegen im Baum (Paket „groups“): Dialoge „Neue Gruppe“ und „Gruppe bearbeiten“, die
//! Rückfrage beim Schließen, Mittelklick, Kontextmenüs, Ordner aus dem Explorer, Ziehen von Zeilen und Kacheln.
//! `GroupsUi` ist der einzige Zustand, den die `Shell` dafür hält; alles andere liegt in den Untermodulen.

pub mod confirm;
pub mod drag;
pub mod edit;
pub mod files;
pub mod menu;
pub mod model;
pub mod rows;
pub mod tool_icons;

use gpui::{App, AppContext, Context, Entity, ScrollHandle, Window};

use kadrell_core::SidebarSort;
use kadrell_core::group::Group;

use self::confirm::{Confirm, ConfirmAction, ConfirmEvent, ConfirmSpec};
use self::drag::{DragRow, DragTile, Over, Plan, TreeView};
use self::edit::{EditEvent, EditGroup};
use self::model::EditGroupModel;
use crate::Shell;
use crate::state::Run;
use crate::ui::context_menu::Cmd;
use crate::ui::sidebar::SidebarCtx;

/// Taste für ⇧F10 und alle Tasten der Dialoge dieses Pakets.
pub fn bind_keys(cx: &mut App) {
    edit::bind_keys(cx);
    confirm::bind_keys(cx);
    crate::ui::context_menu::bind_keys(cx);
}

/// Zustand des Pakets in der `Shell`.
pub struct GroupsUi {
    pub edit: Option<Entity<EditGroup>>,
    pub confirm: Option<Entity<Confirm>>,
    /// Zeile, auf der die mittlere Taste gedrückt wurde (Schlüssel der Session oder Id der Gruppe): geschlossen wird
    /// erst beim Loslassen über derselben Zeile, damit Verrutschen nichts trifft.
    pub middle: Option<rows::RowKey>,
    /// Scrollstand des Baums (für ⇧F10: wo steht die Zeile?).
    pub scroll: ScrollHandle,
}

impl GroupsUi {
    pub fn new() -> Self {
        Self { edit: None, confirm: None, middle: None, scroll: ScrollHandle::new() }
    }

    /// Ein Dialog dieses Pakets hat die Tastatur.
    pub fn open(&self) -> bool {
        self.edit.is_some() || self.confirm.is_some()
    }
}

impl Default for GroupsUi {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {
    // MARK: Neue Gruppe, Gruppe bearbeiten

    /// ⌘⌥N: leere Gruppe mit Name, Ordner und Farbe, Vorschlag ist der Projektordner. Sie bleibt ohne Session stehen
    /// (Herz); Sessions kommen über „Neue Session in dieser Gruppe“, ⌘N oder Verschieben hinein.
    pub fn open_new_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.groups_ui.open() || self.blocking_dialog() || self.context.is_some() {
            return;
        }
        let folder = self.state.settings.start_folder(&self.state.paths.home);
        let folder = folder.trim_end_matches(['/', '\\']).replace('\\', "/");
        let draft = self.state.store.groups.make_group(&folder, None);
        let others = self.state.store.groups.groups.clone();
        let model = EditGroupModel::new(draft, others, true, &self.state.home());
        self.present_overlay(Box::new(move |shell, window, cx| shell.show_group_dialog(model, window, cx)), window, cx);
    }

    pub fn open_edit_group(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.groups_ui.open() || self.blocking_dialog() {
            return;
        }
        let Some(group) = self.state.store.groups.group(id).cloned() else { return };
        let others = self.state.store.groups.groups.iter().filter(|g| g.id != id).cloned().collect();
        let model = EditGroupModel::new(group, others, false, &self.state.home());
        self.present_overlay(Box::new(move |shell, window, cx| shell.show_group_dialog(model, window, cx)), window, cx);
    }

    /// Ein anderer Dialog ist schon offen.
    fn blocking_dialog(&self) -> bool {
        self.overlay.is_some() || self.palette.is_some() || self.settings.is_some() || self.quick_start.is_some() || self.rename.is_some()
    }

    fn show_group_dialog(&mut self, model: EditGroupModel, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| EditGroup::new(model, window, cx));
        cx.subscribe_in(&dialog, window, |this, _, ev: &EditEvent, window, cx| {
            this.groups_ui.edit = None;
            if let EditEvent::Save(group, is_new) = ev {
                this.apply_group_edit(group.clone(), *is_new);
            }
            this.sync(window, cx);
        })
        .detach();
        self.groups_ui.edit = Some(dialog);
        cx.notify();
    }

    fn apply_group_edit(&mut self, group: Group, is_new: bool) {
        if is_new {
            self.state.add_group(group);
        } else {
            self.state.store.groups.update(group);
        }
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=newgroup|editgroup|confirm`, mit `KADRELL_DEV_SEED=groups`
    /// legt ein Wegwerfprofil vorher drei Gruppen mit Terminals an.
    pub fn open_groups_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match std::env::var("KADRELL_DEV_OVERLAY").as_deref() {
            Ok("newgroup") => self.open_new_group(window, cx),
            Ok("editgroup") => {
                let first = self.state.store.groups.groups.first().map(|g| g.id.clone());
                if let Some(id) = first {
                    self.open_edit_group(&id, window, cx);
                }
            }
            Ok("confirm") => {
                let key = self.groups.first().and_then(|g| g.sessions.first()).map(|s| s.key.clone());
                if let Some(key) = key {
                    self.request_close_session(&key, false, window, cx);
                }
            }
            Ok("menu-session") => self.dev_menu(false, cx),
            Ok("menu-group") => self.dev_menu(true, cx),
            _ => {}
        }
    }

    /// Kontextmenü am Baum öffnen (Screenshots): `group` zeigt das der ersten Gruppe, sonst das ihrer ersten Session.
    fn dev_menu(&mut self, group: bool, cx: &mut Context<Self>) {
        let Some(g) = self.groups.first() else { return };
        let items = if group {
            self.group_menu_items(&g.id.clone())
        } else {
            let Some(key) = g.sessions.first().map(|s| s.key.clone()) else { return };
            self.session_menu_items(&key)
        };
        let at = gpui::point(gpui::px(40.), gpui::px(70.));
        self.open_context(at, items, cx);
        // Das Untermenü offen zeigen: Cursor auf den ersten Eintrag mit Untermenü.
        if let Some(menu) = self.context.as_mut()
            && let Some(i) = menu.items.iter().position(|i| matches!(i, crate::ui::context_menu::Item::Sub(..)))
            && !group
        {
            menu.hover(0, i);
        }
    }

    // MARK: Rückfragen

    /// Führt die Aktion sofort aus oder fragt vorher (`Ask` abgeschaltet oder `skip`: sofort).
    fn confirm_or_run(&mut self, spec: ConfirmSpec, skip: bool, window: &mut Window, cx: &mut Context<Self>) {
        let asked = spec.ask.is_none_or(|a| self.state.settings.ask_enabled(a));
        if skip || !asked {
            return self.run_confirmed(&spec.action, window, cx);
        }
        let action = spec.action.clone();
        let dialog = cx.new(|cx| Confirm::new(spec, window, cx));
        cx.subscribe_in(&dialog, window, move |this, _, ev: &ConfirmEvent, window, cx| match ev {
            ConfirmEvent::DontAsk(ask, on) => this.set_ask(*ask, !*on),
            ConfirmEvent::Accept(_) => {
                this.groups_ui.confirm = None;
                this.run_confirmed(&action, window, cx);
            }
            ConfirmEvent::Cancel(_) => {
                this.groups_ui.confirm = None;
                this.sync(window, cx);
            }
        })
        .detach();
        self.groups_ui.confirm = Some(dialog);
        cx.notify();
    }

    /// „Nicht mehr fragen“ gilt sofort, Abbrechen stellt zurück (siehe `Confirm::cancel`).
    fn set_ask(&mut self, ask: kadrell_core::settings::Ask, enabled: bool) {
        let mut settings = self.state.settings.clone();
        settings.set_ask_enabled(ask, enabled);
        self.state.apply_settings(settings);
    }

    fn run_confirmed(&mut self, action: &ConfirmAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            ConfirmAction::CloseSession(key) => self.state.remove_sessions(std::slice::from_ref(key)),
            ConfirmAction::CloseGroup(id) => self.state.close_group(id),
            ConfirmAction::StopSession(key) => {
                self.state.stop_tile(key);
            }
        }
        self.sync(window, cx);
    }

    /// Session beenden und entfernen (`closeSession`): mit Rückfrage, `force` (⌥) und Remote-Sessions ohne.
    pub fn request_close_session(&mut self, key: &str, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.state.session(key) else { return };
        let spec = confirm::close_session_spec(session);
        let skip = force || session.is_remote();
        self.confirm_or_run(spec, skip, window, cx);
    }

    /// Gruppe schließen (`closeGroup`): eine leere geht ohne Rückfrage weg.
    pub fn request_close_group(&mut self, id: &str, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.state.store.groups.group(id) else { return };
        let members = group.session_ids.iter().filter(|k| self.state.session(k).is_some()).count();
        let spec = confirm::close_group_spec(id, &group.name, members);
        self.confirm_or_run(spec, force || members == 0, window, cx);
    }

    /// Session stoppen (`stopSession`): nur mit laufendem Prozess, mit Rückfrage.
    pub fn request_stop(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.run(key) != Run::Running {
            return self.sync(window, cx);
        }
        let Some(session) = self.state.session(key) else { return };
        let spec = confirm::stop_session_spec(session);
        self.confirm_or_run(spec, false, window, cx);
    }

    // MARK: Aktionen der Menüs

    /// Neue Claude-Session im Ordner der Gruppe, darin.
    pub fn new_session_in_group(&mut self, id: &str) {
        let Some(cwd) = self.state.store.groups.group(id).map(|g| g.cwd.clone()) else { return };
        self.state.new_claude_session(&cwd, Some(id), true);
    }

    /// Neues Terminal ohne Claude im Ordner der Gruppe, darin (auch wenn zwei Gruppen denselben Ordner haben).
    pub fn new_terminal_in_group(&mut self, id: &str) {
        let Some(cwd) = self.state.store.groups.group(id).map(|g| g.cwd.clone()) else { return };
        self.state.new_shell_session_in(&cwd, Some(id), true);
    }

    /// Neue Session im Ordner dieser Session, in ihrer Gruppe.
    pub fn new_session_beside(&mut self, key: &str) {
        let Some(session) = self.state.session(key) else { return };
        let cwd = session.cwd.clone();
        let group = self.state.store.groups.group_for_session(key).map(|g| g.id.clone());
        if !cwd.is_empty() {
            self.state.new_claude_session(&cwd, group.as_deref(), true);
        }
    }

    pub fn show_group_sessions(&mut self, id: &str) {
        let keys = self.state.store.groups.group(id).map(|g| g.session_ids.clone()).unwrap_or_default();
        let keys: Vec<String> = keys.into_iter().filter(|k| self.state.session(k).is_some()).collect();
        if !keys.is_empty() {
            self.state.select_keys(&keys, false);
        }
    }

    /// Ordner im eingestellten Editor öffnen (Menü, ⌥E).
    pub fn open_editor_in(&mut self, cwd: &str) {
        let command = self.state.settings.editor_command();
        if let Err(e) = rows::spawn_editor(&command, cwd) {
            eprintln!("kadrell: Editor nicht gestartet: {e}");
        }
    }

    /// ⌥E: Ordner der Fokus-Session im Editor.
    pub fn open_editor_focused(&mut self) {
        let cwd = self.state.focused.as_deref().and_then(|k| self.state.session(k)).map(|s| s.cwd.clone());
        if let Some(cwd) = cwd {
            self.open_editor_in(&cwd);
        }
    }

    // MARK: Ordner aus dem Explorer

    /// Ordner auf den Baum gezogen: neue Session dort, in der Gruppe mit diesem Ordner oder einer neuen.
    pub fn drop_folder(&mut self, dir: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.state.new_claude_session(dir, None, true);
        self.sync(window, cx);
    }

    // MARK: Ziehen

    /// Was gezogen werden darf, hängt von Gruppierung und Sortierung des Baums ab.
    pub fn tree_view(&self) -> TreeView {
        TreeView { grouped: self.state.grouped(), sort_off: self.state.sort() == SidebarSort::Off }
    }

    /// Das, was der Baum von der Shell braucht (er wird gezeichnet, während die Shell gesperrt ist).
    pub fn sidebar_ctx(&self) -> SidebarCtx {
        SidebarCtx { view: self.tree_view(), scroll: self.groups_ui.scroll.clone(), keys: self.ws.tree_focus }
    }

    /// Zeile im Baum losgelassen: Session sortieren oder verschieben, Gruppe verschieben.
    pub fn drop_on_row(&mut self, drag: &DragRow, key: Option<&str>, group: &str, window: &mut Window, cx: &mut Context<Self>) {
        let over = match key {
            Some(key) => Over::Session { key, group },
            None => Over::Head { group },
        };
        if let Some(plan) = drag::plan(drag, over, self.tree_view()) {
            self.apply_plan(&plan);
            self.sync(window, cx);
        }
    }

    /// Kachel an der Titelzeile auf eine andere Kachel gezogen (`moveSession` der Mac-App).
    pub fn drop_tile(&mut self, drag: &DragTile, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(plan) = drag::plan_tile(&self.state.store.groups, &drag.key, target) {
            self.apply_plan(&plan);
            self.sync(window, cx);
        }
    }

    /// Führt einen Plan aus: gruppiert über den Kern (`groups.json`), flach über die Reihenfolge der Einstellungen.
    fn apply_plan(&mut self, plan: &Plan) {
        if let Plan::ReorderFlat { id, target } = plan {
            let shown: Vec<String> = self.groups.iter().flat_map(|g| g.sessions.iter().map(|s| s.key.clone())).collect();
            if let Some(order) = drag::flat_reordered(&shown, id, target) {
                self.state.set_flat_order(&order);
            }
            return;
        }
        drag::apply(&mut self.state.store.groups, plan);
    }

    // MARK: Tastatur

    /// Befehle des Baums (Session- und Gruppenmenü): führt aus und gleicht ab; was einen Dialog öffnet, gleicht selbst ab.
    pub fn run_tree_cmd(&mut self, cmd: Cmd, window: &mut Window, cx: &mut Context<Self>) {
        let Some(cmd) = self.run_group_cmd(cmd, window, cx) else { return };
        if let Some(cmd) = self.run_session_cmd(cmd, window, cx) {
            debug_assert!(false, "Befehl ohne Zuständigen: {cmd:?}");
        }
    }

    /// Befehle der Gruppe; `Some` gibt zurück, was nicht dazugehört.
    fn run_group_cmd(&mut self, cmd: Cmd, window: &mut Window, cx: &mut Context<Self>) -> Option<Cmd> {
        match cmd {
            Cmd::ToggleFavorite(id) => self.state.store.groups.toggle_favorite(&id),
            Cmd::NewSessionIn(id) => self.new_session_in_group(&id),
            Cmd::NewTerminalIn(id) => self.new_terminal_in_group(&id),
            Cmd::ShowGroup(id) => self.show_group_sessions(&id),
            Cmd::OpenEditor(path) => self.open_editor_in(&path),
            Cmd::CloseGroup(id) => {
                self.request_close_group(&id, false, window, cx);
                return None;
            }
            Cmd::EditGroup(id) => {
                self.open_edit_group(&id, window, cx);
                return None;
            }
            other => return Some(other),
        }
        self.sync(window, cx);
        None
    }

    /// Befehle der Session; `Some` gibt zurück, was nicht dazugehört.
    fn run_session_cmd(&mut self, cmd: Cmd, window: &mut Window, cx: &mut Context<Self>) -> Option<Cmd> {
        match cmd {
            Cmd::Resume(key) => {
                let _ = self.state.request_start(&key);
            }
            Cmd::Stop(key) => {
                self.request_stop(&key, window, cx);
                return None;
            }
            Cmd::Remove(key) => {
                self.request_close_session(&key, false, window, cx);
                return None;
            }
            Cmd::ShowOnly(key) => self.state.show_only(&key),
            Cmd::HideFromView(key) => self.state.select_keys(&[key], true),
            Cmd::MoveTo { key, group } => {
                self.state.move_sessions(&[key], &group);
            }
            Cmd::MoveMany { keys, group } => {
                self.state.move_sessions(&keys, &group);
            }
            Cmd::CopyPath(path) => cx.write_to_clipboard(gpui::ClipboardItem::new_string(path)),
            Cmd::Reveal(path) => cx.open_with_system(std::path::Path::new(&path)),
            Cmd::NewSessionHere(key) => self.new_session_beside(&key),
            other => return Some(other),
        }
        self.sync(window, cx);
        None
    }

    /// Palette `> Gruppe bearbeiten` und Menü: Gruppe der fokussierten Session.
    pub fn edit_focused_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let group = self.state.focused.as_deref().and_then(|k| self.state.store.groups.group_for_session(k)).map(|g| g.id.clone());
        if let Some(id) = group {
            self.open_edit_group(&id, window, cx);
        }
    }

    /// ⇧F10: Kontextmenü der Fokus-Session an ihrer Zeile im Baum (bei zugeklappter Gruppe das der Gruppe).
    pub fn show_context_menu_for_focus(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.groups_ui.open() || self.blocking_dialog() || self.context.is_some() {
            return;
        }
        let Some(key) = self.state.focused.clone() else { return };
        let a = crate::appearance::current(cx);
        let m = a.rows();
        let measures = rows::Rows { top_pad: a.s(m.top_inset), group_row: a.s(m.group_row), session_row: a.s(m.session_row), group_gap: a.s(m.group_gap) };
        let Some((top, group_row)) = rows::focus_row_top(&self.groups, &key, measures) else { return };
        let bounds = self.groups_ui.scroll.bounds();
        let offset = self.groups_ui.scroll.offset();
        let height = if group_row { measures.group_row } else { measures.session_row };
        let at = gpui::point(bounds.origin.x + gpui::px(48.), bounds.origin.y + offset.y + gpui::px(top + height));
        let items = if group_row {
            let Some(id) = self.state.store.groups.group_for_session(&key).map(|g| g.id.clone()) else { return };
            self.group_menu_items(&id)
        } else {
            self.session_menu_items(&key)
        };
        self.open_context(at, items, cx);
    }
}

/// Nur für Screenshots: `KADRELL_DEV_SEED=groups` legt in einem Wegwerfprofil drei Gruppen mit je zwei Terminals an
/// (im Ordner des Programms und darunter), damit Dialoge und Menüs etwas zeigen.
pub fn dev_seed(state: &mut crate::state::AppState) {
    if std::env::var("KADRELL_DEV_SEED").as_deref() != Ok("groups") || !state.profile.temporary {
        return;
    }
    let base = std::env::current_dir().map(|p| p.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")).unwrap_or_else(|_| state.home());
    for (name, sub) in [("kadrell", ""), ("rust", "/rust"), ("app", "/app")] {
        let group = state.store.groups.make_group(&format!("{base}{sub}"), Some(name));
        let (id, cwd) = (group.id.clone(), group.cwd.clone());
        state.add_group(group);
        for _ in 0..2 {
            state.new_shell_session_in(&cwd, Some(&id), false);
        }
    }
}
