//! Gezeichnete Bausteine der Baumzeilen in der Größe, die `SidebarRenderer` ihnen gibt: Statuspunkt in allen Formen
//! (`Icons.statusDot`), Favoritenherz 12 x 12, Typ-Symbole Server, Computer und Pi (`Icons.server/computer/pi`).
//! Alle Maße kommen in Punkten schon mit der UI-Größe multipliziert.

use gpui::{Bounds, BorderStyle, FontWeight, Hsla, IntoElement, ParentElement, PathBuilder, Pixels, Styled, canvas, div, fill, point, px, quad, size};

use super::groups::tool_icons::heart_points;
use super::tree_rules::{self, DotShape, RING_INSET, RING_WIDTH};

fn rect_at(b: Bounds<Pixels>, (x, y, w, h): tree_rules::Frac) -> Bounds<Pixels> {
    let (bw, bh) = (b.size.width, b.size.height);
    Bounds::new(point(b.origin.x + bw * x, b.origin.y + bh * y), size(bw * w, bh * h))
}

fn disc(window: &mut gpui::Window, b: Bounds<Pixels>, color: Hsla) {
    window.paint_quad(fill(b, color).corner_radii(b.size.width / 2.));
}

fn ring(window: &mut gpui::Window, b: Bounds<Pixels>, color: Hsla, scale: f32) {
    let inset = px(RING_INSET * scale);
    let inner = Bounds::new(point(b.origin.x + inset, b.origin.y + inset), size(b.size.width - inset * 2., b.size.height - inset * 2.));
    window.paint_quad(quad(inner, inner.size.width / 2., gpui::transparent_black(), px(RING_WIDTH * scale), color, BorderStyle::Solid));
}

fn paint_marks(window: &mut gpui::Window, b: Bounds<Pixels>, shape: DotShape, mark: Hsla) {
    for (n, frac) in tree_rules::marks(shape).into_iter().enumerate() {
        let r = rect_at(b, frac);
        // Beim Ausrufezeichen ist der zweite Teil der Punkt unter dem Strich.
        let round = shape == DotShape::Bang && n == 1;
        window.paint_quad(fill(r, mark).corner_radii(if round { r.size.width / 2. } else { px(0.) }));
    }
}

/// Statuspunkt der Größe `diameter` (mit UI-Größe `scale`): Scheibe, bei `Ring` nur ein Ring, bei `Pause`/`Bang` mit
/// einem Zeichen in `mark` darauf.
pub fn status_dot(shape: DotShape, color: Hsla, mark: Hsla, diameter: f32, scale: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, (), window, _| match shape {
            DotShape::Ring => ring(window, b, color, scale),
            _ => {
                disc(window, b, color);
                paint_marks(window, b, shape, mark);
            }
        },
    )
    .size(px(diameter))
    .flex_none()
}

/// Gefülltes Herz der Größe `size` (Favorit in der Kopfzeile, 12 x 12) in Gruppenfarbe.
pub fn heart_filled(color: Hsla, size: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, (), window, _| {
            let k = size / 16.;
            let pts: Vec<_> = heart_points().into_iter().map(|(x, y)| point(b.origin.x + px(x * k), b.origin.y + px(y * k))).collect();
            let mut path = PathBuilder::fill();
            path.add_polygon(&pts, true);
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .size(px(size))
    .flex_none()
}

type Pt = (f32, f32);

/// Typ der Session für das Symbol hinter dem Punkt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TypeIcon {
    Server,
    Computer,
    Pi,
}

/// Reihenfolge wie `drawTypeIcon`: Remote vor Shell vor Pi, sonst kein Symbol.
pub fn type_icon(remote: bool, shell: bool, pi: bool) -> Option<TypeIcon> {
    if remote {
        Some(TypeIcon::Server)
    } else if shell {
        Some(TypeIcon::Computer)
    } else {
        pi.then_some(TypeIcon::Pi)
    }
}

fn stroke(path: &mut PathBuilder, pts: &[Pt], k: f32, origin: gpui::Point<Pixels>, closed: bool) {
    let pts: Vec<_> = pts.iter().map(|(x, y)| point(origin.x + px(x * k), origin.y + px(y * k))).collect();
    path.add_polygon(&pts, closed);
}

fn box_pts(x: f32, y: f32, w: f32, h: f32) -> [Pt; 4] {
    [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
}

fn paint_stroked(window: &mut gpui::Window, b: Bounds<Pixels>, k: f32, color: Hsla, shapes: &[(&[Pt], bool)]) {
    let mut path = PathBuilder::stroke(px(1.3 * k));
    for (pts, closed) in shapes {
        stroke(&mut path, pts, k, b.origin, *closed);
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn paint_leds(window: &mut gpui::Window, b: Bounds<Pixels>, k: f32, color: Hsla) {
    for y in [3.5, 10.5] {
        let mut path = PathBuilder::fill();
        stroke(&mut path, &box_pts(10.5, y, 2., 2.), k, b.origin, true);
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}

/// Server-Rack: zwei Einschübe mit Status-LED (`Icons.server`), `size` x `size` Punkte.
fn server(color: Hsla, size: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, (), window, _| {
            let k = size / 16.;
            let (top, bottom) = (box_pts(2., 2., 12., 5.), box_pts(2., 9., 12., 5.));
            paint_stroked(window, b, k, color, &[(&top, true), (&bottom, true)]);
            paint_leds(window, b, k, color);
        },
    )
    .size(px(size))
    .flex_none()
}

/// Bildschirm mit Fuß (`Icons.computer`).
fn computer(color: Hsla, size: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, (), window, _| {
            let k = size / 16.;
            let screen = box_pts(1.5, 2., 13., 9.);
            paint_stroked(window, b, k, color, &[(&screen, true), (&[(8., 11.), (8., 14.)], false), (&[(4.5, 14.), (11.5, 14.)], false)]);
        },
    )
    .size(px(size))
    .flex_none()
}

/// π in der Schrift der Zeile (`Icons.pi`: Größe 1,15 mal Höhe, halbfett, mittig).
fn pi(color: Hsla, size: f32) -> impl IntoElement {
    div().flex().flex_none().items_center().justify_center().size(px(size)).text_size(px(size * 1.15)).font_weight(FontWeight::SEMIBOLD).text_color(color).child("π")
}

/// Das Typ-Symbol, 12 x 12 Punkte mit UI-Größe.
pub fn type_icon_element(kind: TypeIcon, color: Hsla, size: f32) -> gpui::AnyElement {
    match kind {
        TypeIcon::Server => server(color, size).into_any_element(),
        TypeIcon::Computer => computer(color, size).into_any_element(),
        TypeIcon::Pi => pi(color, size).into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `drawTypeIcon`: Server vor Computer vor Pi.
    #[test]
    fn type_icon_order_is_remote_shell_pi() {
        assert_eq!(type_icon(true, true, true), Some(TypeIcon::Server));
        assert_eq!(type_icon(false, true, true), Some(TypeIcon::Computer));
        assert_eq!(type_icon(false, false, true), Some(TypeIcon::Pi));
        assert_eq!(type_icon(false, false, false), None);
    }

    #[test]
    fn led_boxes_lie_inside_the_server_rack() {
        for y in [3.5f32, 10.5] {
            let pts = box_pts(10.5, y, 2., 2.);
            assert!(pts.iter().all(|(x, y)| (2. ..=14.).contains(x) && (2. ..=14.).contains(y)));
        }
    }
}
