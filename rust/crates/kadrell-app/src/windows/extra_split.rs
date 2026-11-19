//! TEILT in der Leiste eines weiteren Fensters (Layout Frei): die Teilrichtung an der Fokus-Kachel dieses Fensters, wie
//! im Hauptfenster (`Shell::cycle_split`). Der Klick schaltet auf Frei um und speichert die Teilungen.

use gpui::Context;
use serde_json::json;

use super::extra::ExtraWindow;
use crate::ui::layout::{LayoutMode, SPLITS_KEY};
use crate::ui::tiling::{next_split, set_split, split_at};

/// Teilrichtung an der Fokus-Kachel: `a` (auto) ohne Fokus oder ohne Vorgabe.
pub fn split_for(tiles: &[String], focus: Option<&str>, splits: &str) -> char {
    let at = focus.and_then(|f| tiles.iter().position(|k| k == f));
    at.map_or('a', |i| split_at(splits, i))
}

impl ExtraWindow {
    /// Teilrichtung an der Fokus-Kachel dieses Fensters für die Leiste.
    pub(super) fn focused_split(&self, cx: &gpui::App) -> char {
        let view = self.view(cx);
        split_for(&self.tile_keys(&view, cx), view.focus.as_deref(), &self.layout.splits)
    }

    /// Klick auf TEILT: auto, rechts, unten, auto. Im Zoom gilt nichts (die Kachel steht allein da).
    pub(super) fn cycle_split(&mut self, cx: &mut Context<Self>) {
        let view = self.view(cx);
        let tiles = self.tile_keys(&view, cx);
        let Some(i) = view.focus.as_deref().and_then(|f| tiles.iter().position(|k| k == f)).filter(|_| !view.zoom) else { return };
        let next = next_split(split_at(&self.layout.splits, i));
        self.layout.splits = set_split(&self.layout.splits, i, next);
        let splits = json!(self.layout.splits);
        self.shell.update(cx, |sh, _| sh.state.save_workspace_setting(SPLITS_KEY, Some(splits)));
        self.set_mode(LayoutMode::Custom, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_split_belongs_to_the_focus_tile_of_the_window() {
        let tiles = keys(&["a", "b", "c"]);
        assert_eq!(split_for(&tiles, Some("b"), "rdr"), 'd');
        assert_eq!(split_for(&tiles, Some("c"), "rd"), 'a', "ohne Eintrag: auto");
        assert_eq!(split_for(&tiles, None, "rdr"), 'a');
        assert_eq!(split_for(&tiles, Some("gone"), "rdr"), 'a');
    }

    #[test]
    fn the_click_walks_auto_right_down() {
        let mut splits = String::new();
        let mut seen = Vec::new();
        for _ in 0..4 {
            let next = next_split(split_at(&splits, 1));
            splits = set_split(&splits, 1, next);
            seen.push(split_at(&splits, 1));
        }
        assert_eq!(seen, ['r', 'd', 'a', 'r']);
    }
}
