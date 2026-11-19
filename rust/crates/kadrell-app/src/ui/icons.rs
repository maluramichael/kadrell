//! Gezeichnete Strichsymbole wie `app/Kadrell/UI/Icons.swift`: keine Schriftzeichen, sondern Pfade.

use super::layout::LayoutMode;
use gpui::{Bounds, Hsla, IntoElement, PathBuilder, Pixels, Point, Styled, canvas, point, px};

/// Ein Linienzug in Symbolkoordinaten (Ursprung oben links), optional geschlossen oder gefüllt.
pub(super) struct Stroke {
    pub(super) points: Vec<(f32, f32)>,
    pub(super) closed: bool,
    /// Gefüllte Fläche statt Linie (LED-Punkte, gefülltes Herz).
    pub(super) fill: bool,
}

pub(super) fn open(points: &[(f32, f32)]) -> Stroke {
    Stroke { points: points.to_vec(), closed: false, fill: false }
}

/// Geschlossener Linienzug.
pub(super) fn closed(points: Vec<(f32, f32)>) -> Stroke {
    Stroke { points, closed: true, fill: false }
}

pub(super) fn rect(x: f32, y: f32, w: f32, h: f32) -> Stroke {
    closed(vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
}

/// Linie mit quadratischen Enden (`lineCapStyle = .square`): beide Enden um die halbe Strichstärke verlängert.
pub(super) fn square_line(a: (f32, f32), b: (f32, f32), width: f32) -> Stroke {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt().max(f32::EPSILON);
    let (ux, uy) = (dx / len * width / 2., dy / len * width / 2.);
    open(&[(a.0 - ux, a.1 - uy), (b.0 + ux, b.1 + uy)])
}

const ARC_STEPS: usize = 24;

/// Ellipse in dem Rechteck (`NSBezierPath(ovalIn:)`) als geschlossener Linienzug.
pub(super) fn ellipse(x: f32, y: f32, w: f32, h: f32) -> Stroke {
    let (cx, cy) = (x + w / 2., y + h / 2.);
    closed(
        (0..ARC_STEPS)
            .map(|i| {
                let t = i as f32 / ARC_STEPS as f32 * std::f32::consts::TAU;
                (cx + w / 2. * t.cos(), cy + h / 2. * t.sin())
            })
            .collect(),
    )
}

/// Gefüllte Ellipse (LED-Punkt).
pub(super) fn dot(x: f32, y: f32, w: f32, h: f32) -> Stroke {
    Stroke { fill: true, ..ellipse(x, y, w, h) }
}

/// Rechteck mit abgerundeten Ecken (`NSBezierPath(roundedRect:xRadius:yRadius:)`), Viertelkreise je Ecke.
pub(super) fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Stroke {
    const STEPS: usize = 6;
    let corners = [(x + w - r, y + r, -90.), (x + w - r, y + h - r, 0.), (x + r, y + h - r, 90.), (x + r, y + r, 180.)];
    let mut points = Vec::new();
    for (cx, cy, start) in corners {
        for k in 0..=STEPS {
            let a = (start + 90. * k as f32 / STEPS as f32).to_radians();
            points.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    closed(points)
}

/// Kubische Bézierkurve von `p0` nach `p3` als Punkte ohne den Startpunkt.
pub(super) fn cubic(p0: (f32, f32), c1: (f32, f32), c2: (f32, f32), p3: (f32, f32)) -> Vec<(f32, f32)> {
    const STEPS: usize = 12;
    (1..=STEPS)
        .map(|i| {
            let t = i as f32 / STEPS as f32;
            let u = 1. - t;
            let f = |a: f32, b: f32, c: f32, d: f32| u * u * u * a + 3. * u * u * t * b + 3. * u * t * t * c + t * t * t * d;
            (f(p0.0, c1.0, c2.0, p3.0), f(p0.1, c1.1, c2.1, p3.1))
        })
        .collect()
}

pub(super) fn to_screen(origin: Point<Pixels>, p: (f32, f32)) -> Point<Pixels> {
    point(origin.x + px(p.0), origin.y + px(p.1))
}

fn build(strokes: &[Stroke], origin: Point<Pixels>, width: f32, fill: bool) -> Option<gpui::Path> {
    let mut b = if fill { PathBuilder::fill() } else { PathBuilder::stroke(px(width)) };
    let mut any = false;
    for s in strokes.iter().filter(|s| s.fill == fill) {
        let pts: Vec<_> = s.points.iter().map(|p| to_screen(origin, *p)).collect();
        b.add_polygon(&pts, s.closed);
        any = true;
    }
    if any { b.build().ok() } else { None }
}

/// Zeichenfläche `size` x `size`, auf der die Linienzüge in `color` mit Strichstärke `width` liegen.
pub(super) fn icon(size: f32, strokes: Vec<Stroke>, color: Hsla, width: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            for fill in [false, true] {
                if let Some(path) = build(&strokes, bounds.origin, width, fill) {
                    window.paint_path(path, color);
                }
            }
        },
    )
    .size(px(size))
    .flex_none()
}

/// Gruppen-Chevron in Normalgröße.
pub fn chevron(open_state: bool, color: Hsla) -> impl IntoElement {
    chevron_scaled(open_state, color, 1.)
}

/// Gruppen-Chevron (16 x 16, Strich 1,5, mal UI-Größe `scale`): offen zeigt nach unten, zu nach rechts.
pub fn chevron_scaled(open_state: bool, color: Hsla, scale: f32) -> impl IntoElement {
    // Innenfläche 7 x 7 ab (4,5 | 4,5), Mitte bei 8.
    let (a, b) = (4.5, 11.5);
    let mid = 8.;
    let stroke = if open_state {
        open(&[(a, mid - 2.), (mid, mid + 2.), (b, mid - 2.)])
    } else {
        open(&[(mid - 2., a), (mid + 2., mid), (mid - 2., b)])
    };
    let stroke = Stroke { points: stroke.points.iter().map(|p| (p.0 * scale, p.1 * scale)).collect(), ..stroke };
    icon(16. * scale, vec![stroke], color, 1.5 * scale)
}

/// Pfeil der Leiste: nach links (`left`) oder rechts, 10 x 10.
pub fn arrow(left: bool, color: Hsla) -> impl IntoElement {
    let stroke = if left {
        open(&[(6., 1.5), (2.5, 5.), (6., 8.5)])
    } else {
        open(&[(4., 1.5), (7.5, 5.), (4., 8.5)])
    };
    icon(10., vec![stroke], color, 1.4)
}

/// Rechteck `i` der Layout-Symbole (x, y, Breite, Höhe).
#[derive(Clone, Copy)]
struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Area {
    fn max_x(self) -> f32 {
        self.x + self.w
    }
    fn max_y(self) -> f32 {
        self.y + self.h
    }
    fn mid_x(self) -> f32 {
        self.x + self.w / 2.
    }
    fn mid_y(self) -> f32 {
        self.y + self.h / 2.
    }
    fn frame(self) -> Stroke {
        rect(self.x, self.y, self.w, self.h)
    }
    /// Senkrechte Linie bei `x` von `from` bis zum unteren Rand.
    fn vline(self, x: f32, from: f32) -> Stroke {
        open(&[(x, from), (x, self.max_y())])
    }
}

fn hline(y: f32, from: f32, to: f32) -> Stroke {
    open(&[(from, y), (to, y)])
}

/// Grid: vier Quadrate.
fn grid_strokes(i: Area) -> Vec<Stroke> {
    let (w, h) = ((i.w - 2.) / 2., (i.h - 2.) / 2.);
    [(0., 0.), (1., 0.), (0., 1.), (1., 1.)].iter().map(|(c, r)| rect(i.x + c * (w + 2.), i.y + r * (h + 2.), w, h)).collect()
}

/// Stack: Rahmen mit Linien bei einem und zwei Dritteln der Höhe.
fn stack_strokes(i: Area) -> Vec<Stroke> {
    let at = |k: f32| i.y + k * i.h / 3.;
    vec![i.frame(), hline(at(1.), i.x, i.max_x()), hline(at(2.), i.x, i.max_x())]
}

/// Haupt + Spalte: Rahmen, Teilung bei 55 %, rechts davon in der Mitte quer.
fn main_strokes(i: Area) -> Vec<Stroke> {
    let x = i.x + i.w * 0.55;
    vec![i.frame(), i.vline(x, i.y), hline(i.mid_y(), x, i.max_x())]
}

/// Spirale: Rahmen, Mittelteilung, rechts eine Querlinie und darunter eine weitere Teilung bei drei Vierteln.
fn spiral_strokes(i: Area) -> Vec<Stroke> {
    let q = i.mid_x() + i.w / 4.;
    vec![i.frame(), i.vline(i.mid_x(), i.y), hline(i.mid_y(), i.mid_x(), i.max_x()), i.vline(q, i.mid_y())]
}

/// Frei: Rahmen, Teilung bei 40 %, links davon in der Mitte quer (`.custom` der Mac-App).
fn custom_strokes(i: Area) -> Vec<Stroke> {
    let x = i.x + i.w * 0.4;
    vec![i.frame(), i.vline(x, i.y), hline(i.mid_y(), i.x, x)]
}

/// Scrollen: zwei Spalten, die dritte ragt rechts hinaus.
fn scroll_strokes(i: Area) -> Vec<Stroke> {
    let w = (i.w - 2.) * 0.42;
    let x = i.x + 2. * w + 4.;
    vec![
        rect(i.x, i.y, w, i.h),
        rect(i.x + w + 2., i.y, w, i.h),
        open(&[(i.max_x(), i.y), (x, i.y), (x, i.max_y()), (i.max_x(), i.max_y())]),
    ]
}

/// Reihe: drei gleich breite Spalten, alle sichtbar.
fn row_strokes(i: Area) -> Vec<Stroke> {
    let w = (i.w - 4.) / 3.;
    (0..3).map(|c| rect(i.x + c as f32 * (w + 2.), i.y, w, i.h)).collect()
}

/// Linienzüge eines Layouts in der Fläche `i`.
fn layout_strokes(mode: LayoutMode, i: Area) -> Vec<Stroke> {
    match mode {
        LayoutMode::Grid => grid_strokes(i),
        LayoutMode::Main => main_strokes(i),
        LayoutMode::Spiral => spiral_strokes(i),
        LayoutMode::Custom => custom_strokes(i),
        LayoutMode::Scroll => scroll_strokes(i),
        LayoutMode::Row => row_strokes(i),
        LayoutMode::Stack => stack_strokes(i),
    }
}

/// Layout-Symbol der Leiste, 10 x 10, Strich 1,2 (`Icons.layout`): zeigt das aktuelle Layout. Die Fläche liegt
/// zwei Punkte vom Rand der Zeichenfläche.
pub fn layout(mode: LayoutMode, color: Hsla) -> impl IntoElement {
    icon(10., layout_strokes(mode, Area { x: 1., y: 1., w: 8., h: 8. }), color, 1.2)
}

/// Layout-Symbol der Leiste mal UI-Größe (`Theme.scale`): Fläche, Linienzüge und Strich wachsen gemeinsam.
pub fn layout_scaled(mode: LayoutMode, color: Hsla, scale: f32) -> impl IntoElement {
    let strokes = layout_strokes(mode, Area { x: 1., y: 1., w: 8., h: 8. });
    let strokes = strokes.into_iter().map(|k| Stroke { points: k.points.iter().map(|p| (p.0 * scale, p.1 * scale)).collect(), ..k }).collect();
    icon(10. * scale, strokes, color, 1.2 * scale)
}

/// Layout-Symbol fürs Menü (`Icons.layoutImage`): 16 x 16, die Fläche zwei Punkte vom Rand, Strich 1,2.
pub fn layout_menu(mode: LayoutMode, color: Hsla) -> impl IntoElement {
    icon(16., layout_strokes(mode, Area { x: 2., y: 2., w: 12., h: 12. }), color, 1.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(strokes: &[Stroke]) -> (f32, f32, f32, f32) {
        let pts = strokes.iter().flat_map(|s| s.points.iter());
        pts.fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, p| (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1)))
    }

    #[test]
    fn bar_layouts_fill_the_inner_square() {
        let i = Area { x: 1., y: 1., w: 8., h: 8. };
        for m in LayoutMode::ALL.into_iter().filter(|m| *m != LayoutMode::Scroll) {
            assert_eq!(bounds(&layout_strokes(m, i)), (1., 1., 9., 9.), "{m:?}");
        }
    }

    #[test]
    fn stack_has_frame_and_lines_at_thirds() {
        let s = stack_strokes(Area { x: 0., y: 0., w: 9., h: 9. });
        assert_eq!(s.len(), 3);
        assert_eq!(s[1].points, vec![(0., 3.), (9., 3.)]);
        assert_eq!(s[2].points, vec![(0., 6.), (9., 6.)]);
    }

    #[test]
    fn main_and_spiral_use_their_split_positions() {
        let i = Area { x: 0., y: 0., w: 20., h: 20. };
        assert_eq!(main_strokes(i)[1].points[0].0, 11.);
        assert_eq!(spiral_strokes(i)[3].points, vec![(15., 10.), (15., 20.)]);
    }

    #[test]
    fn square_line_extends_both_ends_by_half_the_width() {
        let s = square_line((4., 8.), (12., 8.), 1.5);
        assert_eq!(s.points, vec![(3.25, 8.), (12.75, 8.)]);
    }

    #[test]
    fn rounded_rect_stays_inside_its_box() {
        let (x0, y0, x1, y1) = bounds(&[rounded_rect(1.5, 2., 13., 9., 1.5)]);
        assert!((x0 - 1.5).abs() < 1e-4 && (y0 - 2.).abs() < 1e-4 && (x1 - 14.5).abs() < 1e-4 && (y1 - 11.).abs() < 1e-4);
    }
}
