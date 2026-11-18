//! Farbschemata wie in `app/Kadrell/Theme.swift` (Reihenfolge der Werte dort: bg, panel, surface, line, fg,
//! sub, muted, detached, running, waiting, idle, error). Alle zwölf Schemata der Mac-App, mit denselben Ids.

use gpui::{Hsla, Rgba, rgb};

#[derive(Clone, Copy)]
pub struct Theme {
    /// Id wie in der Mac-App und in `settings.json` (`colorTheme`).
    pub id: &'static str,
    /// Anzeigename in der Auswahl der Einstellungen.
    pub name: &'static str,
    pub bg: Hsla,
    pub panel: Hsla,
    pub surface: Hsla,
    pub line: Hsla,
    pub fg: Hsla,
    pub sub: Hsla,
    pub muted: Hsla,
    pub detached: Hsla,
    pub running: Hsla,
    pub waiting: Hsla,
    pub idle: Hsla,
    pub error: Hsla,
    pub accent: Hsla,
    /// `accent`, aber lesbar als Text auf allen Dialogflächen (`Theme.accentText`).
    pub accent_text: Hsla,
    /// `error`, aber lesbar als Text auf allen Dialogflächen (`Theme.errorText`).
    pub error_text: Hsla,
    pub ansi: [Hsla; 8],
    pub dark: bool,
}

fn hex(v: u32) -> Hsla {
    rgb(v).into()
}

/// Farbe als sRGB-Komponenten 0 bis 1.
type Rgb = [f32; 3];

fn comps(c: Hsla) -> Rgb {
    let r: Rgba = c.into();
    [r.r, r.g, r.b]
}

fn from_comps(c: Rgb) -> Hsla {
    Rgba { r: c[0], g: c[1], b: c[2], a: 1. }.into()
}

fn lin(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

/// Relative Luminanz nach der W3C-Formel.
fn lum(c: Rgb) -> f32 {
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

fn contrast_rgb(a: Rgb, b: Rgb) -> f32 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// `color-mix(in srgb, a fraction, b)`.
fn mixed(a: Rgb, fraction: f32, b: Rgb) -> Rgb {
    [0, 1, 2].map(|i| a[i] * fraction + b[i] * (1. - fraction))
}

/// Verschiebt `c` Richtung `target` (bei Bedarf darüber hinaus), bis der Kontrast zu jeder Fläche in `backgrounds`
/// mindestens `ratio` erreicht (`NSColor.ensuringContrast`, binäre Suche mit Komponenten in 0...1).
fn ensuring_contrast(c: Rgb, ratio: f32, backgrounds: &[Rgb], target: Rgb) -> Rgb {
    if backgrounds.iter().all(|b| contrast_rgb(c, *b) >= ratio) {
        return c;
    }
    let at = |t: f32| [0, 1, 2].map(|i| (c[i] + (target[i] - c[i]) * t).clamp(0., 1.));
    let (mut lo, mut hi) = (0.0f32, 3.0f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if backgrounds.iter().all(|b| contrast_rgb(at(mid), *b) >= ratio) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    at(hi)
}

/// Erst Richtung `toward`, reicht das nicht, weiter Richtung `extreme` (`accessibleText` der Mac-App).
fn accessible(c: Rgb, backgrounds: &[Rgb], toward: Rgb, extreme: Rgb) -> Rgb {
    ensuring_contrast(ensuring_contrast(c, 4.5, backgrounds, toward), 4.5, backgrounds, extreme)
}

/// Rohwerte eines Schemas wie in `ColorTheme.all` der Mac-App.
struct Spec {
    id: &'static str,
    name: &'static str,
    dark: bool,
    colors: [u32; 12],
    ansi: [u32; 8],
    accent: Option<u32>,
}

const SPECS: [Spec; 12] = [
    Spec {
        id: "mocha",
        name: "Catppuccin Mocha",
        dark: true,
        colors: [0x11111b, 0x181825, 0x1e1e2e, 0x313244, 0xcdd6f4, 0xa6adc8, 0x6c7086, 0x45475a, 0x89b4fa, 0xf9e2af, 0xa6e3a1, 0xf38ba8],
        ansi: [0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de],
        accent: None,
    },
    Spec {
        id: "latte",
        name: "Catppuccin Latte",
        dark: false,
        colors: [0xeff1f5, 0xe6e9ef, 0xdce0e8, 0xccd0da, 0x4c4f69, 0x6c6f85, 0x8c8fa1, 0xacb0be, 0x1e66f5, 0xdf8e1d, 0x40a02b, 0xd20f39],
        ansi: [0x5c5f77, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xacb0be],
        accent: None,
    },
    Spec {
        id: "monokai",
        name: "Monokai",
        dark: true,
        colors: [0x1e1f1c, 0x272822, 0x3e3d32, 0x49483e, 0xf8f8f2, 0xcfcfc2, 0x75715e, 0x5b5a4e, 0x66d9ef, 0xe6db74, 0xa6e22e, 0xf92672],
        ansi: [0x272822, 0xf92672, 0xa6e22e, 0xf4bf75, 0x66d9ef, 0xae81ff, 0xa1efe4, 0xf8f8f2],
        accent: None,
    },
    Spec {
        id: "dracula",
        name: "Dracula",
        dark: true,
        colors: [0x191a21, 0x21222c, 0x282a36, 0x44475a, 0xf8f8f2, 0xc6c8d1, 0x6272a4, 0x565970, 0xbd93f9, 0xf1fa8c, 0x50fa7b, 0xff5555],
        ansi: [0x21222c, 0xff5555, 0x50fa7b, 0xf1fa8c, 0xbd93f9, 0xff79c6, 0x8be9fd, 0xf8f8f2],
        accent: None,
    },
    Spec {
        id: "nord",
        name: "Nord",
        dark: true,
        colors: [0x2e3440, 0x3b4252, 0x434c5e, 0x4c566a, 0xeceff4, 0xd8dee9, 0x7b88a1, 0x4c566a, 0x88c0d0, 0xebcb8b, 0xa3be8c, 0xbf616a],
        ansi: [0x3b4252, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x81a1c1, 0xb48ead, 0x88c0d0, 0xe5e9f0],
        accent: None,
    },
    Spec {
        id: "gruvbox",
        name: "Gruvbox Dark",
        dark: true,
        colors: [0x1d2021, 0x282828, 0x32302f, 0x3c3836, 0xebdbb2, 0xd5c4a1, 0x928374, 0x504945, 0x83a598, 0xfabd2f, 0xb8bb26, 0xfb4934],
        ansi: [0x282828, 0xcc241d, 0x98971a, 0xd79921, 0x458588, 0xb16286, 0x689d6a, 0xa89984],
        accent: None,
    },
    Spec {
        id: "one-dark",
        name: "One Dark",
        dark: true,
        colors: [0x21252b, 0x282c34, 0x2c313a, 0x3e4451, 0xabb2bf, 0x9da5b4, 0x5c6370, 0x4b5263, 0x61afef, 0xe5c07b, 0x98c379, 0xe06c75],
        ansi: [0x3f4451, 0xe06c75, 0x98c379, 0xe5c07b, 0x61afef, 0xc678dd, 0x56b6c2, 0xd7dae0],
        accent: None,
    },
    Spec {
        id: "solarized-dark",
        name: "Solarized Dark",
        dark: true,
        colors: [0x002b36, 0x00212b, 0x073642, 0x0e4552, 0x93a1a1, 0x839496, 0x586e75, 0x3b5560, 0x268bd2, 0xb58900, 0x859900, 0xdc322f],
        ansi: [0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5],
        accent: None,
    },
    Spec {
        id: "solarized-light",
        name: "Solarized Light",
        dark: false,
        colors: [0xfdf6e3, 0xeee8d5, 0xe4ddc8, 0xd9d2bd, 0x586e75, 0x657b83, 0x93a1a1, 0xc9c2ae, 0x268bd2, 0xb58900, 0x859900, 0xdc322f],
        ansi: [0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5],
        accent: None,
    },
    Spec {
        id: "github-light",
        name: "GitHub Light",
        dark: false,
        colors: [0xffffff, 0xf6f8fa, 0xeaeef2, 0xd0d7de, 0x1f2328, 0x424a53, 0x6e7781, 0xafb8c1, 0x0969da, 0x9a6700, 0x1a7f37, 0xcf222e],
        ansi: [0x24292f, 0xcf222e, 0x116329, 0x4d2d00, 0x0550ae, 0x8250df, 0x1b7c83, 0x6e7781],
        accent: None,
    },
    // Werte aus Monsters (github.com/Saturday-Company/monsters, Apache-2.0, docs/design.md).
    Spec {
        id: "instrument-dark",
        name: "Instrument Dark",
        dark: true,
        colors: [0x1c1c1a, 0x151514, 0x252523, 0x2e2e2b, 0xecebe5, 0xa3a19a, 0x8d8b85, 0x5c5a54, 0xecebe5, 0xff5e14, 0x4fd98a, 0xff3b2f],
        ansi: [0x252523, 0xff5a4e, 0x4fd98a, 0xf2c14e, 0x6ea8fe, 0xc792ea, 0x5fd4d0, 0xecebe5],
        accent: Some(0xff8a4c),
    },
    Spec {
        id: "instrument-light",
        name: "Instrument Light",
        dark: false,
        colors: [0xf2f1ec, 0xe3e1db, 0xfbfaf7, 0xcfccc4, 0x1a1a18, 0x5c5a54, 0x6b6963, 0xb5b2a9, 0x1a1a18, 0xff4f00, 0x1f8f4e, 0xd8231a],
        ansi: [0x1a1a18, 0xb51c14, 0x16703c, 0x8a6100, 0x1f5fbf, 0x8a3fb0, 0x0f7a78, 0x5c5a54],
        accent: Some(0xa83500),
    },
];

impl Theme {
    fn from_spec(s: &Spec) -> Self {
        let n = s.colors.map(hex);
        let accent = s.accent.map_or(n[8], hex);
        let c = |i: usize| comps(n[i]);
        let extreme = if s.dark { [1.; 3] } else { [0.; 3] };
        let dialog = dialog_surfaces(c(0), c(1), c(2), s.dark);
        Self {
            id: s.id,
            name: s.name,
            bg: n[0],
            panel: n[1],
            surface: n[2],
            line: n[3],
            fg: n[4],
            sub: n[5],
            muted: n[6],
            detached: n[7],
            running: n[8],
            waiting: n[9],
            idle: n[10],
            error: n[11],
            accent,
            accent_text: from_comps(ensuring_contrast(comps(accent), 4.5, &dialog, extreme)),
            error_text: from_comps(ensuring_contrast(c(11), 4.5, &dialog, extreme)),
            ansi: s.ansi.map(hex),
            dark: s.dark,
        }
    }

    /// Alle Schemata in der Reihenfolge der Mac-App.
    pub fn all() -> Vec<Theme> {
        SPECS.iter().map(Theme::from_spec).collect()
    }

    /// Schema zu einer Id, `None` bei Unbekanntem.
    pub fn by_id(id: &str) -> Option<Theme> {
        SPECS.iter().find(|s| s.id == id).map(Theme::from_spec)
    }

    pub fn mocha() -> Self {
        Self::from_spec(&SPECS[0])
    }

    pub fn latte() -> Self {
        Self::from_spec(&SPECS[1])
    }

    /// Erhabene Fläche der Dialoge: die hellste der drei Schemaflächen (`Theme.raised`).
    pub fn raised(&self) -> Hsla {
        from_comps(raised_of(comps(self.bg), comps(self.panel), comps(self.surface)))
    }

    /// Eingelassenes Feld für Werte (`Theme.well`): in dunklen Schemata `panel` Richtung Schwarz, in hellen `bg`.
    pub fn well(&self) -> Hsla {
        from_comps(well_of(comps(self.bg), comps(self.panel), self.dark))
    }

    /// Die Farben, die das Terminal braucht.
    pub fn term(&self) -> kadrell_term::TermTheme {
        kadrell_term::TermTheme {
            bg: self.bg,
            fg: self.fg,
            sub: self.sub,
            line: self.line,
            accent: self.accent,
            ansi: self.ansi,
        }
    }

    /// Gruppenfarbe wie `Theme.group` der Mac-App: auf hellem Grund 0,6 Anteil Farbe, 0,4 Schwarz.
    pub fn group(&self, c: u32) -> Hsla {
        if self.dark {
            return hex(c);
        }
        let ch = |shift: u32| (((c >> shift) & 0xff) as f32 * 0.6).round() as u32;
        hex((ch(16) << 16) | (ch(8) << 8) | ch(0))
    }

    /// Schrift auf einer Fläche wie `Theme.pillText(on:)`: `bg`, sonst Weiß, sonst Schwarz (Kontrast mindestens 4,5).
    pub fn pill_text_on(&self, face: Hsla) -> Hsla {
        let white = hex(0xffffff);
        if contrast(self.bg, face) >= 4.5 {
            self.bg
        } else if contrast(white, face) >= 4.5 {
            white
        } else {
            hex(0)
        }
    }

    /// Lesbare Schrift auf einer Akzentfläche (Marke „neu“, aktive Pille).
    pub fn pill_text(&self) -> Hsla {
        if self.dark { self.bg } else { hex(0xffffff) }
    }

    /// Beschriftung einer neutralen Taste: `fg`, bei zu wenig Kontrast auf der Kappe Richtung Schwarz oder Weiß
    /// (`KeyRole.label`).
    pub fn key_label(&self) -> Hsla {
        let extreme = if self.dark { [1.; 3] } else { [0.; 3] };
        from_comps(ensuring_contrast(comps(self.fg), 4.5, &[comps(self.surface)], extreme))
    }
}

fn raised_of(bg: Rgb, panel: Rgb, surface: Rgb) -> Rgb {
    [panel, bg, surface].into_iter().fold(surface, |best, c| if lum(c) > lum(best) { c } else { best })
}

fn well_of(bg: Rgb, panel: Rgb, dark: bool) -> Rgb {
    if dark { mixed(panel, 0.72, [0.; 3]) } else { bg }
}

/// Die Dialogflächen, gegen die Text lesbar sein muss: `panel`, `surface`, `raised`, `well`, `bg`.
fn dialog_surfaces(bg: Rgb, panel: Rgb, surface: Rgb, dark: bool) -> [Rgb; 5] {
    [panel, surface, raised_of(bg, panel, surface), well_of(bg, panel, dark), bg]
}

fn luminance(c: Hsla) -> f32 {
    lum(comps(c))
}

/// WCAG-Kontrastverhältnis zweier Farben.
fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

pub use kadrell_term::FONT;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_schemes_with_unique_ids() {
        let all = Theme::all();
        assert_eq!(all.len(), 12);
        let mut ids: Vec<_> = all.iter().map(|t| t.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 12);
        assert!(Theme::by_id("instrument-dark").is_some());
        assert!(Theme::by_id("nope").is_none());
    }

    #[test]
    fn accent_text_is_readable_on_dialog_surfaces() {
        for t in Theme::all() {
            for surface in [t.panel, t.surface, t.raised(), t.well(), t.bg] {
                assert!(contrast(t.accent_text, surface) >= 4.49, "{} accent text", t.id);
            }
        }
    }

    #[test]
    fn raised_is_the_brightest_surface() {
        let same = |a: Hsla, b: Hsla| (luminance(a) - luminance(b)).abs() < 1e-4;
        let latte = Theme::latte();
        assert!(same(latte.raised(), latte.bg));
        let mocha = Theme::mocha();
        assert!(same(mocha.raised(), mocha.surface));
        let light = Theme::by_id("instrument-light").unwrap();
        assert!(same(light.raised(), light.surface));
    }
}
