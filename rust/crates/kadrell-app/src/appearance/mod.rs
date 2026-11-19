//! Darstellung zur Laufzeit (README „Darstellung“): UI-Größe, Design des Baums, Laufzeit im Baum, Terminal-Schrift,
//! Zeilenabstand und Innenabstand, Kachelabstand, Hintergrundbild samt Deckkraft. Die Einstellungen stehen in
//! `kadrell-core`; hier werden sie zu einem GPUI-Global (`Appearance`), das Baum, Leiste, Kacheln und Terminals bei
//! jedem Zeichnen lesen. `apply` setzt es nach jeder Änderung im Einstellungen-Dialog und beim Start, ein Neustart
//! ist nie nötig (er würde alle Claude-Prozesse mitnehmen).

pub mod a11y;
pub mod breathing;
pub mod fonts;
pub mod metrics;
pub mod wallpaper;
pub mod window_color;
pub mod window_theme;
pub mod zoom;

use gpui::{App, Global, Hsla, SharedString};
use kadrell_core::settings::{DEFAULT_FONT_SIZE, FONT_SIZES};
use kadrell_core::Settings;
use kadrell_core::settings::SidebarStyle;
use kadrell_term::{TermStyle, TermStyleGlobal, TermThemeGlobal};

pub use metrics::RowMetrics;

use crate::ThemeGlobal;

#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    /// UI-Größe als Faktor (`Theme.scale`): Baum, Leiste, Kacheln, nicht die Terminals.
    pub scale: f32,
    pub sidebar: SidebarStyle,
    /// Laufzeit der Sessions im Baum (`sidebar.showAge`).
    pub show_age: bool,
    /// Zeile mit der letzten Nachricht unter jeder Session (`showLastMessage`).
    pub show_message: bool,
    pub font_family: String,
    pub font_size: f32,
    pub line_spacing: f32,
    /// Innenabstand der Terminals in Punkten.
    pub padding: f32,
    /// Abstand zwischen den Kacheln und zum Rand der Arbeitsfläche (`tiles.gap`).
    pub gap: f32,
    /// Pfad des Hintergrundbilds, leer ohne.
    pub wallpaper: String,
    /// Deckkraft von Baum und Kacheln, 1 = deckend (`tiles.opacity`).
    pub opacity: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance::from_settings(&Settings::default(), &[])
    }
}

impl Global for Appearance {}

impl Appearance {
    /// Aus den Einstellungen; `installed` sind die Familien des Systems für die Schriftwahl.
    pub fn from_settings(s: &Settings, installed: &[String]) -> Appearance {
        Appearance {
            scale: metrics::clamp_scale(s.ui_scale()),
            sidebar: s.sidebar_style(),
            show_age: s.sidebar_show_age(),
            show_message: s.show_last_message(),
            font_family: fonts::resolve_family(&s.terminal_font_name(), installed),
            font_size: s.terminal_font_size() as f32,
            line_spacing: s.terminal_line_spacing() as f32,
            padding: s.terminal_padding() as f32,
            gap: s.tile_gap() as f32,
            wallpaper: s.background_image(),
            opacity: s.tile_opacity().clamp(0., 1.) as f32,
        }
    }

    /// Wert in logischen Punkten mal UI-Größe.
    pub fn s(&self, v: f32) -> f32 {
        metrics::scaled(v, self.scale)
    }

    pub fn rows(&self) -> RowMetrics {
        RowMetrics::of(self.sidebar)
    }

    /// Stil für die Terminals.
    pub fn term_style(&self) -> TermStyle {
        TermStyle { family: SharedString::from(self.font_family.clone()), size: self.font_size, line_spacing: self.line_spacing, padding: self.padding }
    }

    /// Fläche in der Farbe des Schemas mit der Deckkraft von Baum und Kacheln, unter 100 % scheint das Bild durch.
    pub fn panel_fill(&self, color: Hsla) -> Hsla {
        Hsla { a: color.a * self.opacity, ..color }
    }
}

/// Der gesetzte Zustand; vor `apply` die Voreinstellung.
pub fn current(cx: &App) -> Appearance {
    cx.try_global::<Appearance>().cloned().unwrap_or_default()
}

/// Entwicklungs-Schalter für Screenshots ohne Eingabe: `KADRELL_DEV_SIDEBAR_STYLE=compact`, `KADRELL_DEV_UI_SCALE=130`
/// (Prozent), `KADRELL_DEV_WALLPAPER=<Pfad>`, `KADRELL_DEV_OPACITY=<Prozent>`. Sie überstimmen die Datei nur im Speicher.
fn with_dev_overrides(mut s: Settings) -> Settings {
    if let Ok(v) = std::env::var("KADRELL_DEV_SIDEBAR_STYLE") {
        s.set("sidebar.style", serde_json::json!(v));
    }
    if let Some(p) = std::env::var("KADRELL_DEV_UI_SCALE").ok().and_then(|v| v.parse::<f64>().ok()) {
        s.set_ui_scale(p / 100.);
    }
    if let Ok(v) = std::env::var("KADRELL_DEV_WALLPAPER") {
        s.set_background_image(&v);
    }
    if let Some(p) = std::env::var("KADRELL_DEV_OPACITY").ok().and_then(|v| v.parse::<f64>().ok()) {
        s.set_tile_opacity(p / 100.);
    }
    s
}

/// Liest die Einstellungen in `Appearance` und Terminal-Stil und lässt alle Fenster neu zeichnen. `dev` schaltet die
/// Entwicklungs-Schalter ein (nur beim Start, sonst gewänne die Datei nie wieder).
pub fn apply(cx: &mut App, settings: &Settings, dev: bool) {
    let settings = if dev { with_dev_overrides(settings.clone()) } else { settings.clone() };
    let installed = cx.text_system().all_font_names();
    let appearance = Appearance::from_settings(&settings, &installed);
    cx.set_global(TermStyleGlobal(appearance.term_style()));
    set_term_alpha(cx, appearance.opacity);
    cx.set_global(appearance);
    cx.refresh_windows();
}

/// Die Terminalfläche trägt dieselbe Deckkraft wie die Kacheln, sonst deckte sie das Hintergrundbild zu.
pub fn set_term_alpha(cx: &mut App, opacity: f32) {
    let Some(theme) = cx.try_global::<ThemeGlobal>().map(|t| t.0) else { return };
    let mut term = theme.term();
    term.bg = Hsla { a: term.bg.a * opacity, ..term.bg };
    cx.set_global(TermThemeGlobal(term));
}

/// Schriftgröße begrenzen wie `Settings.terminalFontSize`.
pub fn clamp_font_size(v: f64) -> f64 {
    v.clamp(FONT_SIZES.0, FONT_SIZES.1)
}

pub const DEFAULT_SIZE: f64 = DEFAULT_FONT_SIZE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_settings_defaults() {
        let a = Appearance::default();
        assert_eq!(a.scale, 1.);
        assert_eq!(a.sidebar, SidebarStyle::Classic);
        assert!(a.show_age && !a.show_message);
        assert_eq!((a.font_size, a.line_spacing, a.padding, a.gap, a.opacity), (12., 1., 0., 6., 1.));
        assert!(a.wallpaper.is_empty());
        assert_eq!(a.font_family, fonts::BUNDLED_FAMILY);
    }

    #[test]
    fn settings_flow_into_the_appearance() {
        let mut s = Settings::default();
        s.set_ui_scale(1.3);
        s.set_sidebar_style(SidebarStyle::Compact);
        s.set_terminal_font_size(15.);
        s.set_terminal_line_spacing(1.2);
        s.set_terminal_padding(4.);
        s.set_tile_gap(10.);
        s.set_tile_opacity(0.5);
        s.set_terminal_font_name("Consolas");
        let a = Appearance::from_settings(&s, &["Consolas".to_string()]);
        assert_eq!((a.scale, a.sidebar, a.font_size, a.padding, a.gap, a.opacity), (1.3, SidebarStyle::Compact, 15., 4., 10., 0.5));
        assert!((a.line_spacing - 1.2).abs() < 1e-6);
        assert_eq!(a.font_family, "Consolas");
        assert_eq!(a.rows(), RowMetrics::COMPACT);
        let style = a.term_style();
        assert_eq!((style.size, style.padding, style.family.as_ref()), (15., 4., "Consolas"));
    }

    #[test]
    fn scale_values_are_the_clamped_logical_points() {
        let mut s = Settings::default();
        s.set_ui_scale(1.3);
        let a = Appearance::from_settings(&s, &[]);
        assert!((a.s(24.) - 31.2).abs() < 1e-4);
        s.set_ui_scale(9.);
        assert_eq!(Appearance::from_settings(&s, &[]).scale, 2.);
    }

    #[test]
    fn opacity_scales_the_alpha_of_a_fill() {
        let mut s = Settings::default();
        s.set_tile_opacity(0.4);
        let a = Appearance::from_settings(&s, &[]);
        let c = gpui::rgb(0x112233).into();
        assert!((a.panel_fill(c).a - 0.4).abs() < 1e-6);
    }

    #[test]
    fn font_size_is_clamped_like_the_mac_app() {
        assert_eq!(clamp_font_size(500.), 72.);
        assert_eq!(clamp_font_size(1.), 6.);
        assert_eq!(DEFAULT_SIZE, 12.);
    }
}
