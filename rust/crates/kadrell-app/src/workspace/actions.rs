//! Die Kürzel der Arbeitsfläche als Tabelle Aktion zu Handler (wie `hotkeyActions` in `WorkspaceView+Hotkeys.swift`)
//! und die Befehle dahinter: Fokus bewegen, Kacheln tauschen, Trennlinien schieben, Frei teilen, Layout wechseln,
//! Zoom schließen. Vorschau, Sync, Auto und Baumtasten stehen in `modes`.

use gpui::{Context, KeyDownEvent, Window};
use kadrell_core::hotkeys::HotkeyAction as A;
use serde_json::json;

use crate::Shell;
use crate::ui::layout::{
    Direction, GRID_COLUMNS_KEY, Id, LayoutMode, RATIO_PREFIX, SCROLL_COLUMNS_KEY, SPLITS_KEY, neighbor, neighbor_index, next_split, set_split,
    split_at, step_cols, step_scroll_cols,
};
use crate::ui::model::position;
use crate::ui::settings::shortcut::KeyInput;

/// Schlüssel für den Auto-Modus in den Einstellungen.
pub const AUTO_KEY: &str = "workspace.auto";

pub type Handler = fn(&mut Shell, &mut Window, &mut Context<Shell>);

/// Alle Aktionen, die die Arbeitsfläche selbst ausführt. Die anderen Kürzel (Dialoge, Rechte Sidebar, Palette,
/// Neustart) gehören anderen Stellen und bleiben hier ohne Eintrag. `focus1` bis `focus9` laufen über `tile_index`.
static HANDLERS: &[(A, Handler)] = &[
    (A::FocusLeft, |s, w, c| s.move_focus(Direction::Left, w, c)),
    (A::FocusRight, |s, w, c| s.move_focus(Direction::Right, w, c)),
    (A::FocusUp, |s, w, c| s.move_focus(Direction::Up, w, c)),
    (A::FocusDown, |s, w, c| s.move_focus(Direction::Down, w, c)),
    (A::SwapLeft, |s, w, c| s.swap_focused(Direction::Left, w, c)),
    (A::SwapRight, |s, w, c| s.swap_focused(Direction::Right, w, c)),
    (A::SwapUp, |s, w, c| s.swap_focused(Direction::Up, w, c)),
    (A::SwapDown, |s, w, c| s.swap_focused(Direction::Down, w, c)),
    (A::ResizeLeft, |s, _, c| s.resize_focused(Direction::Left, c)),
    (A::ResizeRight, |s, _, c| s.resize_focused(Direction::Right, c)),
    (A::ResizeUp, |s, _, c| s.resize_focused(Direction::Up, c)),
    (A::ResizeDown, |s, _, c| s.resize_focused(Direction::Down, c)),
    (A::SplitRight, |s, _, c| s.set_split_here('r', c)),
    (A::SplitDown, |s, _, c| s.set_split_here('d', c)),
    (A::NextSession, |s, w, c| s.cycle_focus(1, w, c)),
    (A::PrevSession, |s, w, c| s.cycle_focus(-1, w, c)),
    (A::LastSession, |s, w, c| s.focus_last(w, c)),
    (A::PreviewNext, |s, w, c| s.preview_step(1, w, c)),
    (A::PreviewPrev, |s, w, c| s.preview_step(-1, w, c)),
    (A::NextWaiting, |s, w, c| s.focus_next_waiting(w, c)),
    (A::Zoom, |s, w, c| s.zoom_focused(w, c)),
    (A::NextLayout, |s, _, c| s.cycle_layout(c)),
    (A::CloseFocused, |s, w, c| s.close_focused_tile(w, c)),
    (A::SyncInput, |s, w, c| s.toggle_sync(w, c)),
    (A::FocusSidebar, |s, w, c| s.focus_tree(w, c)),
    (A::FocusWorkspace, |s, w, c| s.focus_workspace(w, c)),
    (A::RenameSession, |s, w, c| s.rename_focused(w, c)),
    (A::CycleSort, |s, _, c| s.cycle_sort(c)),
    (A::ToggleGrouping, |s, _, c| s.toggle_grouped(c)),
    (A::OpenEditor, |s, w, c| s.open_editor_focused(w, c)),
];

/// Der Handler einer Aktion der Arbeitsfläche, `None` für alle anderen.
pub fn handler_for(action: A) -> Option<Handler> {
    HANDLERS.iter().find(|(a, _)| *a == action).map(|(_, h)| *h)
}

/// Gehört die Aktion der Arbeitsfläche?
pub fn is_workspace_action(action: A) -> bool {
    action.tile_index().is_some() || handler_for(action).is_some()
}

impl Shell {
    /// Steht ein Dialog, die Palette oder eine Eingabe offen? Dann gehören alle Tasten ihr.
    pub(crate) fn modal_open(&self) -> bool {
        self.overlay.is_some()
            || self.palette.is_some()
            || self.settings.is_some()
            || self.quick_start.is_some()
            || self.rename.is_some()
            || self.ext_ui.dialog.is_some()
            || self.usage.overlay_open()
            || self.misc.overlay_open()
            || self.groups_ui.open()
            || self.agent_pick.is_some()
            || self.remote.is_some()
            || self.context.is_some()
    }

    /// Ein Tastendruck, bevor das Terminal ihn sieht: Vorschau beenden, Kürzel aus den Einstellungen, Baumtasten, Sync.
    /// `true`: verbraucht.
    pub(crate) fn workspace_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.modal_open() {
            return false;
        }
        let ks = &ev.keystroke;
        let m = ks.modifiers;
        let input = KeyInput { control: m.control, alt: m.alt, shift: m.shift, platform: m.platform, key: ks.key.clone() };
        if self.preview_key(&input, window, cx) {
            return true;
        }
        self.ws.keys.refresh(&self.state.settings);
        if let Some(action) = self.ws.keys.lookup(&input).filter(|a| is_workspace_action(*a)) {
            self.perform(action, window, cx);
            return true;
        }
        if self.ws.tree_focus && self.tree_key(&input, window, cx) {
            return true;
        }
        self.forward_sync(ks, window, cx);
        false
    }

    /// Führt eine Aktion der Arbeitsfläche aus. Jede außer Vorschau blättern beendet eine laufende Vorschau.
    pub(crate) fn perform(&mut self, action: A, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(action, A::PreviewNext | A::PreviewPrev) {
            self.end_preview(false, window, cx);
        }
        if let Some(i) = action.tile_index() {
            self.focus_tile_at(i, window, cx);
        } else if let Some(run) = handler_for(action) {
            run(self, window, cx);
        }
    }

    // --- Kacheln ---

    /// Alle Kacheln der Arbeitsfläche in Reihenfolge: im Auto-Modus der gefilterte Pool, sonst die Auswahl. Ein Zoom
    /// verdeckt davon nur Teile, er ändert diese Liste nicht.
    pub(crate) fn tiles(&self) -> Vec<Id> {
        if self.ws.auto.on {
            return self.ws.auto.shown.iter().filter_map(|k| position(&self.groups, k)).collect();
        }
        self.selection.items.clone()
    }

    pub(crate) fn tile_keys(&self) -> Vec<String> {
        self.tiles().iter().map(|&(g, s)| self.groups[g].sessions[s].key.clone()).collect()
    }

    /// Was gerade zu sehen ist: die Vorschau allein, sonst bei Zoom die Fokus-Kachel allein, sonst alle Kacheln.
    pub(crate) fn visible(&self) -> Vec<Id> {
        if let Some(id) = self.preview_id() {
            return vec![id];
        }
        let tiles = self.tiles();
        match self.selected {
            Some(focus) if self.state.zoom && tiles.contains(&focus) => vec![focus],
            _ => tiles,
        }
    }

    /// Fokus auf eine Kachel: ist sie ausgewählt, wandert der Fokus in der Auswahl; im Auto-Modus kann sie auch nur
    /// angezeigt sein. Die Tastatur geht an sie.
    pub(crate) fn set_focus(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tiles().contains(&id) {
            return;
        }
        if self.selection.contains(id) {
            self.selection.focus_on(id);
            self.apply_selection(window, cx);
        } else {
            let key = self.groups[id.0].sessions[id.1].key.clone();
            self.ws.tree_focus = false;
            self.state.focus_any(&key);
            self.sync(window, cx);
        }
    }

    fn focus_tile_at(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(&id) = self.tiles().get(i) {
            self.set_focus(id, window, cx);
        }
    }

    fn focus_index(&self) -> Option<usize> {
        let focus = self.selected?;
        self.tiles().iter().position(|id| *id == focus)
    }

    /// Nachbar der Fokus-Kachel: im Stack und Zoom nach Reihenfolge, sonst nach Lage der Felder.
    fn neighbor_tile(&self, d: Direction) -> Option<Id> {
        let tiles = self.tiles();
        let i = self.focus_index()?;
        let j = if self.layout.mode == LayoutMode::Stack || self.state.zoom {
            neighbor_index(i, tiles.len(), LayoutMode::Stack, d)
        } else {
            neighbor(i, &self.layout.frames, d)
        };
        j.and_then(|j| tiles.get(j).copied())
    }

    /// ⌥-Pfeil: Fokus zur Nachbarkachel in dieser Richtung.
    pub(crate) fn move_focus(&mut self, d: Direction, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.neighbor_tile(d) {
            self.set_focus(id, window, cx);
        }
    }

    /// Nächste (+1) oder vorige (-1) Kachel, am Ende wieder vorn.
    pub(crate) fn cycle_focus(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let tiles = self.tiles();
        let Some(i) = self.focus_index() else { return };
        let n = tiles.len() as isize;
        self.set_focus(tiles[(i as isize + step).rem_euclid(n) as usize], window, cx);
    }

    /// ⌥⇥: zurück zur zuletzt fokussierten Kachel.
    pub(crate) fn focus_last(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.state.last_focused.as_deref().and_then(|k| position(&self.groups, k)) else { return };
        self.set_focus(id, window, cx);
    }

    /// ⌥⇧-Pfeil: die Fokus-Kachel tauscht mit ihrem Nachbarn. Die Reihenfolge gehört dem Baum: in derselben Gruppe
    /// tauschen die Sessions den Platz, über Gruppengrenzen rückt die ganze Gruppe an die Stelle der Zielgruppe.
    pub(crate) fn swap_focused(&mut self, d: Direction, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(from), Some((g, s))) = (self.selected, self.neighbor_tile(d)) else { return };
        let (id, target) = (self.groups[from.0].sessions[from.1].key.clone(), self.groups[g].sessions[s].key.clone());
        self.move_session_to(&id, &target);
        self.sync(window, cx);
    }

    /// Baum und Arbeitsfläche teilen eine Reihenfolge (`moveSession` der Mac-App).
    pub(crate) fn move_session_to(&mut self, id: &str, target: &str) {
        let groups = &mut self.state.store.groups;
        let of = |key: &str, groups: &kadrell_core::group::GroupStore| groups.group_for_session(key).map(|g| g.id.clone());
        let (Some(from), Some(to)) = (of(id, groups), of(target, groups)) else { return };
        if from == to {
            groups.move_session(id, target);
        } else {
            groups.move_group(&from, &to);
        }
    }

    /// ⌥Esc: die Fokus-Kachel aus der Auswahl nehmen, im Zoom erst zurück ins Layout. Die Session läuft weiter.
    pub(crate) fn close_focused_tile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.zoom {
            self.state.zoom = false;
            cx.notify();
            return;
        }
        let Some(key) = self.state.focused.clone() else { return };
        self.state.select_keys(&[key], true);
        self.sync(window, cx);
    }

    /// ⌥Z: die Fokus-Kachel allein, die Auswahl bleibt. Noch einmal zeigt wieder alle.
    pub(crate) fn zoom_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_some() {
            self.toggle_zoom(cx);
            self.sync(window, cx);
        }
    }

    // --- Layout ---

    pub(crate) fn set_mode(&mut self, mode: LayoutMode, cx: &mut Context<Self>) {
        self.layout.mode = mode;
        self.layout.reveal = true;
        self.state.layout = crate::control_layout(mode);
        self.state.save_layout();
        cx.notify();
    }

    /// ⌘L: reihum durch alle Layouts.
    pub(crate) fn cycle_layout(&mut self, cx: &mut Context<Self>) {
        self.set_mode(self.layout.mode.next(), cx);
    }

    /// ‹ ›: Spalten im Grid (0 = automatisch) bzw. sichtbare Spalten beim Scrollen.
    pub(crate) fn step_cols(&mut self, delta: isize, cx: &mut Context<Self>) {
        match self.layout.mode {
            LayoutMode::Grid => {
                self.layout.cols = step_cols(self.layout.cols, self.tiles().len(), delta);
                self.state.save_workspace_setting(GRID_COLUMNS_KEY, Some(json!(self.layout.cols)));
            }
            LayoutMode::Scroll => {
                self.layout.scroll_cols = step_scroll_cols(self.layout.scroll_cols, delta);
                self.state.save_workspace_setting(SCROLL_COLUMNS_KEY, Some(json!(self.layout.scroll_cols)));
            }
            _ => return,
        }
        cx.notify();
    }

    /// Verhältnisse der Vorlage `key` in die Einstellungen, ohne Eintrag wieder gleich verteilt.
    pub(crate) fn persist_ratio(&mut self, key: &str) {
        let value = self.layout.ratios.get(key).map(|v| json!(v));
        self.state.save_workspace_setting(&format!("{RATIO_PREFIX}{key}"), value);
    }

    /// Loslassen an einer Trennlinie: das Ergebnis des Ziehens bleibt.
    pub(crate) fn end_drag(&mut self) {
        if let Some(line) = self.layout.dragging.take() {
            self.persist_ratio(&line.key);
        }
    }

    /// ⌃⌥-Pfeile wie tmux `resize-pane`: die Trennlinie an der Fokus-Kachel wandert 5 % in Pfeilrichtung, beim
    /// Scrollen schaltet ← / → die Spaltenbreite eine Stufe (⅓ ½ ⅔).
    pub(crate) fn resize_focused(&mut self, d: Direction, cx: &mut Context<Self>) {
        if self.state.zoom || self.ws.preview.is_active() {
            return;
        }
        let Some(i) = self.focus_index() else { return };
        let changed = if self.layout.mode == LayoutMode::Scroll {
            self.layout.step_scroll_width(i, d).then(|| crate::ui::layout::SCROLL_KEY.to_string())
        } else {
            self.layout.frames.get(i).copied().and_then(|frame| self.layout.nudge(frame, d))
        };
        if let Some(key) = changed {
            self.persist_ratio(&key);
            cx.notify();
        }
    }

    /// Frei (i3 `split h/v`): wo die Kachel nach der fokussierten entsteht, `r` rechts, `d` unten, `a` längere Seite.
    /// Schaltet auf Frei um.
    pub(crate) fn set_split_here(&mut self, dir: char, cx: &mut Context<Self>) {
        if self.state.zoom || self.ws.preview.is_active() {
            return;
        }
        let Some(i) = self.focus_index() else { return };
        self.layout.splits = set_split(&self.layout.splits, i, dir);
        self.state.save_workspace_setting(SPLITS_KEY, Some(json!(self.layout.splits)));
        self.set_mode(LayoutMode::Custom, cx);
    }

    /// Teilrichtung an der Fokus-Kachel für die Leiste (`a` ohne Vorgabe).
    pub(crate) fn focused_split(&self) -> char {
        self.focus_index().map_or('a', |i| split_at(&self.layout.splits, i))
    }

    /// Klick auf TEILT: auto, rechts, unten, auto.
    pub(crate) fn cycle_split(&mut self, cx: &mut Context<Self>) {
        self.set_split_here(next_split(self.focused_split()), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_one_owner() {
        let mut seen = Vec::new();
        for (action, _) in HANDLERS {
            assert!(!seen.contains(action), "{action:?} doppelt in der Tabelle");
            assert!(A::ALL.contains(action), "{action:?} ist keine bekannte Aktion");
            assert!(action.tile_index().is_none(), "focusN laufen über tile_index");
            seen.push(*action);
        }
        let tiles = A::ALL.iter().filter(|a| a.tile_index().is_some()).count();
        assert_eq!(tiles, 9);
        assert!(A::ALL.iter().filter(|a| a.tile_index().is_some()).all(|a| is_workspace_action(*a)));
    }

    #[test]
    fn the_table_covers_the_workspace_actions_of_the_mac_app() {
        // `hotkeyActions` in WorkspaceView+Hotkeys.swift plus Vorschau, nächste wartende, Baum und Umbenennen.
        for action in [
            A::FocusLeft, A::FocusRight, A::FocusUp, A::FocusDown, A::SwapLeft, A::SwapRight, A::SwapUp, A::SwapDown,
            A::ResizeLeft, A::ResizeRight, A::ResizeUp, A::ResizeDown, A::SplitRight, A::SplitDown, A::NextSession,
            A::PrevSession, A::LastSession, A::Zoom, A::NextLayout, A::SyncInput, A::CloseFocused, A::PreviewNext,
            A::PreviewPrev, A::NextWaiting, A::FocusSidebar, A::FocusWorkspace, A::OpenEditor,
        ] {
            assert!(handler_for(action).is_some(), "{action:?} fehlt");
        }
    }

    #[test]
    fn other_places_keep_their_actions() {
        for action in [
            A::ShowHelp, A::ShowStats, A::ShowExtensions, A::FocusRightSidebar, A::ToggleRightSidebar, A::QuickStart,
            A::SearchHistory, A::Restart,
        ] {
            assert!(!is_workspace_action(action), "{action:?} gehört nicht der Arbeitsfläche");
        }
    }
}
