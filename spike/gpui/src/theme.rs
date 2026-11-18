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

    /// Lesbare Schrift auf einer Akzentfläche (Marke „neu“, aktive Pille).
    pub fn pill_text(&self) -> Hsla {
        if self.dark { self.bg } else { hex(0xffffff) }
    }
}

/// Terminal-Schrift: auf dem Mac die der App, unter Windows die mitgelieferte.
pub const FONT: &str = "JetBrainsMono Nerd Font Mono";
