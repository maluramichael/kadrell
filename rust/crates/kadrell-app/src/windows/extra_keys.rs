//! Kürzel und Menüpunkte im weiteren Fenster: sie wirken auf das Fenster mit Fokus (`AppDelegate.current`), wie in der
//! Mac-App, wo jedes Fenster seine eigene Arbeitsfläche hat. Dazu gehören nächste und vorige Kachel, Fokus in eine
//! Richtung, Kachel 1 bis 9, Zoom, Spaltenzahl, Kachel schließen und die Kürzel aus den Einstellungen. Was Dialoge öffnet
//! (Schnellstart, Verlaufssuche, Statistik, Umbenennen), geht an das Hauptfenster (`current::forward_hotkey`).
//! Sync gilt je Fenster (`extra_modes`). Vorschau, Teilen und Tauschen kennt nur das Hauptfenster.

use gpui::{Context, KeyDownEvent, Window};
use kadrell_core::hotkeys::HotkeyAction as A;
use serde_json::json;

use super::extra::ExtraWindow;
use crate::ui::layout::{
    Direction, GRID_COLUMNS_KEY, LayoutMode, Rect, SCROLL_COLUMNS_KEY, neighbor, neighbor_index, step_cols, step_scroll_cols,
};
use crate::ui::settings::shortcut::KeyInput;

/// Welche Kachel bekommt den Fokus? `tiles` sind die Schlüssel der Auswahl in Anzeigereihenfolge, `frames` ihre Rahmen
/// beim letzten Zeichnen. Im Stack und im Zoom zählt die Reihenfolge, sonst die Lage der Felder (`neighborTile`).
pub fn target_tile(action: A, tiles: &[String], focus: Option<&str>, frames: &[Rect], by_order: bool) -> Option<String> {
    if let Some(i) = action.tile_index() {
        return tiles.get(i).cloned();
    }
    let at = focus.and_then(|f| tiles.iter().position(|k| k == f));
    match action {
        A::NextSession => cycle(tiles, at, 1),
        A::PrevSession => cycle(tiles, at, -1),
        A::FocusLeft => step_dir(tiles, at, frames, by_order, Direction::Left),
        A::FocusRight => step_dir(tiles, at, frames, by_order, Direction::Right),
        A::FocusUp => step_dir(tiles, at, frames, by_order, Direction::Up),
        A::FocusDown => step_dir(tiles, at, frames, by_order, Direction::Down),
        _ => None,
    }
}

/// Nächste (+1) oder vorige (-1) Kachel, am Ende wieder vorn.
fn cycle(tiles: &[String], at: Option<usize>, step: isize) -> Option<String> {
    let n = tiles.len() as isize;
    let i = at? as isize;
    tiles.get((i + step).rem_euclid(n) as usize).cloned()
}

fn step_dir(tiles: &[String], at: Option<usize>, frames: &[Rect], by_order: bool, d: Direction) -> Option<String> {
    let i = at?;
    let j = if by_order { neighbor_index(i, tiles.len(), LayoutMode::Stack, d) } else { neighbor(i, frames, d) };
    j.and_then(|j| tiles.get(j).cloned())
}

/// Im Zoom zeigt das Fenster nur die Fokus-Kachel, solange es mehr als eine gibt.
pub fn zoomed_keys(tiles: &[String], focus: Option<&str>, zoom: bool) -> Vec<String> {
    match focus {
        Some(f) if zoom && tiles.len() > 1 && tiles.iter().any(|k| k == f) => vec![f.to_string()],
        _ => tiles.to_vec(),
    }
}

impl ExtraWindow {
    /// Ein Tastendruck, bevor das Terminal ihn sieht: Kürzel aus den Einstellungen (`workspace_key` des Hauptfensters).
    pub(super) fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.context.is_some() || self.dialog_here(ev, window, cx) {
            return;
        }
        let m = ev.keystroke.modifiers;
        let input = KeyInput { control: m.control, alt: m.alt, shift: m.shift, platform: m.platform, key: ev.keystroke.key.clone() };
        self.keys.refresh(&self.shell.read(cx).state.settings);
        let Some(action) = self.keys.lookup(&input) else { return self.forward_sync(&ev.keystroke, window, cx) };
        if self.run_hotkey(action, window, cx) {
            cx.stop_propagation();
        } else {
            self.forward_sync(&ev.keystroke, window, cx);
        }
    }

    /// Steht hier ein Dialog, gehören ihm die Tasten (die Kürzel des Fensters ruhen); Esc schließt ihn auch ohne Fokus
    /// (`Shell::cancel_visible`). `true`: die Taste ist vergeben.
    fn dialog_here(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (shell, index, idle) = (self.shell.clone(), self.index, self.idle_focus.clone());
        if !crate::ui::overlay_host::shows_overlays(shell.read(cx).win.host, index) || !shell.read(cx).modal_open() {
            return false;
        }
        if ev.keystroke.key == "escape" && shell.update(cx, |sh, scx| sh.cancel_visible_with(&idle, window, scx)) {
            cx.stop_propagation();
        }
        true
    }

    /// Führt die Aktion in diesem Fenster aus; `false`, wenn dieses Fenster sie nicht kennt.
    fn run_hotkey(&mut self, action: A, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match action {
            A::Zoom => self.toggle_zoom(cx),
            A::NextLayout => self.cycle_layout(cx),
            A::CloseFocused => self.close_focused(cx),
            A::SyncInput => self.toggle_sync(cx),
            A::ShowStats => self.run_here(true, cx, |sh, w, c| sh.toggle_stats(w, c)),
            A::QuickStart | A::SearchHistory | A::Restart => {
                let dialog = action != A::Restart;
                super::current::forward_hotkey(&self.shell, self.index, action.raw(), dialog, cx);
            }
            A::RenameSession => self.run_here(true, cx, |sh, w, c| sh.rename_focused(w, c)),
            other => return self.focus_by_action(other, window, cx),
        }
        true
    }

    /// Fokus-Aktionen: nächste und vorige Kachel, Richtung, Kachel 1 bis 9.
    fn focus_by_action(&mut self, action: A, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let known = action.tile_index().is_some()
            || matches!(action, A::NextSession | A::PrevSession | A::FocusLeft | A::FocusRight | A::FocusUp | A::FocusDown);
        if !known {
            return false;
        }
        let view = self.view(cx);
        let tiles = self.tile_keys(&view, cx);
        let by_order = view.zoom || self.layout.mode == LayoutMode::Stack;
        if let Some(key) = target_tile(action, &tiles, view.focus.as_deref(), &self.layout.frames, by_order) {
            self.focus_tile(&key, window, cx);
        }
        true
    }

    /// Schlüssel aller Kacheln der Auswahl in Anzeigereihenfolge (auch die der Zoom verdeckt).
    pub(super) fn tile_keys(&self, view: &super::views::View, cx: &gpui::App) -> Vec<String> {
        Self::visible_keys(view, &self.shell.read(cx).groups)
    }

    /// ⌥Z, Menü „Kachel zoomen“: die Fokus-Kachel allein, die Auswahl bleibt. Noch einmal zeigt wieder alle.
    pub(super) fn toggle_zoom(&mut self, cx: &mut Context<Self>) {
        let index = self.index;
        self.shell.update(cx, |sh, _| sh.state.view_toggle_zoom(index));
        cx.notify();
    }

    /// ⌥Esc: im Zoom zurück ins Layout, sonst die Fokus-Kachel aus der Auswahl dieses Fensters nehmen. Die Session läuft
    /// weiter.
    fn close_focused(&mut self, cx: &mut Context<Self>) {
        let (index, view) = (self.index, self.view(cx));
        if view.zoom {
            self.shell.update(cx, |sh, _| sh.state.view_clear_zoom(index));
            cx.notify();
            return;
        }
        if let Some(key) = view.focus {
            self.shell.update(cx, |sh, _| sh.state.view_select(index, &[key], true));
            self.changed(cx);
        }
    }

    /// ‹ ›: Spalten im Grid (0 = automatisch) bzw. sichtbare Spalten beim Scrollen. Die Einstellung gilt für alle Fenster.
    pub(super) fn step_columns(&mut self, delta: isize, cx: &mut Context<Self>) {
        let (key, value) = match self.layout.mode {
            LayoutMode::Grid => {
                let n = self.tile_keys(&self.view(cx), cx).len();
                self.layout.cols = step_cols(self.layout.cols, n, delta);
                (GRID_COLUMNS_KEY, self.layout.cols)
            }
            LayoutMode::Scroll => {
                self.layout.scroll_cols = step_scroll_cols(self.layout.scroll_cols, delta);
                (SCROLL_COLUMNS_KEY, self.layout.scroll_cols)
            }
            _ => return,
        };
        self.shell.update(cx, |sh, _| sh.state.save_workspace_setting(key, Some(json!(value))));
        cx.notify();
    }

    /// Menü „Nächste/Vorherige Session“.
    pub(super) fn step_focus(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let action = if step > 0 { A::NextSession } else { A::PrevSession };
        self.focus_by_action(action, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn grid() -> Vec<Rect> {
        vec![Rect::new(0., 0., 100., 100.), Rect::new(100., 0., 100., 100.), Rect::new(0., 100., 100., 100.), Rect::new(100., 100., 100., 100.)]
    }

    #[test]
    fn next_and_previous_wrap_around() {
        let t = keys(&["a", "b", "c"]);
        assert_eq!(target_tile(A::NextSession, &t, Some("c"), &[], false).as_deref(), Some("a"));
        assert_eq!(target_tile(A::PrevSession, &t, Some("a"), &[], false).as_deref(), Some("c"));
        assert_eq!(target_tile(A::NextSession, &t, Some("a"), &[], false).as_deref(), Some("b"));
    }

    #[test]
    fn without_focus_nothing_moves() {
        let t = keys(&["a", "b"]);
        assert_eq!(target_tile(A::NextSession, &t, None, &[], false), None);
        assert_eq!(target_tile(A::NextSession, &t, Some("zzz"), &[], false), None);
        assert_eq!(target_tile(A::NextSession, &[], None, &[], false), None);
    }

    #[test]
    fn direction_follows_the_frames_in_a_grid() {
        let t = keys(&["a", "b", "c", "d"]);
        let f = grid();
        assert_eq!(target_tile(A::FocusRight, &t, Some("a"), &f, false).as_deref(), Some("b"));
        assert_eq!(target_tile(A::FocusDown, &t, Some("a"), &f, false).as_deref(), Some("c"));
        assert_eq!(target_tile(A::FocusLeft, &t, Some("d"), &f, false).as_deref(), Some("c"));
        assert_eq!(target_tile(A::FocusUp, &t, Some("d"), &f, false).as_deref(), Some("b"));
        assert_eq!(target_tile(A::FocusLeft, &t, Some("a"), &f, false), None, "am Rand bleibt der Fokus");
    }

    #[test]
    fn direction_follows_the_order_in_stack_and_zoom() {
        let t = keys(&["a", "b", "c"]);
        assert_eq!(target_tile(A::FocusDown, &t, Some("a"), &[], true).as_deref(), Some("b"));
        assert_eq!(target_tile(A::FocusUp, &t, Some("c"), &[], true).as_deref(), Some("b"));
    }

    #[test]
    fn numbered_tiles_pick_by_position() {
        let t = keys(&["a", "b"]);
        assert_eq!(target_tile(A::Focus1, &t, None, &[], false).as_deref(), Some("a"));
        assert_eq!(target_tile(A::Focus2, &t, Some("a"), &[], false).as_deref(), Some("b"));
        assert_eq!(target_tile(A::Focus3, &t, Some("a"), &[], false), None, "Kachel 3 gibt es nicht");
    }

    #[test]
    fn other_actions_are_not_tile_moves() {
        let t = keys(&["a", "b"]);
        assert_eq!(target_tile(A::Zoom, &t, Some("a"), &[], false), None);
    }

    #[test]
    fn zoom_shows_only_the_focus_tile_when_there_are_several() {
        let t = keys(&["a", "b", "c"]);
        assert_eq!(zoomed_keys(&t, Some("b"), true), keys(&["b"]));
        assert_eq!(zoomed_keys(&t, Some("b"), false), t);
        assert_eq!(zoomed_keys(&keys(&["a"]), Some("a"), true), keys(&["a"]));
        assert_eq!(zoomed_keys(&t, Some("x"), true), t, "ein Fokus ohne Kachel zoomt nichts");
        assert_eq!(zoomed_keys(&t, None, true), t);
    }
}
