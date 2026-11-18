//! Gezeichnete Symbole der Kachelköpfe und der Spinner, Port von `Icons.swift` (x, pen, circle, square, minus, spinner).
//! Alle auf einer 16 x 16 Fläche, Strichstärke 1,5.

use std::f32::consts::PI;

use gpui::{Bounds, Hsla, IntoElement, LineCap, PathBuilder, PathStyle, Pixels, StrokeOptions, Styled, canvas, point, px};

use super::icons::{self, Stroke, open, rect};

const WIDTH: f32 = 1.5;

fn scaled(strokes: Vec<Stroke>, k: f32) -> Vec<Stroke> {
    strokes.into_iter().map(|s| Stroke { points: s.points.iter().map(|p| (p.0 * k, p.1 * k)).collect(), closed: s.closed }).collect()
}

fn x_strokes() -> Vec<Stroke> {
    vec![open(&[(4., 4.), (12., 12.)]), open(&[(12., 4.), (4., 12.)])]
}

fn pen_strokes() -> Vec<Stroke> {
    let body = Stroke { points: vec![(11.5, 2.5), (13.5, 4.5), (5., 13.), (3., 13.), (3., 11.)], closed: true };
    vec![body, open(&[(10., 4.), (12., 6.)])]
}

/// Kreis als Vieleck, Mitte (8, 8), Radius 4.
fn circle_strokes() -> Vec<Stroke> {
    let points = (0..32).map(|i| {
        let a = i as f32 / 32. * 2. * PI;
        (8. + 4. * a.cos(), 8. + 4. * a.sin())
    });
    vec![Stroke { points: points.collect(), closed: true }]
}

fn square_strokes() -> Vec<Stroke> {
    vec![rect(4., 4., 8., 8.)]
}

fn minus_strokes() -> Vec<Stroke> {
    vec![open(&[(4., 8.), (12., 8.)])]
}

fn glyph(strokes: Vec<Stroke>, color: Hsla, scale: f32) -> impl IntoElement {
    icons::icon(16. * scale, scaled(strokes, scale), color, WIDTH * scale)
}

/// Schließen-Kreuz.
pub fn x(color: Hsla, scale: f32) -> impl IntoElement {
    glyph(x_strokes(), color, scale)
}

/// Stift: umbenennen.
pub fn pen(color: Hsla, scale: f32) -> impl IntoElement {
    glyph(pen_strokes(), color, scale)
}

/// Kreis-Umriss: aus der Ansicht nehmen.
pub fn circle(color: Hsla, scale: f32) -> impl IntoElement {
    glyph(circle_strokes(), color, scale)
}

/// Quadrat-Umriss: zoomen.
pub fn square(color: Hsla, scale: f32) -> impl IntoElement {
    glyph(square_strokes(), color, scale)
}

/// Strich: Zoom verkleinern.
pub fn minus(color: Hsla, scale: f32) -> impl IntoElement {
    glyph(minus_strokes(), color, scale)
}

/// Punkte des Bogens: 270 Grad ab `start` (Grad) auf dem Kreis um (`cx`, `cy`) mit Radius `r`.
pub fn arc_points(cx: f32, cy: f32, r: f32, start: f32) -> Vec<(f32, f32)> {
    (0..=48)
        .map(|i| {
            let a = (start + 270. * i as f32 / 48.).to_radians();
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Spinner (`Icons.spinner`): Bogen von 270 Grad mit runden Enden, `size` x `size`, Strichstärke `width`; `turn` in 0..1
/// ist der Anteil einer Umdrehung, eine volle Drehung pro Sekunde.
pub fn spinner(size: f32, width: f32, color: Hsla, turn: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (cx, cy) = (bounds.origin.x.as_f32() + size / 2., bounds.origin.y.as_f32() + size / 2.);
            let radius = size / 2. - width / 2.;
            let options = StrokeOptions { line_width: width, line_cap: LineCap::Round, ..Default::default() };
            let mut b = PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(options));
            let pts: Vec<_> = arc_points(cx, cy, radius, turn * 360.).into_iter().map(|(x, y)| point(px(x), px(y))).collect();
            b.add_polygon(&pts, false);
            if let Ok(path) = b.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(size))
    .flex_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spinner_arc_spans_270_degrees_on_the_radius() {
        let pts = arc_points(0., 0., 10., 0.);
        assert_eq!(pts.len(), 49);
        assert!((pts[0].0 - 10.).abs() < 1e-4 && pts[0].1.abs() < 1e-4);
        let last = pts[48];
        assert!(last.0.abs() < 1e-3 && (last.1 + 10.).abs() < 1e-3, "270 Grad: oben");
        assert!(pts.iter().all(|p| ((p.0 * p.0 + p.1 * p.1).sqrt() - 10.).abs() < 1e-3));
    }

    #[test]
    fn glyphs_stay_inside_the_16_point_cell() {
        for strokes in [x_strokes(), pen_strokes(), circle_strokes(), square_strokes(), minus_strokes()] {
            assert!(strokes.iter().flat_map(|s| &s.points).all(|p| (0. ..=16.).contains(&p.0) && (0. ..=16.).contains(&p.1)));
        }
    }
}
