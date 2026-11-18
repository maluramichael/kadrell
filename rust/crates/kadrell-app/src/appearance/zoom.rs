//! Terminal-Schrift per Tastatur: ⌘+ / ⌘- / ⌘0 (Strg unter Windows) ändern die Schriftgröße für alle Terminals,
//! `menuFontBigger`, `menuFontSmaller`, `menuFontReset` der Mac-App. Die Größe landet in den Einstellungen des Profils.

use gpui::{Context, Div, InteractiveElement, KeyBinding, actions};

use crate::Shell;
use crate::state::attention::patch_setting;

use super::{DEFAULT_SIZE, clamp_font_size};

actions!(kadrell_zoom, [FontBigger, FontSmaller, FontReset]);

/// Neue Größe nach einem Schritt (`delta`) oder, ohne, zurück auf die Standardgröße.
pub fn next_size(current: f64, delta: Option<f64>) -> f64 {
    clamp_font_size(delta.map_or(DEFAULT_SIZE, |d| current + d))
}

/// Tasten: Strg bzw. Cmd mit `=`/`+` (je nach Layout), `-` und `0`.
pub fn bind_keys(cx: &mut gpui::App) {
    macro_rules! mods {
        ($m:literal) => {
            cx.bind_keys([
                KeyBinding::new(concat!($m, "-="), FontBigger, None),
                KeyBinding::new(concat!($m, "-+"), FontBigger, None),
                KeyBinding::new(concat!($m, "-shift-="), FontBigger, None),
                KeyBinding::new(concat!($m, "--"), FontSmaller, None),
                KeyBinding::new(concat!($m, "-0"), FontReset, None),
            ])
        };
    }
    if cfg!(target_os = "macos") {
        mods!("cmd");
    } else {
        mods!("ctrl");
    }
}

/// Aktionen des Fensters.
pub fn attach_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|this, _: &FontBigger, _, cx| this.font_step(Some(1.), cx)))
        .on_action(cx.listener(|this, _: &FontSmaller, _, cx| this.font_step(Some(-1.), cx)))
        .on_action(cx.listener(|this, _: &FontReset, _, cx| this.font_step(None, cx)))
}

impl Shell {
    /// Schriftgröße der Terminals um `delta` ändern, ohne `delta` auf die Standardgröße; wirkt sofort in allen
    /// Terminals und in allen Fenstern.
    pub(crate) fn font_step(&mut self, delta: Option<f64>, cx: &mut Context<Self>) {
        let size = next_size(self.state.settings.terminal_font_size(), delta);
        self.state.settings.set_terminal_font_size(size);
        patch_setting(&self.state.profile.settings_path(), "terminal.fontSize", serde_json::json!(size));
        super::apply(cx, &self.state.settings, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_by_one_point_and_stays_in_range() {
        assert_eq!(next_size(12., Some(1.)), 13.);
        assert_eq!(next_size(12., Some(-1.)), 11.);
        assert_eq!(next_size(72., Some(1.)), 72.);
        assert_eq!(next_size(6., Some(-1.)), 6.);
    }

    #[test]
    fn zero_returns_to_the_default_size() {
        assert_eq!(next_size(30., None), 12.);
    }
}
