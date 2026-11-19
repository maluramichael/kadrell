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

/// Ganze Schritte aus einem Mausrad-Ereignis (`handleScroll` der Mac-App): ein Trackpad liefert Pixel (20 je Schritt),
/// ein Rad Zeilen (eine je Schritt). Der Rest bleibt für das nächste Ereignis, sonst sprängen Wischer zweistellig.
/// Gibt die Schritte und den neuen Rest zurück.
pub fn wheel_steps(dy: f32, precise: bool, rest: f32) -> (i32, f32) {
    let total = rest + if precise { dy / 20. } else { dy };
    let steps = total.trunc();
    (steps as i32, total - steps)
}

/// Zählt der Modifier als „nur ⌘“ (Windows: nur Strg)? Mit Umschalt oder Alt dazu gehört das Rad anderen Funktionen.
pub fn is_font_wheel(m: gpui::Modifiers, mac: bool) -> bool {
    let (primary, other) = if mac { (m.platform, m.control) } else { (m.control, m.platform) };
    primary && !other && !m.alt && !m.shift
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
    /// ⌘ (Strg) + Mausrad über der Arbeitsfläche: Schriftgröße wie ⌘+ und ⌘-. `true`: verbraucht.
    pub(crate) fn font_wheel(&mut self, ev: &gpui::ScrollEvent, cx: &mut Context<Self>) -> bool {
        if !is_font_wheel(ev.modifiers, cfg!(target_os = "macos")) || self.modal_open() {
            return false;
        }
        let precise = matches!(ev.delta, gpui::ScrollDelta::Pixels(_));
        let dy = ev.delta.pixel_delta(gpui::px(1.)).y.as_f32();
        let (steps, rest) = wheel_steps(dy, precise, self.ws.wheel_rest);
        self.ws.wheel_rest = rest;
        if steps != 0 {
            self.font_step(Some(f64::from(steps)), cx);
        }
        true
    }

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
    fn a_trackpad_collects_twenty_pixels_per_step_and_keeps_the_rest() {
        assert_eq!(wheel_steps(8., true, 0.), (0, 0.4));
        let (steps, rest) = wheel_steps(14., true, 0.4);
        assert_eq!(steps, 1);
        assert!((rest - 0.1).abs() < 1e-5, "{rest}");
        assert_eq!(wheel_steps(-45., true, 0.), (-2, -0.25));
    }

    #[test]
    fn a_wheel_steps_one_line_at_a_time() {
        assert_eq!(wheel_steps(3., false, 0.), (3, 0.));
        assert_eq!(wheel_steps(-1., false, 0.), (-1, 0.));
    }

    #[test]
    fn only_the_primary_modifier_alone_zooms() {
        let m = |control, platform, alt, shift| gpui::Modifiers { control, platform, alt, shift, ..Default::default() };
        assert!(is_font_wheel(m(true, false, false, false), false));
        assert!(is_font_wheel(m(false, true, false, false), true));
        assert!(!is_font_wheel(m(true, false, false, true), false), "Umschalt scrollt seitlich");
        assert!(!is_font_wheel(m(true, false, true, false), false));
        assert!(!is_font_wheel(m(true, false, false, false), true), "auf dem Mac ist ⌘ die Taste");
        assert!(!is_font_wheel(m(false, false, false, false), false));
    }

    #[test]
    fn zero_returns_to_the_default_size() {
        assert_eq!(next_size(30., None), 12.);
    }
}
