//! Icons zur Laufzeit gezeichnet (kein Build-Schritt, keine Bilddatei): dunkles Rundquadrat mit Terminal-Zeichen
//! ">_" und einem Statuspunkt (gelb = wartet, grün = arbeitet), dazu das Zahlen-Overlay für die Taskleiste.
//! Ausgabe: BGRA, nicht vormultipliziert, Zeile für Zeile von oben (Windows-Icon-Format).
//! Das Rundquadrat ist ein Platzhalter in Kadrell-Farben; das echte App-Icon (`Kadrell.icns`) kommt mit dem Bundle.

use crate::model::Dot;

type Rgb = [f32; 3];

const BODY: Rgb = [0x26 as f32, 0x2A as f32, 0x33 as f32];
const GLYPH: Rgb = [0xE8 as f32, 0xEA as f32, 0xF0 as f32];
const WAITING: Rgb = [0xFF as f32, 0xC8 as f32, 0x3D as f32];
const WORKING: Rgb = [0x34 as f32, 0xC7 as f32, 0x59 as f32];
const BADGE: Rgb = [0xE5 as f32, 0x48 as f32, 0x4D as f32];

/// Ein Pixel, in das Ebenen "over" gemischt werden (nicht vormultipliziert).
#[derive(Clone, Copy, Default)]
struct Px {
    rgb: Rgb,
    a: f32,
}

impl Px {
    fn over(&mut self, color: Rgb, coverage: f32) {
        let a = coverage.clamp(0.0, 1.0);
        let out = a + self.a * (1.0 - a);
        if out <= 0.0 {
            return;
        }
        for (dst, c) in self.rgb.iter_mut().zip(color) {
            *dst = (c * a + *dst * self.a * (1.0 - a)) / out;
        }
        self.a = out;
    }
}

struct Canvas {
    size: u32,
    px: Vec<Px>,
}

impl Canvas {
    fn new(size: u32) -> Self {
        Self { size, px: vec![Px::default(); (size * size) as usize] }
    }

    /// Jede Ebene als Distanzfeld (negativ = innen), Kantenglättung über 1 Pixel.
    fn fill(&mut self, color: Rgb, sd: impl Fn(f32, f32) -> f32) {
        for y in 0..self.size {
            for x in 0..self.size {
                let d = sd(x as f32 + 0.5, y as f32 + 0.5);
                self.px[(y * self.size + x) as usize].over(color, 0.5 - d);
            }
        }
    }

    fn into_bgra(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in self.px {
            let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
            out.extend_from_slice(&[c(p.rgb[2]), c(p.rgb[1]), c(p.rgb[0]), c(p.a * 255.0)]);
        }
        out
    }
}

fn rounded_rect(cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> impl Fn(f32, f32) -> f32 {
    move |x, y| {
        let (dx, dy) = ((x - cx).abs() - (hw - r), (y - cy).abs() - (hh - r));
        (dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0)) - r
    }
}

fn segment(a: (f32, f32), b: (f32, f32), half_width: f32) -> impl Fn(f32, f32) -> f32 {
    move |x, y| {
        let (abx, aby) = (b.0 - a.0, b.1 - a.1);
        let t = (((x - a.0) * abx + (y - a.1) * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
        (x - (a.0 + abx * t)).hypot(y - (a.1 + aby * t)) - half_width
    }
}

fn circle(cx: f32, cy: f32, r: f32) -> impl Fn(f32, f32) -> f32 {
    move |x, y| (x - cx).hypot(y - cy) - r
}

/// Tray-Icon in `size` Pixeln (quadratisch): Rundquadrat, ">_", Statuspunkt unten rechts.
pub fn render_tray_icon(size: u32, dot: Dot) -> Vec<u8> {
    let s = size as f32;
    let mut c = Canvas::new(size);
    c.fill(BODY, rounded_rect(s / 2.0, s / 2.0, s / 2.0, s / 2.0, s * 0.22));
    let w = s * 0.07;
    c.fill(GLYPH, segment((s * 0.24, s * 0.28), (s * 0.44, s * 0.46), w));
    c.fill(GLYPH, segment((s * 0.44, s * 0.46), (s * 0.24, s * 0.64), w));
    c.fill(GLYPH, segment((s * 0.50, s * 0.68), (s * 0.70, s * 0.68), w));
    let color = match dot {
        Dot::None => return c.into_bgra(),
        Dot::Waiting => WAITING,
        Dot::Working => WORKING,
    };
    let (cx, cy, r) = (s * 0.72, s * 0.72, s * 0.22);
    c.fill(BODY, circle(cx, cy, r + s * 0.05));
    c.fill(color, circle(cx, cy, r));
    c.into_bgra()
}

/// 3x5-Pixelschrift: Ziffern 0 bis 9 und "+" (Index 10), je Zeile 3 Bits (MSB links).
const FONT: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b000, 0b010, 0b111, 0b010, 0b000],
];

/// Das Text-Label des Badges: "1" bis "9", ab 10 "9+".
pub fn badge_label(count: usize) -> String {
    if count > 9 { "9+".into() } else { count.to_string() }
}

fn glyph_index(ch: char) -> usize {
    ch.to_digit(10).map_or(10, |d| d as usize)
}

fn draw_glyph(c: &mut Canvas, idx: usize, x0: u32, y0: u32, scale: u32) {
    for (row, bits) in FONT[idx].iter().enumerate() {
        for col in 0..3u32 {
            if bits & (0b100 >> col) == 0 {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    let (x, y) = (x0 + col * scale + dx, y0 + row as u32 * scale + dy);
                    if x < c.size && y < c.size {
                        c.px[(y * c.size + x) as usize].over([255.0; 3], 1.0);
                    }
                }
            }
        }
    }
}

/// Zahlen-Overlay (rote Scheibe, weiße Zahl) für die Taskleiste, `size` meist 16.
pub fn render_badge(size: u32, count: usize) -> Vec<u8> {
    let s = size as f32;
    let mut c = Canvas::new(size);
    c.fill(BADGE, circle(s / 2.0, s / 2.0, s / 2.0 - 0.5));
    let label = badge_label(count);
    let scale = (size / 8).max(1);
    let n = label.chars().count() as u32;
    let width = n * 3 * scale + (n - 1) * scale;
    let (x0, y0) = ((size.saturating_sub(width)) / 2, size.saturating_sub(5 * scale) / 2);
    for (i, ch) in label.chars().enumerate() {
        draw_glyph(&mut c, glyph_index(ch), x0 + i as u32 * 4 * scale, y0, scale);
    }
    c.into_bgra()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(buf: &[u8], size: u32, x: u32, y: u32) -> u8 {
        buf[((y * size + x) * 4 + 3) as usize]
    }

    #[test]
    fn tray_icon_has_size_rounded_corners_and_opaque_center() {
        let b = render_tray_icon(32, Dot::None);
        assert_eq!(b.len(), 32 * 32 * 4);
        assert_eq!(alpha(&b, 32, 0, 0), 0, "Ecke bleibt durchsichtig");
        assert_eq!(alpha(&b, 32, 16, 3), 255);
    }

    #[test]
    fn dot_colors_differ() {
        let (w, k, n) = (render_tray_icon(32, Dot::Waiting), render_tray_icon(32, Dot::Working), render_tray_icon(32, Dot::None));
        assert_ne!(w, k);
        assert_ne!(w, n);
        // Pixel im Punkt (BGRA): gelb hat viel Rot, grün wenig.
        let i = ((23 * 32 + 23) * 4) as usize;
        assert!(w[i + 2] > 0xF0 && k[i + 2] < 0x60);
    }

    #[test]
    fn badge_label_caps_at_nine_plus() {
        assert_eq!(badge_label(3), "3");
        assert_eq!(badge_label(9), "9");
        assert_eq!(badge_label(10), "9+");
    }

    #[test]
    fn badge_draws_white_digit_on_red() {
        let b = render_badge(16, 8);
        assert_eq!(b.len(), 16 * 16 * 4);
        assert!(b.chunks(4).any(|p| p[0] == 255 && p[1] == 255 && p[2] == 255 && p[3] == 255));
        assert_eq!(alpha(&b, 16, 0, 0), 0);
    }
}
