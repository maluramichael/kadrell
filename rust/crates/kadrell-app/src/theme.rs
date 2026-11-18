//! Farbschemata wie in `app/Kadrell/Theme.swift` (Reihenfolge der Werte dort: bg, panel, surface, line, fg,
//! sub, muted, detached, running, waiting, idle, error).

use gpui::{Hsla, rgb};

#[derive(Clone, Copy)]
pub struct Theme {
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
    pub ansi: [Hsla; 8],
    pub dark: bool,
}

fn hex(v: u32) -> Hsla {
    rgb(v).into()
}

impl Theme {
    fn new(dark: bool, c: [u32; 12], ansi: [u32; 8]) -> Self {
        let n = c.map(hex);
        Self {
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
            accent: n[8],
            ansi: ansi.map(hex),
            dark,
        }
    }

    pub fn mocha() -> Self {
        Self::new(
            true,
            [
                0x11111b, 0x181825, 0x1e1e2e, 0x313244, 0xcdd6f4, 0xa6adc8, 0x6c7086, 0x45475a, 0x89b4fa,
                0xf9e2af, 0xa6e3a1, 0xf38ba8,
            ],
            [0x45475a, 0xf38ba8, 0xa6e3a1, 0xf9e2af, 0x89b4fa, 0xf5c2e7, 0x94e2d5, 0xbac2de],
        )
    }

    pub fn latte() -> Self {
        Self::new(
            false,
            [
                0xeff1f5, 0xe6e9ef, 0xdce0e8, 0xccd0da, 0x4c4f69, 0x6c6f85, 0x8c8fa1, 0xacb0be, 0x1e66f5,
                0xdf8e1d, 0x40a02b, 0xd20f39,
            ],
            [0x5c5f77, 0xd20f39, 0x40a02b, 0xdf8e1d, 0x1e66f5, 0xea76cb, 0x179299, 0xacb0be],
        )
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
}

fn luminance(c: Hsla) -> f32 {
    let r = c.to_rgb();
    let lin = |v: f32| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * lin(r.r) + 0.7152 * lin(r.g) + 0.0722 * lin(r.b)
}

/// WCAG-Kontrastverhältnis zweier Farben.
fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

pub use kadrell_term::FONT;
