//! In welchem Fenster steht ein Dialog? Wie `SheetPresenter.present` und `ChildPanel.follow` hängt jedes Overlay an dem
//! Fenster, das den Befehl gegeben hat. Die Dialoge selbst gehören der `Shell` (ein Satz Zustand), gezeichnet werden sie
//! in genau einem Fenster, dem Wirt (`WinState::host`): `None` ist das Hauptfenster. Ein weiteres Fenster, das einen
//! Dialog öffnet, wird Wirt, solange irgendein Overlay offen ist. Die Fläche der Maße (`overlay::area`) setzt jedes Fenster
//! in seinem eigenen Zeichnen, die Overlays folgen so der Größe ihres Fensters.

use gpui::{AnyElement, Context, FocusHandle, IntoElement, Window};

use crate::Shell;

/// Der Wirt nach einem Befehl aus Fenster `from`: ein weiteres Fenster, wenn dabei ein Overlay aufging, das vorher nicht
/// offen war; sonst bleibt es, wie es war. So überstimmt ein Befehl aus dem Hauptfenster einen alten Wirt nicht von allein,
/// ein offener Dialog bleibt in seinem Fenster.
pub fn host_after_command(current: Option<usize>, from: usize, was_open: bool, is_open: bool) -> Option<usize> {
    match (is_open, was_open) {
        (false, _) => None,
        (true, true) => current,
        (true, false) => (from != 0).then_some(from),
    }
}

/// Zeichnet Fenster `index` die Overlays? Das Hauptfenster, solange kein weiteres Fenster Wirt ist.
pub fn shows_overlays(host: Option<usize>, index: usize) -> bool {
    host.unwrap_or(0) == index
}

fn push<E: IntoElement>(layers: &mut Vec<AnyElement>, view: Option<E>) {
    layers.extend(view.map(IntoElement::into_any_element));
}

impl Shell {
    /// Nummer des Fensters, in dem die Overlays stehen (0 = Hauptfenster).
    pub(crate) fn overlay_host(&self) -> usize {
        self.win.host.unwrap_or(0)
    }

    /// Alle Dialoge und Panels, in der Reihenfolge ihrer Lage über dem Fenster; leer, wenn `index` nicht der Wirt ist.
    pub(crate) fn overlay_layers(&self, index: usize) -> Vec<AnyElement> {
        let mut layers = Vec::new();
        if !shows_overlays(self.win.host, index) {
            return layers;
        }
        push(&mut layers, self.overlay.clone());
        push(&mut layers, self.palette.clone());
        push(&mut layers, self.quick_start.clone());
        push(&mut layers, self.agent_pick.clone());
        push(&mut layers, self.remote.clone());
        push(&mut layers, self.settings.clone());
        push(&mut layers, self.ext_ui.dialog.clone());
        push(&mut layers, self.rename.clone());
        push(&mut layers, self.misc.help.clone());
        push(&mut layers, self.misc.confirm.clone());
        push(&mut layers, self.groups_ui.edit.clone());
        push(&mut layers, self.groups_ui.confirm.clone());
        push(&mut layers, self.usage.stats.clone());
        push(&mut layers, self.closing_layer());
        layers
    }

    /// Ist kein Overlay mehr offen (und keines vorgemerkt), endet die Rolle des Wirts: das Hauptfenster ist es wieder.
    pub(crate) fn settle_host(&mut self) {
        if self.win.host.is_some() && !self.any_overlay_open() && !self.overlays.has_pending() {
            self.win.host = None;
        }
    }

    /// Nach einem Befehl aus Fenster `from`: ging ein Overlay auf, steht es in diesem Fenster.
    pub(crate) fn note_host_after(&mut self, from: usize, was_open: bool) {
        self.win.host = host_after_command(self.win.host, from, was_open, self.any_overlay_open());
    }

    /// Ist `window` ein weiteres Fenster, hat der Befehl dort die Terminals nicht zu halten: das Hauptfenster gleicht ab. Hier
    /// bleibt, was dem Fenster gehört: vorgemerkte Dialoge zeigen, den Wirt freigeben, die Tastatur zurückgeben.
    pub(crate) fn sync_foreign(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_in_main(cx);
        self.run_pending_overlay(window, cx);
        let was_host = self.win.host;
        self.settle_host();
        if was_host.is_some() && self.win.host.is_none() {
            self.refocus_extra(window, cx);
        }
        cx.notify();
    }

    /// Dialog zu: die Tastatur geht an die Fokus-Kachel dieses weiteren Fensters zurück (`ExtraWindow::focus_pane`).
    fn refocus_extra(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.extras.iter().find(|(_, h)| *h == window.window_handle()).map(|(i, _)| *i) else { return };
        let pane = self.state.wins.get(index).and_then(|v| v.focus.as_ref()).and_then(|k| self.panes.get(k)).cloned();
        if let Some(pane) = pane {
            pane.read(cx).focus.clone().focus(window, cx);
        }
    }

    /// Ist `window` das Hauptfenster? Vor dem Start des Fensters (kein Handle) gilt jedes Fenster als Hauptfenster.
    pub(crate) fn is_main_window(&self, window: &Window) -> bool {
        self.main_window.is_none_or(|main| main == window.window_handle())
    }

    /// Hat das Fenster mit dieser Tastatur-Ruhelage den Fokus nicht in einem Dialog? Für Esc ohne Fokus im Dialog.
    pub(crate) fn window_has_focus(&self, window: &Window, idle: &FocusHandle, cx: &gpui::App) -> bool {
        let Some(focused) = window.focused(cx) else { return true };
        focused == *idle || self.groups.iter().flat_map(|g| g.sessions.iter()).filter_map(|s| s.pane.as_ref()).any(|p| p.read(cx).focus == focused)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_from_a_second_window_makes_it_the_host() {
        assert_eq!(host_after_command(None, 2, false, true), Some(2));
    }

    #[test]
    fn a_command_from_the_main_window_keeps_the_main_window() {
        assert_eq!(host_after_command(None, 0, false, true), None);
    }

    #[test]
    fn an_open_dialog_stays_in_its_window() {
        assert_eq!(host_after_command(Some(1), 0, true, true), Some(1));
        assert_eq!(host_after_command(Some(1), 2, true, true), Some(1));
        assert_eq!(host_after_command(None, 2, true, true), None, "ein Dialog im Hauptfenster bleibt dort");
    }

    #[test]
    fn a_command_without_a_dialog_changes_nothing() {
        assert_eq!(host_after_command(None, 1, false, false), None);
        assert_eq!(host_after_command(Some(1), 1, true, false), None, "alles zu: kein Wirt mehr");
    }

    #[test]
    fn only_the_host_draws_the_overlays() {
        assert!(shows_overlays(None, 0));
        assert!(!shows_overlays(None, 1));
        assert!(shows_overlays(Some(2), 2));
        assert!(!shows_overlays(Some(2), 0), "das Hauptfenster zeichnet sie nicht mit");
        assert!(!shows_overlays(Some(2), 1));
    }
}
