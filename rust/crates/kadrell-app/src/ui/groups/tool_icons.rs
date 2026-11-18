//! Symbole der schwebenden Leiste an Baumzeilen (Herz, Plus, Stift, Kreuz, Terminal, Server) und der Ordner-Taste
//! im Gruppen-Dialog, gezeichnet wie `Icons.swift`: Pfade, keine Schriftzeichen. Zeichenfläche 16 x 16.

use gpui::{Bounds, Hsla, IntoElement, PathBuilder, Pixels, Styled, canvas, px};

use crate::ui::icons::{Stroke, icon, open, rect, to_screen};

const SIZE: f32 = 16.;
const WIDTH: f32 = 1.4;

pub fn plus(color: Hsla) -> impl IntoElement {
    icon(SIZE, vec![open(&[(8., 3.5), (8., 12.5)]), open(&[(3.5, 8.), (12.5, 8.)])], color, WIDTH)
}

pub fn x(color: Hsla) -> impl IntoElement {
    icon(SIZE, vec![open(&[(4.5, 4.5), (11.5, 11.5)]), open(&[(11.5, 4.5), (4.5, 11.5)])], color, WIDTH)
}

/// Stift: schräger Schaft mit Spitze unten links.
pub fn pen(color: Hsla) -> impl IntoElement {
    let body = Stroke { points: vec![(3., 13.), (4., 9.5), (10., 3.5), (12.5, 6.), (6.5, 12.), (3., 13.)], closed: true };
    icon(SIZE, vec![body, open(&[(8.5, 5.), (11., 7.5)])], color, WIDTH)
}

/// Terminal: Fenster mit Fuß (Neue Session mit ⌘/Strg, ein Terminal ohne Claude).
pub fn computer(color: Hsla) -> impl IntoElement {
    icon(SIZE, vec![rect(2.5, 3.5, 11., 7.), open(&[(5.5, 13.), (10.5, 13.)])], color, WIDTH)
}

/// Server: zwei übereinanderliegende Kästen (Remote-Gruppe).
pub fn server(color: Hsla) -> impl IntoElement {
    icon(SIZE, vec![rect(2.5, 2.5, 11., 4.), rect(2.5, 9., 11., 4.)], color, WIDTH)
}

/// Ordner für die Taste neben dem Pfadfeld.
pub fn folder(color: Hsla) -> impl IntoElement {
    let outline = Stroke { points: vec![(1.5, 4.), (6., 4.), (7.5, 5.5), (14.5, 5.5), (14.5, 12.5), (1.5, 12.5)], closed: true };
    icon(SIZE, vec![outline], color, WIDTH)
}

/// Herzumriss als Linienzug (Herzkurve, 16 x 16).
pub(crate) fn heart_points() -> Vec<(f32, f32)> {
    (0..48)
        .map(|i| {
            let t = i as f32 / 48. * std::f32::consts::TAU;
            let x = 16. * t.sin().powi(3);
            let y = 13. * t.cos() - 5. * (2. * t).cos() - 2. * (3. * t).cos() - (4. * t).cos();
            (8. + x * 0.43, 7.4 - y * 0.43)
        })
        .collect()
}

/// Herz, gefüllt wenn die Gruppe Favorit ist.
pub fn heart(color: Hsla, filled: bool) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let pts: Vec<_> = heart_points().into_iter().map(|p| to_screen(bounds.origin, p)).collect();
            let mut b = if filled { PathBuilder::fill() } else { PathBuilder::stroke(px(WIDTH)) };
            b.add_polygon(&pts, true);
            if let Ok(path) = b.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(SIZE))
    .flex_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heart_stays_inside_its_canvas() {
        let pts = heart_points();
        assert_eq!(pts.len(), 48);
        for (x, y) in pts {
            assert!((0.0..=SIZE).contains(&x) && (0.0..=SIZE).contains(&y), "{x} {y}");
        }
    }
}
