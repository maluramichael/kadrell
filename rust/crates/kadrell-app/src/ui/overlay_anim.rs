//! Ein- und Ausfahrt der fensterfüllenden Overlays (Einstellungen, Statistik) wie `OverlayLimit` in `OverlayPanel.swift`:
//! von oben herein mit Kurve (0,2 / 0,85 / 0,3 / 1) in 0,24 s ohne Nachfedern, hinaus mit (0,5 / 0 / 0,8 / 0,35) in 0,18 s.
//! Mit „Bewegung reduzieren“ fährt nichts: der Dialog blendet nur kurz ein (0,12 s) und aus (0,1 s). Die Kurven und
//! Zeiten sind rein rechnerisch (testbar ohne Fenster); `slide` hängt sie als GPUI-Animation an die Fläche.

use std::cell::Cell;
use std::time::Duration;

use gpui::{Animation, AnimationExt, AnyView, Div, ElementId, IntoElement, ParentElement, Styled, div, px};

/// Einfahrt: Dauer in Sekunden.
pub const SLIDE_IN_SECS: f32 = 0.24;
pub const SLIDE_OUT_SECS: f32 = 0.18;
/// Mit „Bewegung reduzieren“.
pub const REDUCED_IN_SECS: f32 = 0.12;
pub const REDUCED_OUT_SECS: f32 = 0.1;

/// Lage des Dialogs: `offset` ist der Anteil des Wegs nach oben hinaus (0 = eingefahren, 1 = ganz draußen), `opacity`
/// die Deckkraft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slide {
    pub offset: f32,
    pub opacity: f32,
}

pub fn in_secs(reduce: bool) -> f32 {
    if reduce { REDUCED_IN_SECS } else { SLIDE_IN_SECS }
}

/// Dauer der Ausfahrt (`OverlayLimit.slideOutDuration`): danach darf der Dialog verschwinden.
pub fn out_secs(reduce: bool) -> f32 {
    if reduce { REDUCED_OUT_SECS } else { SLIDE_OUT_SECS }
}

/// Kubische Bézier-Kurve wie `timingCurve(x1, y1, x2, y2)`: Zeit `x` in 0 bis 1 auf Fortschritt.
pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    let at = |t: f32, a: f32, b: f32| 3. * (1. - t) * (1. - t) * t * a + 3. * (1. - t) * t * t * b + t * t * t;
    let (mut lo, mut hi) = (0f32, 1f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if at(mid, x1, x2) < x { lo = mid } else { hi = mid }
    }
    at((lo + hi) / 2., y1, y2)
}

fn ease_out(x: f32) -> f32 {
    bezier(0., 0., 0.58, 1., x)
}

fn ease_in(x: f32) -> f32 {
    bezier(0.42, 0., 1., 1., x)
}

/// Lage bei Fortschritt `p` (0 bis 1 der Zeit) der Einfahrt.
pub fn slide_in_at(p: f32, reduce: bool) -> Slide {
    if reduce {
        return Slide { offset: 0., opacity: ease_out(p) };
    }
    Slide { offset: 1. - bezier(0.2, 0.85, 0.3, 1., p), opacity: 1. }
}

/// Lage bei Fortschritt `p` der Ausfahrt.
pub fn slide_out_at(p: f32, reduce: bool) -> Slide {
    if reduce {
        return Slide { offset: 0., opacity: 1. - ease_in(p) };
    }
    Slide { offset: bezier(0.5, 0., 0.8, 0.35, p), opacity: 1. }
}

thread_local! {
    static LEAVING: Cell<bool> = const { Cell::new(false) };
}

/// Fährt das offene fensterfüllende Overlay gerade hinaus? `slide` liest es, damit dieselbe Fläche die Ausfahrt spielt.
pub fn leaving() -> bool {
    LEAVING.with(Cell::get)
}

pub fn set_leaving(v: bool) {
    LEAVING.with(|l| l.set(v));
}

/// Hängt Ein- oder Ausfahrt an die Fläche. Ohne eigene Layout-Wirkung: die Fläche wird relativ verschoben. `travel` ist
/// der Weg nach oben in Pixeln (Höhe der Fläche). Läuft die Ausfahrt (`leaving`), spielt sie unter eigener Kennung neu.
pub fn slide(frame: Div, id: &'static str, travel: f32, reduce: bool) -> impl IntoElement {
    let out = leaving();
    let secs = if out { out_secs(reduce) } else { in_secs(reduce) };
    let key = ElementId::from((id, if out { 1usize } else { 0 }));
    div().flex().flex_col().flex_1().min_h_0().relative().child(frame.size_full().max_h_full()).with_animation(
        key,
        Animation::new(Duration::from_secs_f32(secs)),
        move |el, p| {
            let s = if out { slide_out_at(p, reduce) } else { slide_in_at(p, reduce) };
            el.top(px(-s.offset * travel)).opacity(s.opacity)
        },
    )
}

/// Ein Overlay, das nach dem Schließen noch ausfährt: Shell hält die Ansicht bis `until` und zeichnet sie weiter.
pub struct Closing {
    pub view: AnyView,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_in_runs_from_above_to_rest_without_overshoot() {
        assert!((slide_in_at(0., false).offset - 1.).abs() < 1e-3);
        assert!(slide_in_at(1., false).offset.abs() < 1e-3);
        let mut last = 1.1;
        for i in 0..=20 {
            let o = slide_in_at(i as f32 / 20., false).offset;
            assert!(o <= last + 1e-4 && o >= -1e-3, "kein Nachfedern: {o}");
            last = o;
        }
    }

    #[test]
    fn slide_out_runs_to_the_top() {
        assert!(slide_out_at(0., false).offset.abs() < 1e-3);
        assert!((slide_out_at(1., false).offset - 1.).abs() < 1e-3);
        assert!(slide_out_at(0.5, false).offset < slide_out_at(0.9, false).offset);
    }

    /// `testReduceMotionShortensSlide`: ohne Bewegung nur Ein- und Ausblenden, kürzer, nichts verschiebt sich.
    #[test]
    fn reduce_motion_only_fades_and_is_shorter() {
        assert!(out_secs(true) < out_secs(false));
        assert!(in_secs(true) < in_secs(false));
        for i in 0..=10 {
            let p = i as f32 / 10.;
            assert_eq!(slide_in_at(p, true).offset, 0.);
            assert_eq!(slide_out_at(p, true).offset, 0.);
        }
        assert!(slide_in_at(0., true).opacity < 0.01);
        assert!(slide_in_at(1., true).opacity > 0.99);
        assert!(slide_out_at(1., true).opacity < 0.01);
        assert_ne!(in_secs(false), out_secs(false));
    }

    #[test]
    fn durations_match_the_mac_app() {
        assert_eq!((SLIDE_IN_SECS, SLIDE_OUT_SECS), (0.24, 0.18));
        assert_eq!((REDUCED_IN_SECS, REDUCED_OUT_SECS), (0.12, 0.1));
    }

    #[test]
    fn leaving_flag_is_settable() {
        set_leaving(true);
        assert!(leaving());
        set_leaving(false);
        assert!(!leaving());
    }
}
