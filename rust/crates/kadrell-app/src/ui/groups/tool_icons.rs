//! Symbole der schwebenden Leiste an Baumzeilen und Kacheln (Herz, Plus, Stift, Kreuz, Kreis, Quadrat, Minus,
//! Terminal, Server, π) und der Ordner-Taste im Gruppen-Dialog, gezeichnet wie `Icons.swift`: Pfade, keine
//! Schriftzeichen. Die Maße stammen aus einem 16 x 16 großen Raster; `*_at(size, …)` zeichnet in einer Fläche
//! `size` x `size` (Herz, Stift, Terminal, Server skalieren mit, Plus, Kreuz, Kreis, Quadrat, Minus ziehen feste
//! Abstände vom Rand ab, wie die Mac-App).

use gpui::{FontWeight, Hsla, IntoElement, ParentElement, Styled, div, px};

use crate::ui::icons::{Stroke, closed, cubic, dot, ellipse, icon, open, rect, rounded_rect, square_line};

const SIZE: f32 = 16.;
/// Strichstärke der Symbole (`lineWidth = 1.5`).
const WIDTH: f32 = 1.5;
/// Strichstärke von Terminal und Server (`lineWidth = 1.3`).
const THIN: f32 = 1.3;

/// Rechnet Linienzüge aus dem 16er-Raster auf `size` um (Verschiebung `offset` danach).
fn scaled(strokes: Vec<Stroke>, size: f32, offset: f32) -> Vec<Stroke> {
    let k = size / SIZE;
    strokes
        .into_iter()
        .map(|s| Stroke { points: s.points.iter().map(|p| (offset + p.0 * k, offset + p.1 * k)).collect(), ..s })
        .collect()
}

/// UI-Größe (`Theme.scale`): Die Mac-App zeichnet die Symbole in logischen Punkten in einem skalierten Kontext, also
/// wachsen Striche, Abstände und Fläche gemeinsam.
fn ui_scale() -> f32 {
    crate::ui::overlay::scale().max(0.1)
}

/// Symbolfläche `size` (schon mal UI-Größe): `build` liefert die Linienzüge für die logische Größe `size / scale`, sie
/// werden dann mit der UI-Größe vergrößert, ebenso die Strichstärke `width`.
fn drawn(size: f32, color: Hsla, width: f32, build: impl FnOnce(f32) -> Vec<Stroke>) -> impl IntoElement {
    let s = ui_scale();
    icon(size, magnify(build(size / s), s), color, width * s)
}

fn magnify(strokes: Vec<Stroke>, s: f32) -> Vec<Stroke> {
    strokes.into_iter().map(|k| Stroke { points: k.points.iter().map(|p| (p.0 * s, p.1 * s)).collect(), ..k }).collect()
}

fn plus_strokes(size: f32) -> Vec<Stroke> {
    let (a, b, mid) = (3., size - 3., size / 2.);
    vec![square_line((mid, a), (mid, b), WIDTH), square_line((a, mid), (b, mid), WIDTH)]
}

fn x_strokes(size: f32) -> Vec<Stroke> {
    let (a, b) = (4., size - 4.);
    vec![square_line((a, a), (b, b), WIDTH), square_line((b, a), (a, b), WIDTH)]
}

/// Plus (neue Session in der Gruppe), Abstand 3 vom Rand.
pub fn plus(color: Hsla) -> impl IntoElement {
    plus_at(SIZE * ui_scale(), color)
}

pub fn plus_at(size: f32, color: Hsla) -> impl IntoElement {
    drawn(size, color, WIDTH, plus_strokes)
}

/// Kreuz (schließen), Abstand 4 vom Rand.
pub fn x(color: Hsla) -> impl IntoElement {
    x_at(SIZE * ui_scale(), color)
}

pub fn x_at(size: f32, color: Hsla) -> impl IntoElement {
    drawn(size, color, WIDTH, x_strokes)
}

/// Kreis-Umriss: Kachel aus der Ansicht nehmen, ohne die Session zu schließen (`Icons.circle`).
pub fn circle_at(size: f32, color: Hsla) -> impl IntoElement {
    drawn(size, color, WIDTH, |l| vec![ellipse(4., 4., l - 8., l - 8.)])
}

/// Quadrat-Umriss: Kachel auf die ganze Arbeitsfläche zoomen (`Icons.square`).
pub fn square_at(size: f32, color: Hsla) -> impl IntoElement {
    drawn(size, color, WIDTH, |l| vec![rect(4., 4., l - 8., l - 8.)])
}

/// Waagerechter Strich: den Zoom wieder verkleinern (`Icons.minus`).
pub fn minus_at(size: f32, color: Hsla) -> impl IntoElement {
    drawn(size, color, WIDTH, |l| vec![square_line((4., l / 2.), (l - 4., l / 2.), WIDTH)])
}

/// Stift: geschlossener Umriss mit Strich, quadratische Enden (`Icons.pen`).
fn pen_strokes() -> Vec<Stroke> {
    let body = closed(vec![(11.5, 2.5), (13.5, 4.5), (5., 13.), (3., 13.), (3., 11.)]);
    vec![body, square_line((10., 4.), (12., 6.), WIDTH)]
}

pub fn pen(color: Hsla) -> impl IntoElement {
    pen_at(SIZE * ui_scale(), color)
}

pub fn pen_at(size: f32, color: Hsla) -> impl IntoElement {
    drawn(size, color, WIDTH, |l| scaled(pen_strokes(), l, 0.))
}

/// Terminal: Monitor mit Fuß (Neue Session mit ⌘/Strg, ein Terminal ohne Claude; `Icons.computer`).
fn computer_strokes() -> Vec<Stroke> {
    vec![rounded_rect(1.5, 2., 13., 9., 1.5), open(&[(8., 11.), (8., 14.)]), open(&[(4.5, 14.), (11.5, 14.)])]
}

pub fn computer_at(size: f32, color: Hsla) -> impl IntoElement {
    drawn(size, color, THIN, |l| scaled(computer_strokes(), l, 0.))
}

pub fn computer(color: Hsla) -> impl IntoElement {
    computer_at(SIZE * ui_scale(), color)
}

/// Server-Rack: zwei Einschübe mit Status-LED (`Icons.server`).
fn server_strokes() -> Vec<Stroke> {
    vec![
        rounded_rect(2., 2., 12., 5., 1.),
        rounded_rect(2., 9., 12., 5., 1.),
        dot(10.5, 3.5, 2., 2.),
        dot(10.5, 10.5, 2., 2.),
    ]
}

pub fn server_at(size: f32, color: Hsla) -> impl IntoElement {
    drawn(size, color, THIN, |l| scaled(server_strokes(), l, 0.))
}

pub fn server(color: Hsla) -> impl IntoElement {
    server_at(SIZE * ui_scale(), color)
}

/// π: Pi mit lokalem Modell statt Claude (`Icons.pi`): Systemschrift in `size * 1,15`, halbfett, zentriert.
pub fn pi_at(size: f32, color: Hsla) -> impl IntoElement {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(size))
        .overflow_hidden()
        .text_size(px(size * 1.15))
        .line_height(px(size))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child("π")
}

/// Ordner für die Taste neben dem Pfadfeld.
pub fn folder(color: Hsla) -> impl IntoElement {
    let outline = closed(vec![(1.5, 4.), (6., 4.), (7.5, 5.5), (14.5, 5.5), (14.5, 12.5), (1.5, 12.5)]);
    let s = ui_scale();
    icon(SIZE * s, magnify(vec![outline], s), color, 1.4 * s)
}

/// Umriss des Herzens als Bézierpfad aus `Icons.heart` (16er-Raster): unten (8 | 13,5), links hoch, Mulde bei
/// (8 | 5,2), rechts wieder hinunter.
fn heart_points() -> Vec<(f32, f32)> {
    let mut pts = vec![(8., 13.5)];
    pts.extend(cubic((8., 13.5), (4.2, 11.), (1.8, 8.6), (1.8, 6.2)));
    pts.extend(cubic((1.8, 6.2), (1.8, 2.6), (6., 2.4), (8., 5.2)));
    pts.extend(cubic((8., 5.2), (10., 2.4), (14.2, 2.6), (14.2, 6.2)));
    pts.extend(cubic((14.2, 6.2), (14.2, 8.6), (11.8, 11.), (8., 13.5)));
    pts.pop();
    pts
}

fn heart_strokes(filled: bool) -> Vec<Stroke> {
    vec![Stroke { points: heart_points(), closed: true, fill: filled }]
}

/// Herz für Favoriten, in der Werkzeugleiste mit einem Punkt Rand (`insetBy(1, 1)`): gefüllt, wenn die Gruppe
/// Favorit ist, sonst nur der Umriss.
pub fn heart(color: Hsla, filled: bool) -> impl IntoElement {
    let s = ui_scale();
    icon(SIZE * s, magnify(scaled(heart_strokes(filled), SIZE - 2., 1.), s), color, WIDTH * s)
}

/// Herz in einer Fläche `size` x `size` ohne Rand (gefülltes Herz der Gruppenzeile, 12 x 12).
pub fn heart_at(size: f32, color: Hsla, filled: bool) -> impl IntoElement {
    drawn(size, color, WIDTH, move |l| scaled(heart_strokes(filled), l, 0.))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extent(strokes: &[Stroke]) -> (f32, f32, f32, f32) {
        let pts = strokes.iter().flat_map(|s| s.points.iter());
        pts.fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |b, p| (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1)))
    }

    #[test]
    fn heart_stays_inside_its_canvas() {
        for (x, y) in heart_points() {
            assert!((0.0..=SIZE).contains(&x) && (0.0..=SIZE).contains(&y), "{x} {y}");
        }
    }

    #[test]
    fn heart_starts_at_the_bottom_tip_and_spans_the_swift_box() {
        let pts = heart_points();
        assert_eq!(pts[0], (8., 13.5));
        let (x0, _, x1, y1) = extent(&heart_strokes(true));
        assert!((x0 - 1.8).abs() < 0.01 && (x1 - 14.2).abs() < 0.01 && (y1 - 13.5).abs() < 0.01);
    }

    #[test]
    fn plus_and_x_use_the_swift_insets() {
        assert_eq!(extent(&plus_strokes(16.)), (2.25, 2.25, 13.75, 13.75));
        let (x0, _, x1, _) = extent(&x_strokes(16.));
        assert!(x0 < 4. && x0 > 3.4 && x1 > 12. && x1 < 12.6);
    }

    #[test]
    fn pen_is_a_closed_outline_plus_a_capped_stroke() {
        let s = pen_strokes();
        assert!(s[0].closed && s[0].points.len() == 5);
        assert!(!s[1].closed);
    }

    #[test]
    fn server_has_two_boxes_and_two_filled_leds() {
        let s = server_strokes();
        assert_eq!(s.iter().filter(|x| x.fill).count(), 2);
        assert_eq!(s.iter().filter(|x| !x.fill).count(), 2);
    }

    #[test]
    fn computer_has_stem_and_base() {
        let s = computer_strokes();
        assert_eq!(s[1].points, vec![(8., 11.), (8., 14.)]);
        assert_eq!(s[2].points, vec![(4.5, 14.), (11.5, 14.)]);
    }

    #[test]
    fn magnify_grows_insets_with_the_ui_size() {
        // Plus in der logischen Größe 16, bei UI-Größe 1,5: Abstand 3 wird 4,5, die Fläche 24.
        let big = magnify(plus_strokes(16.), 1.5);
        let (x0, _, x1, _) = extent(&big);
        assert!((x0 - 2.25 * 1.5).abs() < 1e-4 && (x1 - 13.75 * 1.5).abs() < 1e-4);
        assert_eq!(magnify(vec![open(&[(2., 4.)])], 2.)[0].points, vec![(4., 8.)]);
    }

    #[test]
    fn scaled_keeps_the_ratio_and_offsets() {
        let s = scaled(vec![open(&[(8., 8.)])], 14., 1.);
        assert_eq!(s[0].points, vec![(8., 8.)]);
        let s = scaled(vec![open(&[(16., 0.)])], 12., 0.);
        assert_eq!(s[0].points, vec![(12., 0.)]);
    }
}
