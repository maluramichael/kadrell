//! Runde Ecke oben links an der Arbeitsfläche, Port von `ModuleCornerView.swift`. Liegt 2 px links und oberhalb der
//! Arbeitsfläche, also genau auf den geraden Fugen von Trenner und Leiste, und führt sie als Viertelbogen um die Ecke:
//! außen dunkel, innen hell, je 1 px. Außerhalb des Bogens Gehäuse wie der Baum, innerhalb durchsichtig. Lässt jeden Klick
//! durch. Mittelpunkt des Bogens ist die untere rechte Ecke der Fläche.

use gpui::{Bounds, IntoElement, PathBuilder, Pixels, Styled, canvas, fill, point, px, size};

use super::chrome::{seam_dark, seam_light};
use super::tile_look::{self, MODULE_SEAM};
use crate::appearance::Appearance;
use crate::theme::Theme;

/// Kantenlänge der Fläche in Pixeln: Radius plus Fuge.
pub fn extent(scale: f32) -> f32 {
    tile_look::module_radius(scale) + MODULE_SEAM
}

/// Punkte des Viertelbogens um die Mitte (`k`, `k`) mit Radius `r`, von oben (270 Grad) nach links (180 Grad).
pub fn arc(k: f32, r: f32) -> Vec<(f32, f32)> {
    (0..=32)
        .map(|i| {
            let a = (270. - 90. * i as f32 / 32.).to_radians();
            (k + r * a.cos(), k + r * a.sin())
        })
        .collect()
}

fn stroke_arc(window: &mut gpui::Window, origin: (f32, f32), k: f32, r: f32, color: gpui::Hsla) {
    let mut b = PathBuilder::stroke(px(1.));
    let pts: Vec<_> = arc(k, r).into_iter().map(|(x, y)| point(px(origin.0 + x), px(origin.1 + y))).collect();
    b.add_polygon(&pts, false);
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Die Ecke als Zeichenfläche; der Aufrufer setzt sie mit Versatz (-2, -2) an die Arbeitsfläche.
pub fn view(t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    let k = extent(a.scale);
    let (panel, housing, light, dark) = (t.panel, a.panel_fill(t.panel), seam_light(t), seam_dark(t));
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let origin = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
            let r = k - MODULE_SEAM;
            // Gehäuse außerhalb des Bogens: Quadrat ohne Viertelkreis, mit der Deckkraft der Kacheln.
            let mut outside = vec![(0., 0.), (k, 0.)];
            outside.extend(arc(k, r));
            outside.push((0., k));
            let mut b = PathBuilder::fill();
            let pts: Vec<_> = outside.into_iter().map(|(x, y)| point(px(origin.0 + x), px(origin.1 + y))).collect();
            b.add_polygon(&pts, true);
            if let Ok(path) = b.build() {
                window.paint_path(path, housing);
            }
            // Der obere Streifen liegt deckend, wie die Leiste darüber.
            window.paint_quad(fill(Bounds { origin: bounds.origin, size: size(px(k), px(MODULE_SEAM)) }, panel));
            stroke_arc(window, origin, k, r + 0.5, light);
            stroke_arc(window, origin, k, r + 1.5, dark);
        },
    )
    .absolute()
    .left(px(-MODULE_SEAM))
    .top(px(-MODULE_SEAM))
    .size(px(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corner_spans_radius_plus_seam_and_the_arc_runs_from_top_to_left() {
        assert_eq!(extent(1.), 16.);
        assert_eq!(extent(1.3), 20.);
        let pts = arc(16., 14.);
        assert_eq!(pts.len(), 33);
        assert!((pts[0].0 - 16.).abs() < 1e-4 && (pts[0].1 - 2.).abs() < 1e-4, "oben: (k, 2)");
        let last = pts[32];
        assert!((last.0 - 2.).abs() < 1e-3 && (last.1 - 16.).abs() < 1e-3, "links: (2, k)");
        assert!(pts.iter().all(|p| (((p.0 - 16.).powi(2) + (p.1 - 16.).powi(2)).sqrt() - 14.).abs() < 1e-3));
    }
}
