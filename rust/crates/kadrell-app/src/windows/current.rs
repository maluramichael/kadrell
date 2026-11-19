//! Das aktuelle Fenster (`AppDelegate.current`): Menü, Kürzel, Palette, Dialoge und Fernsteuerung wirken in dem Fenster,
//! das zuletzt Fokus hatte. Die Auswahl der weiteren Fenster steht in `views`; hier geht es um das Umschalten und um
//! Befehle, die ein weiteres Fenster an das Hauptfenster gibt. Dialoge (Neue Session, Palette, Einstellungen) liegen als
//! Overlay im Hauptfenster: es kommt dafür nach vorn, `current` bleibt aber das weitere Fenster, bis der Dialog zu ist
//! (der „Pin“), damit die neue Session dort landet, wo der Befehl gegeben wurde.

use std::time::{Duration, Instant};

use gpui::{Action, AppContext as _, Context, Window};

use crate::Shell;

/// So lange darf ein Pin warten, bis sein Dialog erscheint; ohne Dialog (Befehl tat nichts) fällt er weg.
const PIN_PATIENCE: Duration = Duration::from_secs(2);

/// Ein weiteres Fenster hat einen Dialog im Hauptfenster angefordert.
#[derive(Debug, Clone, Copy)]
pub struct Pin {
    pub index: usize,
    /// Der Dialog war schon einmal offen: sobald er zu ist, endet der Pin.
    seen: bool,
    since: Instant,
}

impl Pin {
    pub fn new(index: usize) -> Pin {
        Pin { index, seen: false, since: Instant::now() }
    }

    /// Nächster Stand des Pins bei dieser Lage der Overlays; `None`, wenn er endet.
    pub fn step(mut self, overlay_open: bool, now: Instant) -> Option<Pin> {
        if overlay_open {
            self.seen = true;
            return Some(self);
        }
        let expired = !self.seen && now.duration_since(self.since) > PIN_PATIENCE;
        (!self.seen && !expired).then_some(self)
    }
}

impl Shell {
    /// Das Fenster `index` ist jetzt vorn: Menü, Kürzel und Leiste richten sich nach ihm. Solange ein Dialog für ein
    /// weiteres Fenster offen ist, ändert das Hauptfenster (das dafür nach vorn kam) nichts.
    pub(crate) fn note_current(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == 0 && self.win.pin.is_some() {
            return;
        }
        self.win.pin = None;
        if self.state.wins.current != index {
            self.state.set_current_window(index);
            cx.notify();
        }
    }

    /// Nummer des Fensters, auf das Befehle wirken (ein geschlossenes zählt nicht).
    pub(crate) fn current_window(&self) -> usize {
        self.state.wins.target()
    }

    pub(crate) fn any_overlay_open(&self) -> bool {
        self.plain_overlay_open() || self.panel_open() || self.usage.overlay_open() || self.misc.overlay_open()
    }

    /// Nach jedem Abgleich: ist der Dialog für ein weiteres Fenster zu, gilt wieder das Hauptfenster, wenn es vorn ist.
    pub(crate) fn settle_pin(&mut self, window: &Window) {
        let Some(pin) = self.win.pin else { return };
        self.win.pin = pin.step(self.any_overlay_open(), Instant::now());
        if self.win.pin.is_none() && window.is_window_active() {
            self.state.set_current_window(0);
        }
    }

}

/// Ein weiteres Fenster gibt einen Befehl an das Hauptfenster. Mit `dialog` kommt das Hauptfenster nach vorn und
/// `current` bleibt das weitere Fenster, bis der Dialog zu ist. Nur der Typ der Aktion wird weitergegeben, nie die Taste.
pub fn forward(shell: &gpui::Entity<Shell>, from: usize, action: Box<dyn Action>, dialog: bool, cx: &mut gpui::App) {
    let main = shell.update(cx, |sh, _| {
        sh.state.set_current_window(from);
        if dialog {
            sh.win.pin = Some(Pin::new(from));
        }
        sh.main_window
    });
    let Some(main) = main else { return };
    let _ = cx.update_window(main, |_, window, cx| {
        if dialog {
            window.activate_window();
        }
        window.dispatch_action(action, cx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_lives_until_its_dialog_has_come_and_gone() {
        let t0 = Instant::now();
        let pin = Pin::new(1);
        let pin = pin.step(false, t0).expect("der Dialog kommt noch");
        let pin = pin.step(true, t0).expect("Dialog offen");
        assert!(pin.step(true, t0).is_some());
        assert!(pin.step(false, t0).is_none(), "Dialog zu: der Pin endet");
    }

    #[test]
    fn a_pin_without_a_dialog_expires() {
        let pin = Pin::new(2);
        assert!(pin.step(false, pin.since + PIN_PATIENCE + Duration::from_millis(1)).is_none());
        assert!(pin.step(false, pin.since + Duration::from_millis(10)).is_some());
    }
}
