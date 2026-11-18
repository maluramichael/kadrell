//! Klänge für Öffnen, Schließen und Umschalten sowie Tipp und Ansage (Swift: `Feedback.swift`, `AttentionTracker`).
//! Der Zustand merkt die Klänge nur vor (`queue_sound`), gespielt wird in der Oberfläche (`Shell::refresh_tray`),
//! weil `kadrell_tray::play` auf Windows wirklich piept und Tests still bleiben müssen.

use kadrell_claude::attention::TipText;
use kadrell_i18n::trd;
use kadrell_tray::Sound;

use super::AppState;

impl AppState {
    /// Merkt einen Klang für die Oberfläche (neue Session, Session entfernt, AUTO/SYNC/GRP umgeschaltet).
    pub fn queue_sound(&mut self, sound: Sound) {
        self.pending_sounds.push(sound);
    }

    /// Die Klänge seit dem letzten Abruf, in der Reihenfolge, in der sie anfielen.
    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.pending_sounds)
    }

    /// Der Tipp, der in der Leiste steht (Kürzel in der Schreibweise des Systems).
    pub fn tip_text(&self) -> Option<String> {
        self.attention.tip.as_ref().map(tip_text)
    }

    /// Klick auf den Tipp: weg damit.
    pub fn dismiss_tip(&mut self) {
        self.attention.clear_tip(None);
    }
}

/// Text eines Tipps.
pub fn tip_text(tip: &TipText) -> String {
    match tip {
        TipText::WaitingMeansYellow => crate::misc::os_text(&trd!("Gelb heißt: Claude wartet auf dich. ⌥N springt zur nächsten wartenden Session.")),
        TipText::Other(text) => text.clone(),
    }
}

/// Ansage für Bildschirmleser: „<Titel> wartet“ bzw. „<Titel> fertig“.
pub fn announcement(title: &str, waiting: bool) -> String {
    if waiting { trd!("%@ wartet", title) } else { trd!("%@ fertig", title) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn announcements_name_the_session_and_the_state() {
        assert_eq!(announcement("Alpha", true), "Alpha wartet");
        assert_eq!(announcement("Alpha", false), "Alpha fertig");
    }

    #[test]
    fn the_waiting_tip_is_written_for_the_system() {
        let text = tip_text(&TipText::WaitingMeansYellow);
        assert!(text.starts_with("Gelb heißt: Claude wartet auf dich."));
        if cfg!(not(target_os = "macos")) {
            assert!(text.contains("Alt+N") && !text.contains('⌥'), "{text}");
        }
        assert_eq!(tip_text(&TipText::Other("x".into())), "x");
    }
}
