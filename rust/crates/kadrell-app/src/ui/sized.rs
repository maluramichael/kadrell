//! Inhalt, der von der Größe seiner Fläche im selben Frame abhängt (Kürzen in der Mitte, Beschriftungen, die umklappen).
//! Die Fläche füllt ihr Elternelement; `build` bekommt beim Vorbereiten des Zeichnens deren Rechteck und liefert den
//! Inhalt, der darin gesetzt und gezeichnet wird. Anders als Maße aus dem letzten Frame stimmt es schon im ersten.

use gpui::{App, AnyElement, AvailableSpace, Bounds, Pixels, Styled, Window, canvas};

/// Fläche, die ihren Inhalt aus ihrem eigenen Rechteck baut. Sie ist absolut und füllt das Elternelement (`inset_0`).
pub fn sized(build: impl FnOnce(Bounds<Pixels>, &mut Window, &mut App) -> AnyElement + 'static) -> gpui::Canvas<AnyElement> {
    canvas(
        move |bounds, window, cx| {
            let mut element = build(bounds, window, cx);
            element.prepaint_as_root(bounds.origin, bounds.size.map(AvailableSpace::Definite), window, cx);
            element
        },
        |_, mut element, window, cx| element.paint(window, cx),
    )
    .absolute()
    .size_full()
}
