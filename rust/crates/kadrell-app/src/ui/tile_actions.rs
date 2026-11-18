//! Was die Symbole in Kachelkopf und Stack-Zeile tun (`WorkspaceView.mouseDown`): schließen (⌥ ohne Rückfrage),
//! umbenennen, aus der Ansicht nehmen (die Session läuft weiter) und zoomen.

use gpui::{Context, Window};

use super::rename::Target;
use super::tile_look::Icon;
use crate::Shell;

impl Shell {
    /// Klick auf ein Symbol der Kachel `key`. `force`: ⌥ gedrückt, Schließen ohne Rückfrage.
    pub(crate) fn tile_icon(&mut self, key: &str, icon: Icon, force: bool, window: &mut Window, cx: &mut Context<Self>) {
        match icon {
            Icon::Close => self.request_close_session(key, force, window, cx),
            Icon::Rename => self.open_rename(Target::Session(key.to_string()), window, cx),
            Icon::Deselect => self.deselect_tile(key, window, cx),
            Icon::Zoom => self.zoom_cell(key, window, cx),
        }
    }

    /// Kreis-Symbol: die Kachel aus der Ansicht nehmen, die Session bleibt und läuft weiter (`deselect`). Die letzte
    /// Kachel bleibt, wie beim Umschalten im Baum.
    pub(crate) fn deselect_tile(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.selected.iter().any(|k| k == key) {
            self.state.select_keys(&[key.to_string()], true);
            self.sync(window, cx);
        }
    }

    /// Quadrat-Symbol: diese Kachel zoomen (wie ⌥Z), im Zoom verkleinert das Symbol wieder. Vor dem Zoomen bekommt die
    /// Kachel den Fokus.
    pub(crate) fn zoom_cell(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.state.zoom {
            self.state.focus_any(key);
        }
        self.toggle_zoom(cx);
        self.sync(window, cx);
    }
}
