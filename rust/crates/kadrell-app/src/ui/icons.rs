//! Gezeichnete Strichsymbole wie `app/Kadrell/UI/Icons.swift`: keine Schriftzeichen, sondern Pfade.

use super::layout::LayoutMode;
use gpui::{Bounds, Hsla, IntoElement, PathBuilder, Pixels, Point, Styled, canvas, point, px};

/// Ein Linienzug in Symbolkoordinaten (Ursprung oben links), optional geschlossen.
struct Stroke {
    points: Vec<(f32, f32)>,
    closed: bool,
}

fn open(points: &[(f32, f32)]) -> Stroke {
    Stroke { points: points.to_vec(), closed: false }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Stroke {
    Stroke { points: vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)], closed: true }
}

fn to_screen(origin: Point<Pixels>, p: (f32, f32)) -> Point<Pixels> {
    point(origin.x + px(p.0), origin.y + px(p.1))
}

fn build(strokes: &[Stroke], origin: Point<Pixels>, width: f32) -> Option<gpui::Path> {
    let mut b = PathBuilder::stroke(px(width));
    for s in strokes {
        let pts: Vec<_> = s.points.iter().map(|p| to_screen(origin, *p)).collect();
        b.add_polygon(&pts, s.closed);
    }
    b.build().ok()
}

/// Zeichenfläche `size` x `size`, auf der die Linienzüge in `color` mit Strichstärke `width` liegen.
fn icon(size: f32, strokes: Vec<Stroke>, color: Hsla, width: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            if let Some(path) = build(&strokes, bounds.origin, width) {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(size))
    .flex_none()
}

/// Gruppen-Chevron (16 x 16, Strich 1,5): offen zeigt nach unten, zu nach rechts.
pub fn chevron(open_state: bool, color: Hsla) -> impl IntoElement {
    // Innenfläche 7 x 7 ab (4,5 | 4,5), Mitte bei 8.
    let (a, b) = (4.5, 11.5);
    let mid = 8.;
    let stroke = if open_state {
        open(&[(a, mid - 2.), (mid, mid + 2.), (b, mid - 2.)])
    } else {
        open(&[(mid - 2., a), (mid + 2., mid), (mid - 2., b)])
    };
    icon(16., vec![stroke], color, 1.5)
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

/// Grid: vier Quadrate.
fn grid_strokes() -> Vec<Stroke> {
    let (i, w) = (1., (8. - 2.) / 2.);
    [(0., 0.), (1., 0.), (0., 1.), (1., 1.)].iter().map(|(c, r)| rect(i + c * (w + 2.), i + r * (w + 2.), w, w)).collect()
}

/// Haupt + Spalte: links ein großes Feld, rechts zwei übereinander.
fn main_strokes() -> Vec<Stroke> {
    vec![rect(1., 1., 3.5, 8.), rect(6., 1., 3., 3.5), rect(6., 5.5, 3., 3.5)]
}

/// Spirale: großes Feld links, rechts oben, dann unten rechts und unten links kleiner.
fn spiral_strokes() -> Vec<Stroke> {
    vec![rect(1., 1., 4., 8.), rect(6., 1., 3., 4.), rect(6., 6., 1.5, 3.), rect(8., 6., 1., 3.)]
}

/// Stack: drei Zeilen, die mittlere hoch (aktive Kachel).
fn stack_strokes() -> Vec<Stroke> {
    vec![rect(1., 1., 8., 1.5), rect(1., 3.7, 8., 3.5), rect(1., 8.4, 8., 0.6)]
}

/// Frei: links oben ein Feld, rechts daneben die Teilung (`.custom` der Mac-App).
fn custom_strokes() -> Vec<Stroke> {
    vec![rect(1., 1., 8., 8.), open(&[(4.2, 1.), (4.2, 9.)]), open(&[(1., 5.), (4.2, 5.)])]
}

/// Scrollen: zwei Spalten, die dritte ragt rechts hinaus.
fn scroll_strokes() -> Vec<Stroke> {
    let w = (8. - 2.) * 0.42;
    let x = 1. + 2. * w + 4.;
    vec![rect(1., 1., w, 8.), rect(1. + w + 2., 1., w, 8.), open(&[(9., 1.), (x, 1.), (x, 9.), (9., 9.)])]
}

/// Reihe: drei gleich breite Spalten, alle sichtbar.
fn row_strokes() -> Vec<Stroke> {
    let w = (8. - 4.) / 3.;
    (0..3).map(|c| rect(1. + c as f32 * (w + 2.), 1., w, 8.)).collect()
}

/// Layout-Symbol der Leiste, 10 x 10, Strich 1,2 (`Icons.layout`): zeigt das aktuelle Layout.
pub fn layout(mode: LayoutMode, color: Hsla) -> impl IntoElement {
    let strokes = match mode {
        LayoutMode::Grid => grid_strokes(),
        LayoutMode::Main => main_strokes(),
        LayoutMode::Spiral => spiral_strokes(),
        LayoutMode::Custom => custom_strokes(),
        LayoutMode::Scroll => scroll_strokes(),
        LayoutMode::Row => row_strokes(),
        LayoutMode::Stack => stack_strokes(),
    };
    icon(10., strokes, color, 1.2)
}
