//! Gemeinsame Bauteile des Instrument-Looks: Fugen, Tastenkante, Fensterknöpfe.
//! Werte stammen aus `app/Kadrell/Theme+Instrument.swift`.

use gpui::{
    App, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, Window, WindowControlArea, div, hsla,
    px, rgb,
};

use super::theme;
use crate::theme::Theme;

/// Höhe der oberen Leiste, die zugleich die Titelleiste ist.
pub const BAR_HEIGHT: f32 = 32.;

fn black(alpha: f32) -> Hsla {
    hsla(0., 0., 0., alpha)
}

fn white(alpha: f32) -> Hsla {
    hsla(0., 0., 1., alpha)
}

/// Dunkle Hälfte der Fuge: in hellen Schemata deutlich schwächer.
pub fn seam_dark(t: &Theme) -> Hsla {
    black(if t.dark { 0.6 } else { 0.13 })
}

/// Helle Hälfte der Fuge: in hellen Schemata deutlich stärker.
pub fn seam_light(t: &Theme) -> Hsla {
    white(if t.dark { 0.05 } else { 0.65 })
}

/// Behält `keep` (0 bis 1) der Helligkeit einer Farbe, zieht sie also Richtung Schwarz.
fn darkened(c: Hsla, keep: f32) -> Hsla {
    Hsla { l: c.l * keep, ..c }
}

/// Kante und Dicke der Taste.
pub fn key_edge(t: &Theme) -> Hsla {
    if t.dark { darkened(t.bg, 0.7) } else { darkened(t.line, 0.55) }
}

/// Display-Grund erhabener Status-Anzeigen, in allen Schemata gleich dunkel.
pub fn screen_face(t: &Theme) -> Hsla {
    rgb(if t.dark { 0x050505 } else { 0x0d0d0c }).into()
}

pub fn screen_ink() -> Hsla {
    rgb(0xefe9dc).into()
}

pub fn screen_muted() -> Hsla {
    rgb(0x8f8b81).into()
}

/// Waagrechte Fuge: dunkle Linie oben, helle direkt darunter.
pub fn h_seam(t: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w_full()
        .child(div().h(px(1.)).w_full().bg(seam_dark(t)))
        .child(div().h(px(1.)).w_full().bg(seam_light(t)))
}

/// Senkrechte Fuge: dunkle Linie links, helle direkt rechts.
pub fn v_seam(t: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .h_full()
        .child(div().w(px(1.)).h_full().bg(seam_dark(t)))
        .child(div().w(px(1.)).h_full().bg(seam_light(t)))
}

const CAPTION_WIDTH: f32 = 46.;

fn caption_button(
    id: &'static str,
    area: WindowControlArea,
    hover: Hsla,
    glyph: impl IntoElement,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .w(px(CAPTION_WIDTH))
        .h_full()
        .window_control_area(area)
        .hover(move |s| s.bg(hover))
        .child(glyph)
}

fn minimize_glyph(color: Hsla) -> Div {
    div().w(px(10.)).h(px(1.)).bg(color)
}

fn maximize_glyph(color: Hsla, maximized: bool) -> Div {
    let size = if maximized { 8. } else { 10. };
    div().size(px(size)).border_1().border_color(color)
}

fn close_glyph(color: Hsla) -> Div {
    div().text_size(px(15.)).text_color(color).child("\u{00d7}")
}

/// Minimieren, Maximieren, Schließen im Kadrell-Stil. Unter Windows zeichnet die App die Titelleiste selbst,
/// das System liefert nur die Trefferflächen (`window_control_area`).
pub fn window_controls(window: &Window, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let hover = t.surface;
    let red: Hsla = rgb(0xe81123).into();
    let ink = t.sub;
    div()
        .flex()
        .flex_none()
        .h_full()
        .child(caption_button("cap-min", WindowControlArea::Min, hover, minimize_glyph(ink)))
        .child(caption_button("cap-max", WindowControlArea::Max, hover, maximize_glyph(ink, window.is_maximized())))
        .child(caption_button("cap-close", WindowControlArea::Close, red, close_glyph(ink)))
}
