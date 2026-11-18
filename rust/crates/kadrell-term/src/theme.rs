//! Farben, die das Terminal braucht. Die App setzt sie als GPUI-Global (`TermThemeGlobal`), das Terminal kennt
//! sonst nichts von ihrem Farbschema.

use gpui::{Global, Hsla};

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
