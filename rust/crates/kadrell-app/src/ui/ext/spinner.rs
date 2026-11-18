//! Kleiner Fortschrittsring (`ProgressView().controlSize(.small)`): zwölf Striche, der hellste wandert im Kreis.
//! Zeichnet sich selbst und bittet bei jedem Bild um das nächste, solange er sichtbar ist.

use std::f32::consts::TAU;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::{Hsla, IntoElement, PathBuilder, Styled, canvas, point, px};

const SPOKES: usize = 12;
/// Eine Runde in Sekunden.
const PERIOD: f32 = 0.9;
const SIZE: f32 = 14.;

/// Deckkraft des Strichs `index`, wenn der hellste bei `lead` (in Strichen) steht: der Schweif hinter ihm klingt ab.
pub fn spoke_alpha(index: usize, lead: f32) -> f32 {
    let behind = (lead - index as f32).rem_euclid(SPOKES as f32);
    (1. - behind / SPOKES as f32).clamp(0.12, 1.)
}

fn lead_now() -> f32 {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0., |d| d.as_secs_f32());
    (secs % PERIOD) / PERIOD * SPOKES as f32
}

/// Der Ring in `color`, 14 px groß.
pub fn spinner(color: Hsla) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            window.request_animation_frame();
            let lead = lead_now().floor();
            let centre = point(bounds.origin.x + bounds.size.width / 2., bounds.origin.y + bounds.size.height / 2.);
            for i in 0..SPOKES {
                let angle = i as f32 / SPOKES as f32 * TAU;
                let (sin, cos) = angle.sin_cos();
                let (inner, outer) = (3.2_f32, 6.2_f32);
                let mut line = PathBuilder::stroke(px(1.4));
                line.move_to(point(centre.x + px(sin * inner), centre.y - px(cos * inner)));
                line.line_to(point(centre.x + px(sin * outer), centre.y - px(cos * outer)));
                if let Ok(path) = line.build() {
                    window.paint_path(path, Hsla { a: color.a * spoke_alpha(i, lead), ..color });
                }
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
    fn leading_spoke_is_brightest_and_the_tail_fades() {
        assert_eq!(spoke_alpha(3, 3.), 1.);
        assert!(spoke_alpha(2, 3.) < spoke_alpha(3, 3.));
        assert!(spoke_alpha(1, 3.) < spoke_alpha(2, 3.));
        assert!(spoke_alpha(4, 3.) >= 0.12, "der Strich vor dem hellsten ist der dunkelste, nie unsichtbar");
    }
}
