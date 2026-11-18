//! Kleiner Drehkreis neben „frage <Modell> …“ (Swift: `ProgressView().controlSize(.small)`): ein Dreiviertelbogen, der
//! sich in knapp einer Sekunde einmal dreht.

use std::f32::consts::TAU;
use std::time::Duration;

use gpui::{Animation, AnimationExt, Bounds, Div, Hsla, IntoElement, ParentElement, PathBuilder, Pixels, Styled, canvas, div, point, px};

const SIZE: f32 = 12.;
const STROKE: f32 = 1.5;
const SWEEP: f32 = TAU * 0.75;
const STEPS: usize = 24;

/// Punkte des Bogens um die Mitte, ab dem Winkel `turn` (Umdrehungen) im Uhrzeigersinn.
fn arc_points(turn: f32, radius: f32) -> Vec<(f32, f32)> {
    (0..=STEPS)
        .map(|i| {
            let angle = turn * TAU + SWEEP * i as f32 / STEPS as f32;
            (radius * angle.cos(), radius * angle.sin())
        })
        .collect()
}

fn arc(turn: f32, color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (c, r) = (bounds.center(), SIZE / 2. - STROKE);
            let pts: Vec<_> = arc_points(turn, r).into_iter().map(|(x, y)| point(c.x + px(x), c.y + px(y))).collect();
            let mut b = PathBuilder::stroke(px(STROKE));
            b.add_polygon(&pts, false);
            if let Ok(path) = b.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(SIZE))
}

pub fn spinner(color: Hsla) -> impl IntoElement {
    let base: Div = div().size(px(SIZE)).flex_none();
    base.with_animation("quick-start-spinner", Animation::new(Duration::from_millis(900)).repeat(), move |el, delta| el.child(arc(delta, color)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_stays_on_the_circle_and_covers_three_quarters() {
        let pts = arc_points(0., 5.);
        assert_eq!(pts.len(), STEPS + 1);
        assert!(pts.iter().all(|(x, y)| ((x * x + y * y).sqrt() - 5.).abs() < 0.001));
        let (a, b) = (pts[0], pts[STEPS]);
        let gap = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
        assert!(gap > 5., "Lücke von einem Viertel: Sehne {gap}");
        assert_ne!(arc_points(0.25, 5.)[0], pts[0], "dreht sich");
    }
}
