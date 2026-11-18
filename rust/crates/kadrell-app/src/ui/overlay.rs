//! Overlay-Rahmen: Abdunklung über dem ganzen Fenster, mittiger Dialog mit fester Breite und Höchsthöhe relativ
//! zum Fenster. Alles liegt absolut im Fenster und wird jeden Frame neu berechnet, folgt also jeder Größenänderung
//! sofort (kein einmal berechneter Rahmen). Werte stammen aus `app/Kadrell/Theme+Instrument.swift`.
//! Echten Weichzeichner gibt GPUI nicht her, deshalb nur die Abdunklung (30 %).

use gpui::{
    BoxShadow, ClickEvent, Div, Hsla, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Pixels, SharedString, Size, StatefulInteractiveElement, Styled, Window, App, div, hsla, point, px,
    relative, size,
};

use super::chrome::{h_seam, key_edge, seam_dark};
use crate::theme::Theme;

/// Dialogradius der Mac-App.
pub const DIALOG_RADIUS: f32 = 14.;

/// Dialogfläche: die hellste Schemafläche (`Theme.raised`).
pub fn raised(t: &Theme) -> Hsla {
    if t.dark { t.surface } else { t.bg }
}

/// Eingelassenes Feld (`Theme.well`): in dunklen Schemata `panel` 72 % zu Schwarz gemischt, sonst `bg`.
pub fn well(t: &Theme) -> Hsla {
    if t.dark { Hsla { l: t.panel.l * 0.72, ..t.panel } } else { t.bg }
}

/// Feste Dialogbreite wie `overlayWidth(_:)`: nie breiter als das Fenster (94 %), sonst `width`.
pub fn overlay_width(d: Div, width: f32) -> Div {
    d.w(px(width)).max_w(relative(0.94))
}

/// Dialoghöhe: höchstens `height`, aber nie mehr als 86 % des Fensters.
pub fn overlay_height(d: Div, height: f32) -> Div {
    d.h(px(height)).max_h(relative(0.86))
}

fn dim() -> Hsla {
    hsla(0., 0., 0., 0.3)
}

/// Abdunklung über dem ganzen Fenster, schluckt Maus und Rad und ruft `on_close` bei Klick daneben auf.
/// Der Dialog darin schluckt seine Klicks selbst (`dialog_frame`).
pub fn backdrop(on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(dim())
        .occlude()
        .on_mouse_down(MouseButton::Left, on_close)
}

/// Erhabene Dialogfläche mit festem Schatten nach unten und Fugenring.
pub fn dialog_frame(t: &Theme) -> Div {
    let shadow = BoxShadow {
        color: hsla(0., 0., 0., if t.dark { 0.5 } else { 0.22 }),
        offset: point(px(0.), px(3.)),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: false,
    };
    div()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(DIALOG_RADIUS))
        .bg(raised(t))
        .border_1()
        .border_color(seam_dark(t))
        .shadow(vec![shadow])
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// Titelzeile in Großbuchstaben, gedämpft.
pub fn dialog_title(text: impl Into<SharedString>, t: &Theme) -> Div {
    div().flex_none().px(px(16.)).pt(px(14.)).pb(px(2.)).text_size(px(11.)).font_weight(gpui::FontWeight::BOLD).text_color(t.sub).child(text.into())
}

/// Taste im Instrument-Look: Kante als Dicke, darauf die Kappe in Akzent.
pub fn key_button(label: impl Into<SharedString>, t: &Theme) -> Div {
    let cap = div()
        .px(px(14.))
        .py(px(4.))
        .rounded(px(4.))
        .bg(t.accent)
        .text_color(t.pill_text())
        .font_weight(gpui::FontWeight::BOLD)
        .child(label.into());
    div().flex_none().pb(px(2.)).rounded(px(5.)).bg(key_edge(t)).cursor_pointer().child(cap)
}

/// Fußzeile: Hinweise links, Haupttaste rechts. `on_click` startet.
pub fn dialog_foot(
    hint: impl Into<SharedString>,
    button: impl Into<SharedString>,
    t: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let key = key_button(button, t).id("dialog-primary").on_click(on_click);
    div()
        .flex()
        .flex_col()
        .flex_none()
        .child(h_seam(t))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(16.))
                .py(px(10.))
                .text_size(px(11.))
                .text_color(t.muted)
                .child(div().min_w_0().overflow_hidden().whitespace_nowrap().child(hint.into()))
                .child(key),
        )
}

/// Fenstergröße beim Start: `KADRELL_DEV_SIZE=1000x700` für Screenshots, sonst 1400 x 900.
pub fn window_size() -> Size<Pixels> {
    let parsed = std::env::var("KADRELL_DEV_SIZE").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?))
    });
    let (w, h) = parsed.unwrap_or((1400., 900.));
    size(px(w), px(h))
}
