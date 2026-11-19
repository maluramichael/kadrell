//! Was die Menüs zur Laufzeit wissen müssen (Swift: `validateMenuItem`, `menuNeedsUpdate`, `buildMenu`): ob die
//! fokussierte Session läuft, gezeigt wird und einen Ordner hat, die Ziele von „Verschieben nach“, die aktuelle
//! Belegung der Kürzel. `Shell::sync_menus` baut daraus bei jeder Änderung einen Schnappschuss und gibt ihn an die
//! gezeichnete Menüleiste (Windows) bzw. das System-Menü (Mac). Reine Funktionen und Tests stehen ohne GPUI hier;
//! die Aktionen der Einträge (Datenaktionen und die Handler dazu) auch.

use std::collections::HashSet;

use gpui::{Context, KeyDownEvent, Window};
use kadrell_core::group::Group;
use kadrell_core::hotkeys::{Hotkey, HotkeyAction};
use kadrell_i18n::trd;

use crate::Shell;
use crate::lang::de;
use crate::ui::groups::menu::move_label;
use crate::ui::layout::LayoutMode;
use crate::ui::model::parse_color;
use crate::ui::settings::shortcut::KeyInput;

// MARK: Datenaktionen

/// Layout wählen (Menü „Ansicht“, ein Eintrag je Modus).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kadrell_shell, no_json)]
pub struct SelectLayout {
    pub mode: String,
}

/// Fokussierte Session in eine Gruppe verschieben (Menü „Session ▸ Verschieben nach“).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kadrell_shell, no_json)]
pub struct MoveToGroup {
    pub group: String,
}

/// Belegbare Kürzel-Aktion aus dem Menü „Ansicht“ (`raw` der `HotkeyAction`).
#[derive(Clone, PartialEq, gpui::Action)]
#[action(namespace = kadrell_shell, no_json)]
pub struct PerformHotkey {
    pub action: String,
}

// MARK: Schnappschuss

/// Zustand der fokussierten Session für die Einträge des Menüs „Session“.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionFlags {
    /// Der Prozess läuft (Fortsetzen aus, Stoppen an).
    pub attached: bool,
    /// Die Session ist gerade zu sehen (Aus Ansicht nehmen an).
    pub shown: bool,
    /// Sie hat einen Ordner (Neue Session hier, Pfad kopieren, Im Explorer zeigen).
    pub has_cwd: bool,
}

/// Eine Gruppe als Ziel von „Verschieben nach“.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveTarget {
    pub id: String,
    pub label: String,
    pub color: u32,
    /// Alle Ziel-Sessions stehen schon hier: abgehakt und aus.
    pub here: bool,
}

/// Schnappschuss für `menus::menus_for`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuContext {
    /// Das echte Menü (mit Zustand, Ebenen und allen Kürzel-Einträgen). Ungebaut (`false`) liefert `menus()` die flache
    /// Fassung, aus der die Palette ihre Kommandos holt.
    pub live: bool,
    /// `None`: keine Session fokussiert, alle Einträge der Session sind aus.
    pub session: Option<SessionFlags>,
    pub moves: Vec<MoveTarget>,
    /// Beschriftung der aktuellen Belegung je Aktion; was fehlt, hat keine Belegung.
    pub bindings: Vec<(HotkeyAction, String)>,
}

impl MenuContext {
    /// Das echte Menü ohne Zustand (Start, bevor die Shell einen Schnappschuss liefert).
    pub fn live() -> MenuContext {
        MenuContext { live: true, ..MenuContext::default() }
    }

    /// Beschriftung der Belegung von `action`, leer ohne.
    pub fn binding(&self, action: HotkeyAction) -> String {
        self.bindings.iter().find(|(a, _)| *a == action).map(|(_, l)| l.clone()).unwrap_or_default()
    }
}

/// Alle Gruppen in Baumreihenfolge als Ziele für `ids`. Gleichnamige tragen ihren Ordner bzw. `ssh <host>` dahinter.
pub fn move_targets(groups: &[Group], ids: &[String], home: &str) -> Vec<MoveTarget> {
    groups
        .iter()
        .map(|g| MoveTarget {
            id: g.id.clone(),
            label: move_label(g, groups, home),
            color: parse_color(&g.color),
            here: !ids.is_empty() && ids.iter().all(|id| g.session_ids.contains(id)),
        })
        .collect()
}

/// Titel eines Layout-Modus (Swift: `LayoutMode.title`).
pub fn layout_title(mode: LayoutMode) -> &'static str {
    match mode {
        LayoutMode::Grid => de!("Grid"),
        LayoutMode::Main => de!("Haupt + Spalte"),
        LayoutMode::Spiral => de!("Spirale"),
        LayoutMode::Custom => de!("Frei"),
        LayoutMode::Scroll => de!("Scrollen"),
        LayoutMode::Row => de!("Reihe"),
        LayoutMode::Stack => de!("Stack"),
    }
}

/// Kürzel, die über `open_hotkey_key` laufen: Dialoge öffnen, Statistik umschalten, neu starten.
pub fn is_open_action(action: HotkeyAction) -> bool {
    matches!(action, HotkeyAction::QuickStart | HotkeyAction::SearchHistory | HotkeyAction::Restart | HotkeyAction::ShowStats)
}

/// Aktionen, die schon im App-Menü stehen (Panels, Swift: `panel(_:)`): die Liste der Kürzel überspringt sie.
pub fn is_panel_action(action: HotkeyAction) -> bool {
    matches!(action, HotkeyAction::ShowHelp | HotkeyAction::ShowStats | HotkeyAction::ShowExtensions)
}

/// Beschriftung einer Kürzel-Aktion im Menü: `Kachel 3` für die Nummern, sonst der Titel der Aktion.
pub fn hotkey_title(action: HotkeyAction) -> String {
    match action.tile_index() {
        Some(i) => trd!("Kachel %lld", (i + 1) as i64),
        None => trd!(&action.title_key()),
    }
}

/// Die belegbaren Kürzel-Aktionen für das Menü „Ansicht“ in der Reihenfolge der Mac-App: ohne Panels, die Kacheln
/// gehören ins Untermenü. `bool`: gehört zum Untermenü „Kachel wählen“.
pub fn hotkey_actions() -> Vec<(HotkeyAction, bool)> {
    HotkeyAction::ALL.iter().copied().filter(|a| !is_panel_action(*a)).map(|a| (a, a.tile_index().is_some())).collect()
}

/// Belegung als Menü-Beschriftung: Mac-Zeichen oder Windows-Schreibweise, leer ohne Taste.
pub fn binding_label(key: &Hotkey) -> String {
    if key.key.is_empty() { String::new() } else { crate::ui::ext::keys::label(key, cfg!(target_os = "macos")) }
}

// MARK: Alle auswählen

/// Was ⌘A / ⇧⌘A zuletzt getan hat, damit dasselbe Kürzel es zurücknimmt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectUndo {
    pub shift: bool,
    pub before: Vec<String>,
    pub focus: Option<String>,
    pub after: HashSet<String>,
}

/// Was der nächste Druck tut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectStep {
    /// Die Auswahl davor zurück (mit ihrem Fokus).
    Restore { before: Vec<String>, focus: Option<String> },
    /// Diese Sessions auswählen.
    Select(Vec<String>),
    /// Nichts auszuwählen.
    Nothing,
}

/// Nochmal dasselbe Kürzel, solange die Auswahl unverändert ist: zurück zur Auswahl davor. Sonst auswählen, ohne
/// Ziel gibt es nichts zu tun (die Mac-App piept).
pub fn select_step(undo: Option<&SelectUndo>, shift: bool, ids: &[String], selected: &[String]) -> SelectStep {
    let now: HashSet<&String> = selected.iter().collect();
    if let Some(u) = undo.filter(|u| u.shift == shift && u.after.iter().collect::<HashSet<_>>() == now) {
        return SelectStep::Restore { before: u.before.clone(), focus: u.focus.clone() };
    }
    if ids.is_empty() { SelectStep::Nothing } else { SelectStep::Select(ids.to_vec()) }
}

/// ⌘A: alle Sessions der Gruppen, in denen schon etwas ausgewählt ist.
pub fn sessions_of_chosen_groups(groups: &[Group], selected: &[String]) -> Vec<String> {
    let chosen: HashSet<&String> = selected.iter().collect();
    groups.iter().filter(|g| g.session_ids.iter().any(|id| chosen.contains(id))).flat_map(|g| g.session_ids.iter().cloned()).collect()
}

/// ⇧⌘A: alle Sessions aller Gruppen.
pub fn sessions_of_all_groups(groups: &[Group]) -> Vec<String> {
    groups.iter().flat_map(|g| g.session_ids.iter().cloned()).collect()
}

// MARK: Shell

/// Zustand der Menüs in der Shell (ein Feld).
#[derive(Debug, Default)]
pub struct MenuState {
    /// Schnappschuss, mit dem Leiste und System-Menü zuletzt gebaut wurden.
    pub built: Option<MenuContext>,
    pub undo: Option<SelectUndo>,
}

impl Shell {
    /// Schnappschuss der Menüs aus dem Zustand der App.
    fn menu_context(&self) -> MenuContext {
        let key = self.state.cur_focus();
        let session = key.as_deref().and_then(|k| self.state.session(k)).map(|s| SessionFlags {
            attached: self.state.run(&s.id) == crate::state::Run::Running,
            shown: self.state.cur_selected().contains(&s.id),
            has_cwd: !s.cwd.is_empty(),
        });
        let ids: Vec<String> = key.into_iter().collect();
        let moves = move_targets(&self.state.store.groups.groups, &ids, &self.state.home());
        let bindings = HotkeyAction::ALL.iter().filter_map(|a| Some((*a, binding_label(self.ws.keys.key_of(*a)?)))).collect();
        MenuContext { live: true, session, moves, bindings }
    }

    /// Einmal je Bild: hat sich etwas geändert, Menüleiste (Windows) und System-Menü (Mac) neu bauen.
    pub(crate) fn sync_menus(&mut self, cx: &mut Context<Self>) {
        let context = self.menu_context();
        if self.menu.built.as_ref() == Some(&context) {
            return;
        }
        self.menubar.update(cx, |bar, _| bar.set_context(&context));
        crate::menus::set_native_menus_for(cx, &context);
        self.menu.built = Some(context);
    }

    /// Gesamte Auswahl (Schlüssel) in Baumreihenfolge.
    fn selected_keys(&self) -> Vec<String> {
        self.state.cur_selected()
    }

    /// Setzt die Auswahl der Mac-App-Art: nur Sessions, die es gibt, der Fokus bleibt in der Auswahl.
    fn select_for_menu(&mut self, keys: Vec<String>, focus: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.ws.tree_focus = false;
        self.ws.preview.clear();
        self.state.zoom = false;
        self.state.cur_set_selection(keys, focus);
        self.sync(window, cx);
    }

    /// ⌘A (Menü „Bearbeiten“) und ⇧⌘A, mit Zurücknehmen beim zweiten Druck.
    fn toggle_selection(&mut self, ids: Vec<String>, shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.selected_keys();
        match select_step(self.menu.undo.as_ref(), shift, &ids, &selected) {
            SelectStep::Restore { before, focus } => {
                self.menu.undo = None;
                self.select_for_menu(before, focus, window, cx);
            }
            SelectStep::Select(keys) => {
                let focus = self.state.cur_focus();
                self.select_for_menu(keys, None, window, cx);
                let after = self.state.cur_selected().into_iter().collect();
                self.menu.undo = Some(SelectUndo { shift, before: selected, focus, after });
            }
            // TODO: kein Piepton in der Rust-App, die Aktion bleibt wirkungslos.
            SelectStep::Nothing => {}
        }
    }

    pub(crate) fn select_groups_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = sessions_of_chosen_groups(&self.state.store.groups.groups, &self.state.cur_selected());
        self.toggle_selection(ids, false, window, cx);
    }

    pub(crate) fn select_everything_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = sessions_of_all_groups(&self.state.store.groups.groups);
        self.toggle_selection(ids, true, window, cx);
    }

    /// Menü „Ansicht“: ein Layout wählen.
    pub(crate) fn select_layout_action(&mut self, mode: &str, cx: &mut Context<Self>) {
        let Some(mode) = LayoutMode::from_name(mode) else { return };
        // Menü und Kürzel wirken im aktuellen Fenster: ein weiteres Fenster hat sein Layout im Zustand (`windows::views`).
        match self.state.wins.current_extra() {
            Some(index) => {
                self.state.view_set_mode(index, kadrell_control::LayoutMode::from_raw(mode.key()).unwrap_or_default());
                cx.notify();
            }
            None => self.set_mode(mode, cx),
        }
    }

    /// Menü „Session“: die fokussierte Session in diese Gruppe.
    pub(crate) fn move_focused_to_group(&mut self, group: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.state.cur_focus() else { return };
        self.run_cmd(crate::ui::context_menu::Cmd::MoveTo { key, group: group.to_string() }, window, cx);
    }

    /// Menü „Session“: Nur diese zeigen, Aus Ansicht nehmen, Neue Session hier, Pfad kopieren, Im Explorer zeigen.
    pub(crate) fn focused_cmd(&mut self, make: impl FnOnce(String, String) -> crate::ui::context_menu::Cmd, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.state.cur_focus() else { return };
        let cwd = self.state.session(&key).map(|s| s.cwd.clone()).unwrap_or_default();
        self.run_cmd(make(key, cwd), window, cx);
    }

    /// Kürzel aus dem Menü „Ansicht“ ausführen (Swift: `menuHotkey`, `perform`).
    pub(crate) fn perform_hotkey_action(&mut self, raw: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = HotkeyAction::from_raw(raw) else { return };
        self.end_preview_for(action, window, cx);
        match action {
            HotkeyAction::QuickStart => self.open_quick_start(window, cx),
            HotkeyAction::SearchHistory => self.open_palette("#", window, cx),
            HotkeyAction::Restart => self.restart(window, cx),
            HotkeyAction::FocusRightSidebar => window.dispatch_action(Box::new(crate::ui::ext::FocusRightSidebar), cx),
            HotkeyAction::ToggleRightSidebar => window.dispatch_action(Box::new(crate::ui::ext::ToggleRightSidebar), cx),
            other => self.perform(other, window, cx),
        }
    }


    /// Kürzel, die ein Panel oder einen Dialog öffnen oder die App neu starten (Swift: `performOpen`). Die Belegung
    /// kommt live aus den Einstellungen. Die Statistik schaltet auch bei offenem Dialog um (Panel zu, wie in der Mac-
    /// App), alles andere nur ohne offenen Dialog. `true`: verbraucht.
    pub(crate) fn open_hotkey_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let m = ev.keystroke.modifiers;
        let input = KeyInput { control: m.control, alt: m.alt, shift: m.shift, platform: m.platform, key: ev.keystroke.key.clone() };
        self.ws.keys.refresh(&self.state.settings);
        let Some(action) = self.ws.keys.lookup(&input).filter(|a| is_open_action(*a)) else { return false };
        let stats_open = self.usage.stats.is_some();
        if self.modal_open() && !(action == HotkeyAction::ShowStats && stats_open) {
            return false;
        }
        match action {
            HotkeyAction::ShowStats => self.toggle_stats(window, cx),
            other => self.perform_hotkey_action(other.raw(), window, cx),
        }
        true
    }

    /// Menü „Einsetzen“ ohne Textfeld im Fokus: der Text geht in das Terminal der Fokus-Kachel.
    pub(crate) fn paste_into_focused(&mut self, cx: &mut Context<Self>) {
        let Some(pane) = self.state.cur_focus().as_deref().and_then(|k| self.panes.get(k)).cloned() else { return };
        pane.update(cx, |pane, cx| pane.paste(cx));
    }

    /// „Kadrell ausblenden“: auf dem Mac die App, sonst das Fenster (in das Tray bzw. die Taskleiste).
    pub(crate) fn hide_app(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if cfg!(target_os = "macos") {
            cx.hide();
        } else {
            self.hide_main_window(window, cx);
        }
    }

    /// Jede Aktion außer dem Blättern beendet eine laufende Vorschau (`perform` macht das für Arbeitsflächen-Kürzel).
    fn end_preview_for(&mut self, action: HotkeyAction, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(action, HotkeyAction::PreviewNext | HotkeyAction::PreviewPrev) && !crate::workspace::actions::is_workspace_action(action) {
            self.end_preview(false, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str, name: &str, cwd: &str, sessions: &[&str]) -> Group {
        let mut g = Group::new(id, name, "#fab387", cwd, sessions);
        g.favorite = false;
        g
    }

    fn keys(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn move_targets_check_the_group_the_sessions_already_live_in() {
        let groups = vec![group("g1", "kadrell", "C:/p/kadrell", &["a"]), group("g2", "spike", "C:/p/spike", &["b"])];
        let targets = move_targets(&groups, &keys(&["a"]), "C:/Users/m");
        assert_eq!((targets[0].here, targets[1].here), (true, false));
        assert_eq!(targets[0].color, 0xfab387);
        assert!(move_targets(&groups, &[], "").iter().all(|t| !t.here), "ohne Session ist nichts abgehakt");
    }

    #[test]
    fn same_named_groups_carry_their_folder_or_ssh_host() {
        let mut remote = group("g2", "api", "", &[]);
        remote.host = Some("dev1".into());
        let groups = vec![group("g1", "api", "C:/Users/m/dev/api", &[]), remote, group("g3", "solo", "/x", &[])];
        let labels: Vec<String> = move_targets(&groups, &keys(&["a"]), "C:/Users/m").into_iter().map(|t| t.label).collect();
        assert_eq!(labels, ["api · ~/dev/api", "api · ssh dev1", "solo"]);
    }

    #[test]
    fn the_hotkey_list_skips_panels_and_sorts_tiles_into_the_submenu() {
        let list = hotkey_actions();
        assert!(list.iter().all(|(a, _)| !is_panel_action(*a)));
        assert_eq!(list.iter().filter(|(_, tile)| *tile).count(), 9);
        assert_eq!(list.len(), HotkeyAction::ALL.len() - 3);
        let last = list.iter().position(|(a, _)| *a == HotkeyAction::LastSession).unwrap();
        let first_tile = list.iter().position(|(_, tile)| *tile).unwrap();
        assert!(first_tile > last, "die Kacheln folgen hinter `lastSession`");
    }

    #[test]
    fn titles_of_hotkey_actions() {
        assert_eq!(hotkey_title(HotkeyAction::Focus3), "Kachel 3");
        assert_eq!(hotkey_title(HotkeyAction::RenameSession), "Session umbenennen");
    }

    #[test]
    fn layout_titles_follow_the_mac_app() {
        let titles: Vec<&str> = LayoutMode::ALL.iter().map(|m| layout_title(*m)).collect();
        assert_eq!(titles, ["Grid", "Haupt + Spalte", "Spirale", "Frei", "Scrollen", "Reihe", "Stack"]);
    }

    fn groups3() -> Vec<Group> {
        vec![group("g1", "a", "/a", &["a1", "a2"]), group("g2", "b", "/b", &["b1"]), group("g3", "c", "/c", &["c1"])]
    }

    #[test]
    fn select_groups_takes_whole_groups_that_already_have_a_selected_session() {
        let g = groups3();
        assert_eq!(sessions_of_chosen_groups(&g, &keys(&["a2", "c1"])), keys(&["a1", "a2", "c1"]));
        assert!(sessions_of_chosen_groups(&g, &[]).is_empty());
        assert_eq!(sessions_of_all_groups(&g), keys(&["a1", "a2", "b1", "c1"]));
    }

    #[test]
    fn the_same_shortcut_again_restores_the_previous_selection() {
        let ids = keys(&["a1", "a2", "b1", "c1"]);
        let first = select_step(None, true, &ids, &keys(&["a1"]));
        assert_eq!(first, SelectStep::Select(ids.clone()));
        let undo = SelectUndo { shift: true, before: keys(&["a1"]), focus: Some("a1".into()), after: ids.iter().cloned().collect() };
        let again = select_step(Some(&undo), true, &ids, &keys(&["c1", "a1", "b1", "a2"]));
        assert_eq!(again, SelectStep::Restore { before: keys(&["a1"]), focus: Some("a1".into()) }, "Reihenfolge ist egal");
    }

    #[test]
    fn a_changed_selection_or_the_other_shortcut_starts_over() {
        let ids = keys(&["a1", "a2"]);
        let undo = SelectUndo { shift: false, before: keys(&["a1"]), focus: None, after: ids.iter().cloned().collect() };
        assert_eq!(select_step(Some(&undo), true, &ids, &ids), SelectStep::Select(ids.clone()), "anderes Kürzel");
        assert_eq!(select_step(Some(&undo), false, &ids, &keys(&["a1"])), SelectStep::Select(ids.clone()), "Auswahl geändert");
        assert_eq!(select_step(None, false, &[], &keys(&["a1"])), SelectStep::Nothing, "nichts zu wählen");
    }

    #[test]
    fn context_defaults_are_the_flat_palette_menu() {
        assert!(!MenuContext::default().live);
        assert!(MenuContext::live().live);
        assert_eq!(MenuContext::default().binding(HotkeyAction::Zoom), "");
    }
}
