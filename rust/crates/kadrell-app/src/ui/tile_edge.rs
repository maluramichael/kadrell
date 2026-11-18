//! Rand und Ränder der Kacheln ohne eigene Zeichnung: `Instrument.topLeftRounded` (Radius auf die Fläche begrenzt),
//! `Instrument.rim` (Rand ohne Unterkante), die Maske einer Kachel, die seitlich unter die Modul-Ecke ragt
//! (`WorkspaceView.moduleMask`), und die Geisterebene für Kacheln, deren Session verschwunden ist (0,15 s ausblenden).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{Div, Hsla, Styled, div, px};

use super::layout::Rect;

/// Dauer, in der eine entfernte Kachel ausblendet (`reload`: `NSAnimationContext` 0,15 s).
pub const FADE_OUT: Duration = Duration::from_millis(150);

/// Radius der Ecke oben links auf die Fläche begrenzt (`topLeftRounded`: `min(max(radius, 0), width, height)`).
pub fn clamp_corner(radius: f32, w: f32, h: f32) -> f32 {
    radius.max(0.).min(w).min(h)
}

/// Rand einer eingesetzten Fläche ohne Unterkante (`Instrument.rim`): links und rechts, mit `top` auch oben, außen eine
/// Linie `dark`, direkt innen eine `light`. Mit `radius` folgt die Ecke oben links dem Bogen. Zwei absolute Ebenen
/// über die ganze Fläche `w` mal `h`.
pub fn rim(top: bool, radius: f32, w: f32, h: f32, dark: Hsla, light: Hsla) -> Vec<Div> {
    let layer = |inset: f32, color: Hsla, r: f32| {
        let d = div().absolute().left(px(inset)).right(px(inset)).top(px(if top { inset } else { 0. })).bottom_0().border_l_1().border_r_1().border_color(color);
        if top { d.border_t_1().rounded_tl(px(clamp_corner(r, w, h))) } else { d }
    };
    vec![layer(0., dark, radius), layer(1., light, radius - 1.)]
}

/// Muss eine Kachel mit dem Rahmen `f` (Koordinaten der Arbeitsfläche) von der Modul-Ecke mit dem Radius `r` verdeckt
/// werden? Ja, wenn sie das Eckquadrat berührt und der nächste Punkt darin außerhalb des Bogens liegt (Mittelpunkt
/// des Bogens: (r, r)). Nein für die Kachel in der Ecke selbst (sie rundet selbst), rechts oder unterhalb davon und
/// ganz links hinaus.
pub fn module_mask_needed(f: Rect, r: f32) -> bool {
    let (nx, ny) = (f.x.max(0.), f.y.max(0.));
    f.max_x() > 0. && f.max_y() > 0. && nx < r && ny < r && (nx - r).hypot(ny - r) > r
}

/// Punkte des Ausschnitts in Koordinaten der Arbeitsfläche: das Eckquadrat ohne den Viertelkreis (Bogen von oben nach
/// links, 32 Schritte). Das, was die Maske der Kachel entfernt.
pub fn module_cut(r: f32) -> Vec<(f32, f32)> {
    let mut pts = vec![(0., 0.), (r, 0.)];
    pts.extend((0..=32).map(|i| {
        let a = (270. - 90. * i as f32 / 32.).to_radians();
        (r + r * a.cos(), r + r * a.sin())
    }));
    pts
}

/// Verdeckt irgendeine der Kacheln seitlich die Modul-Ecke? Dann malt die Ecke ihr Gehäuse deckend, damit die Kachel
/// darunter nicht durchscheint.
pub fn any_under_corner(frames: &[Rect], r: f32) -> bool {
    frames.iter().any(|f| module_mask_needed(*f, r))
}

/// Eine ausblendende Kachel: der letzte Rahmen und der Beginn.
#[derive(Clone, Debug, PartialEq)]
pub struct Ghost {
    pub key: String,
    pub frame: Rect,
    pub since: Instant,
}

/// Merkt die Rahmen der gezeigten Kacheln und macht aus jeder Kachel, deren Session verschwindet, eine Geisterebene mit
/// dem letzten Rahmen. Wer nur aus der Ansicht genommen wird (Session besteht), blendet nicht aus.
#[derive(Default)]
pub struct Ghosts {
    last: HashMap<String, Rect>,
    live: Vec<Ghost>,
}

impl Ghosts {
    /// Ein Zeichnen: `shown` sind die Kacheln mit Rahmen, `exists` sagt, ob die Session noch besteht. Räumt abgelaufene
    /// Geister weg und gibt zurück, ob welche stehen (der Aufrufer zeichnet dann weiter).
    pub fn update(&mut self, now: Instant, shown: &[(String, Rect)], exists: &dyn Fn(&str) -> bool) {
        for (key, frame) in &self.last {
            if !shown.iter().any(|(k, _)| k == key) && !exists(key) {
                self.live.push(Ghost { key: key.clone(), frame: *frame, since: now });
            }
        }
        self.live.retain(|g| now.duration_since(g.since) < FADE_OUT);
        self.last = shown.iter().cloned().collect();
    }

    pub fn live(&self) -> &[Ghost] {
        &self.live
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corner_radius_never_exceeds_the_area() {
        assert_eq!(clamp_corner(12., 100., 100.), 12.);
        assert_eq!(clamp_corner(12., 8., 100.), 8.);
        assert_eq!(clamp_corner(12., 100., 5.), 5.);
        assert_eq!(clamp_corner(-3., 100., 100.), 0.);
    }

    #[test]
    fn a_tile_scrolled_under_the_corner_gets_a_mask_but_not_the_one_in_it() {
        let r = 14.;
        // Seitlich gescrollt: links des Bogens, oben an der Kante.
        assert!(module_mask_needed(Rect::new(-200., 6., 300., 400.), r));
        // Kachel in der Ecke (6, 6): nächster Punkt (6, 6), liegt innerhalb des Bogens (Abstand zu (14, 14) kleiner als 14).
        assert!(!module_mask_needed(Rect::new(6., 6., 300., 400.), r));
        // Rechts der Ecke und unterhalb davon.
        assert!(!module_mask_needed(Rect::new(14., 6., 300., 400.), r));
        assert!(!module_mask_needed(Rect::new(-200., 14., 300., 400.), r));
        // Ganz links hinaus.
        assert!(!module_mask_needed(Rect::new(-300., 6., 300., 400.), r));
        assert!(any_under_corner(&[Rect::new(6., 6., 10., 10.), Rect::new(-50., 0., 100., 50.)], r));
        assert!(!any_under_corner(&[Rect::new(6., 6., 10., 10.)], r));
    }

    #[test]
    fn the_cut_is_the_square_without_the_quarter_circle() {
        let pts = module_cut(14.);
        assert_eq!(pts.len(), 35);
        assert_eq!((pts[0], pts[1]), ((0., 0.), (14., 0.)));
        let (top, left) = (pts[2], pts[34]);
        assert!((top.0 - 14.).abs() < 1e-4 && top.1.abs() < 1e-4, "der Bogen beginnt oben: {top:?}");
        assert!(left.0.abs() < 1e-3 && (left.1 - 14.).abs() < 1e-3, "und endet links: {left:?}");
        assert!(pts[2..].iter().all(|p| ((p.0 - 14.).hypot(p.1 - 14.) - 14.).abs() < 1e-3));
    }

    #[test]
    fn a_removed_session_leaves_a_ghost_for_150_ms_and_a_deselected_one_does_not() {
        let t0 = Instant::now();
        let frame = Rect::new(6., 6., 300., 200.);
        let mut g = Ghosts::default();
        let shown = vec![("a".to_string(), frame), ("b".to_string(), frame)];
        g.update(t0, &shown, &|_| true);
        assert!(g.live().is_empty());
        // b aus der Ansicht genommen, die Session besteht: kein Geist.
        g.update(t0, &shown[..1], &|_| true);
        assert!(g.live().is_empty());
        // a verschwindet mit der Session: Geist mit dem letzten Rahmen.
        g.update(t0, &[], &|k| k != "a");
        assert_eq!(g.live().len(), 1);
        assert_eq!((g.live()[0].key.as_str(), g.live()[0].frame), ("a", frame));
        g.update(t0 + Duration::from_millis(100), &[], &|k| k != "a");
        assert_eq!(g.live().len(), 1, "nach 100 ms noch da, ohne Verdopplung");
        g.update(t0 + Duration::from_millis(151), &[], &|k| k != "a");
        assert!(g.live().is_empty());
    }
}
