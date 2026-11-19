//! Spinner der Kachel (`Icons.spinner`). Die Strichsymbole der Kachelköpfe (x, pen, circle, square, minus) kommen aus
//! `groups::tool_icons`, der einen Quelle für die Geometrie aus `Icons.swift`.

use gpui::{Bounds, Hsla, IntoElement, LineCap, PathBuilder, PathStyle, Pixels, StrokeOptions, Styled, canvas, point, px};

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
}
