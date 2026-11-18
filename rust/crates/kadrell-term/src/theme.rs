//! Farben, die das Terminal braucht. Die App setzt sie als GPUI-Global (`TermThemeGlobal`), das Terminal kennt
//! sonst nichts von ihrem Farbschema.

use gpui::{App, Global, Hsla, SharedString};

/// Terminal-Schrift: auf Mac und Windows die mitgelieferte (siehe `fonts`).
pub const FONT: &str = "JetBrainsMono Nerd Font Mono";

#[derive(Clone, Copy)]
pub struct TermTheme {
    /// Hintergrund und Cursor-Schrift.
    pub bg: Hsla,
    /// Standardschriftfarbe.
    pub fg: Hsla,
    /// Gedämpfte Schrift (DimForeground).
    pub sub: Hsla,
    /// Linie um die Kachel.
    pub line: Hsla,
    /// Auswahl und Fokus-Akzent.
    pub accent: Hsla,
    /// Die acht ANSI-Grundfarben (die hellen Varianten teilen sie).
    pub ansi: [Hsla; 8],
}

pub struct TermThemeGlobal(pub TermTheme);
impl Global for TermThemeGlobal {}

/// Schrift und Abstände der Terminals (Einstellungen Darstellung). Die App setzt sie als GPUI-Global und bei jeder
/// Änderung neu; das Terminal liest sie bei jedem Zeichnen, so wirken Änderungen sofort in allen Terminals.
#[derive(Clone, Debug, PartialEq)]
pub struct TermStyle {
    /// Schriftfamilie wie das Textsystem sie kennt.
    pub family: SharedString,
    /// Schriftgröße in logischen Punkten.
    pub size: f32,
    /// Faktor auf die Grundzeilenhöhe (1 = Standard).
    pub line_spacing: f32,
    /// Innenabstand der Fläche in Punkten, ringsum.
    pub padding: f32,
}

impl Default for TermStyle {
    fn default() -> Self {
        Self { family: FONT.into(), size: 12.0, line_spacing: 1.0, padding: 0.0 }
    }
}

impl TermStyle {
    /// Der gesetzte Stil, sonst der Standard.
    pub fn current(cx: &App) -> TermStyle {
        cx.try_global::<TermStyleGlobal>().map(|g| g.0.clone()).unwrap_or_default()
    }
}

pub struct TermStyleGlobal(pub TermStyle);
impl Global for TermStyleGlobal {}
