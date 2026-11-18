//! Verhalten der Arbeitsfläche, treu zu `WorkspaceView.swift`, `WorkspaceView+Hotkeys.swift` und `App/Hotkeys.swift`:
//! Kürzel aus den Einstellungen, Zoom, Sync, Vorschau, Auto-Modus, Tastatur im Baum, Fokus- und Layoutbefehle.
//! Die reinen Teile (Tabelle der Kürzel, Vorschau, Auto, Sync-Ziele, Baumtasten) sind ohne GPUI getestet, `actions`
//! hängt sie an die Shell.

pub mod actions;
pub mod auto;
mod dev;
pub mod keys;
mod modes;
pub mod preview;
pub mod sync;
pub mod tree_keys;

use kadrell_core::Settings;

use crate::ui::settings::shortcut;

/// Alles, was die Arbeitsfläche über die Auswahl und das Layout hinaus hält.
pub struct Ws {
    /// Kürzel aus den Einstellungen (`keys::Table::refresh` baut bei Änderung neu).
    pub keys: keys::Table,
    /// Sync: Eingaben gehen an alle offenen Kacheln. Bewusst nicht gespeichert.
    pub sync: bool,
    /// Vorschau aus dem Baum (⌥J/⌥K), ohne die Auswahl zu ändern.
    pub preview: preview::Preview,
    /// Auto-Modus (AUTO), gespeichert unter `workspace.auto`.
    pub auto: auto::Auto,
    /// Die Tastatur gehört dem Baum (⌘1), nicht Claude.
    pub tree_focus: bool,
}

impl Ws {
    /// Gehört die Tastatur gerade nicht dem Terminal der Fokus-Kachel (Baum, Vorschau)? Dann holt `Shell::sync` sie
    /// nicht zurück.
    pub fn keeps_focus(&self) -> bool {
        self.tree_focus || self.preview.is_active()
    }

    pub fn new(settings: &Settings) -> Ws {
        Ws {
            keys: keys::Table::new(settings, shortcut::current()),
            sync: false,
            preview: preview::Preview::default(),
            auto: auto::Auto::new(settings.bool_or(actions::AUTO_KEY, false)),
            tree_focus: false,
        }
    }
}
