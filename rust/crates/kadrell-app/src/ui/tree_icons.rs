//! Gezeichnete Bausteine der Baumzeilen in der Größe, die `SidebarRenderer` ihnen gibt: Statuspunkt in allen Formen
//! (`Icons.statusDot`), Favoritenherz 12 x 12, Typ-Symbole Server, Computer und Pi (`Icons.server/computer/pi`).
//! Alle Maße kommen in Punkten schon mit der UI-Größe multipliziert.

use gpui::{BorderStyle, Bounds, Hsla, IntoElement, Pixels, Styled, canvas, fill, point, px, quad, size};

use super::groups::tool_icons;
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
    tool_icons::heart_at(size, color, true)
}

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

/// Das Typ-Symbol, 12 x 12 Punkte mit UI-Größe.
pub fn type_icon_element(kind: TypeIcon, color: Hsla, size: f32) -> gpui::AnyElement {
    match kind {
        TypeIcon::Server => tool_icons::server_at(size, color).into_any_element(),
        TypeIcon::Computer => tool_icons::computer_at(size, color).into_any_element(),
        TypeIcon::Pi => tool_icons::pi_at(size, color).into_any_element(),
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
}
