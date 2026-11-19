//! Oberfläche der Lua-Extensions (Port von `app/Kadrell/Extensions/UI`, `FooterBarView` und den Extension-Teilen der
//! Statusleiste): rechte Seitenleiste und linkes Dock mit den Panels, Statuseinträge oben, Statusleiste unten,
//! Extensions-Dialog und die Einträge für ⌘P `>`.
//!
//! Aufbau: `model` (Zeilen eines Panels), `footer_model` (Platz der Module), `dialog_model` (Regeln des Dialogs) sind
//! reines Rust mit Tests; `panel`, `footer_bar` und `dialog` zeichnen mit GPUI; `keys` bindet die Kürzel; `shell`
//! hängt alles an die `Shell`.
//!
//! Kürzel: Strg+3 (Mac ⌘3) gibt der rechten Seitenleiste die Tastatur, Strg+Alt+B (⌘⌥B) blendet sie ein und aus. Der
//! Extensions-Dialog hat auf dem Mac ⌥F4. Unter Windows schließt Alt+F4 das Fenster, dort öffnet ihn Strg+Umschalt+E
//! (siehe `keys`); beides lässt sich in den Einstellungen umbelegen.

use gpui::{Hsla, actions};
use kadrell_ext::panel::ThemeColor;

use crate::theme::Theme;

pub mod dialog;
pub mod dialog_model;
pub mod footer_bar;
pub mod footer_model;
pub mod keys;
pub mod model;
pub mod panel;
mod shell;
pub mod spinner;

pub use shell::{ExtUi, attach_actions};

actions!(kadrell_ext_ui, [OpenExtensions, FocusRightSidebar, ToggleRightSidebar]);

/// Theme-Name einer Extension auf die Farben des Schemas, wie im Baum: Punkte in der Statusfarbe. `warn` bleibt auch
/// als Text das Gelb des Status-Punkts, mit einer Text-Variante wäre er von `accent` nicht zu unterscheiden.
pub fn status_color(c: ThemeColor, t: &Theme) -> Hsla {
    match c {
        ThemeColor::Accent => t.accent,
        ThemeColor::Muted => t.muted,
        ThemeColor::Ok => t.idle,
        ThemeColor::Warn => t.waiting,
        ThemeColor::Err => t.error,
    }
}

/// Farbe für Text: Akzent und Fehler in ihrer lesbaren Text-Rolle (mindestens 4,5:1 auf jeder Fläche), sonst wie
/// `status_color`.
pub fn text_color(c: ThemeColor, t: &Theme) -> Hsla {
    match c {
        ThemeColor::Accent => t.accent_text,
        ThemeColor::Err => t.error_text,
        other => status_color(other, t),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warn_is_the_yellow_dot_color_everywhere() {
        let t = Theme::mocha();
        assert_eq!(status_color(ThemeColor::Warn, &t), t.waiting);
        assert_eq!(text_color(ThemeColor::Warn, &t), t.waiting);
        assert_eq!(status_color(ThemeColor::Ok, &t), t.idle);
        assert_eq!(status_color(ThemeColor::Err, &t), t.error);
    }

    #[test]
    fn accent_and_error_text_use_their_readable_roles() {
        let t = Theme::latte();
        assert_eq!(text_color(ThemeColor::Accent, &t), t.accent_text);
        assert_eq!(text_color(ThemeColor::Err, &t), t.error_text);
        assert_eq!(text_color(ThemeColor::Muted, &t), t.muted);
    }
}
