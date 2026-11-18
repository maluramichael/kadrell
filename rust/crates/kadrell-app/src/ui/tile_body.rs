//! Körper einer Kachel ohne Terminal, Port von `CellView.drawBody`: fehlender Ordner, Startfehler, beendet (Schraffur
//! und letzte Zeilen), Vorschau, „STARTET …“ mit Spinner, Terminal in anderem Fenster und Schnappschuss-Zeilen.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Bounds, ContentMask, Div, FontWeight, Hsla, IntoElement, ParentElement, PathBuilder, Pixels, Styled, canvas, div, hsla, point,
    px, rgb,
};
use kadrell_i18n::trd;

use super::tile_icons;
use super::tile_look::{self, BodyKind, LINE_FONT, LINE_HEIGHT, LineTone};
use crate::appearance::Appearance;
use crate::theme::Theme;

/// Abgedunkelter Körper bei fehlendem Ordner, Startfehler und beendeter Session.
const SHADE: u32 = 0x11111b;

fn shade(alpha: f32) -> Hsla {
    let c: Hsla = rgb(SHADE).into();
    Hsla { a: alpha, ..c }
}

/// Was der Körper zeigt.
pub struct Body<'a> {
    pub kind: BodyKind,
    pub lines: &'a [String],
    /// Ordner der Session mit `~`, für „ORDNER FEHLT“.
    pub short_cwd: &'a str,
    /// Höhe der Fläche in Pixeln, wenn bekannt: bestimmt, wie viele Zeilen passen und ob eine Beschriftung Platz hat.
    pub height: Option<f32>,
}

fn fill(color: Hsla) -> Div {
    div().absolute().size_full().bg(color)
}

/// Beschriftung unten, mittig, gedämpft oder in Farbe (`drawLabel`): 11 pt, 6 Rand links und rechts, 10 vom unteren Rand.
fn label(text: String, color: Hsla, a: &Appearance) -> Div {
    div()
        .absolute()
        .left(px(a.s(6.)))
        .right(px(a.s(6.)))
        .bottom(px(a.s(10.)))
        .flex()
        .justify_center()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(a.s(11.)))
        .text_color(color)
        .child(text)
}

fn line_color(tone: LineTone, t: &Theme) -> Hsla {
    match tone {
        LineTone::Prompt => t.fg,
        LineTone::Marker => t.sub,
        LineTone::Plain => t.muted,
    }
}

/// Letzte Zeilen des Bildschirms: Schrift 12, Zeilenhöhe 17,4, ab 10 / 8 Innenabstand, die letzten, die hineinpassen.
fn lines_view(lines: &[String], height: Option<f32>, t: &Theme, a: &Appearance) -> Div {
    let room = height.map_or(f32::INFINITY, |h| h / a.scale - 16.);
    let shown = if room.is_finite() { tile_look::visible_lines(lines, room) } else { &lines[lines.len().saturating_sub(40)..] };
    let mut col = div().absolute().left(px(a.s(10.))).right(px(a.s(10.))).top(px(a.s(8.))).bottom(px(a.s(8.))).overflow_hidden().flex().flex_col();
    for line in shown {
        let tone = tile_look::line_tone(line);
        let mut row = div().h(px(a.s(LINE_HEIGHT))).flex_none().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(a.s(LINE_FONT))).text_color(line_color(tone, t));
        if tone == LineTone::Prompt {
            row = row.font_weight(FontWeight::BOLD);
        }
        col = col.child(row.child(line.clone()));
    }
    col
}

/// Schraffur: schwarze Diagonalen, Breite 6, über dem abgedunkelten Körper, am Körper abgeschnitten.
fn hatch(a: &Appearance) -> impl IntoElement + use<> {
    let k = a.scale;
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (w, h) = (bounds.size.width.as_f32() / k, bounds.size.height.as_f32() / k);
            let (ox, oy) = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                for ((x0, y0), (x1, y1)) in tile_look::hatch_lines(w, h) {
                    let mut b = PathBuilder::stroke(px(6. * k));
                    b.move_to(point(px(ox + x0 * k), px(oy + y0 * k)));
                    b.line_to(point(px(ox + x1 * k), px(oy + y1 * k)));
                    if let Ok(path) = b.build() {
                        window.paint_path(path, hsla(0., 0., 0., 0.35));
                    }
                }
            });
        },
    )
    .absolute()
    .size_full()
}

/// Spinner mittig, 24 x 24, Strich 2, eine Umdrehung pro Sekunde.
fn spinner(t: &Theme, a: &Appearance) -> AnyElement {
    let (size, color) = (a.s(24.), t.sub);
    let width = a.s(2.);
    div()
        .absolute()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(div().size(px(size)).with_animation("tile-spinner", Animation::new(Duration::from_secs(1)).repeat(), move |el, delta| el.child(tile_icons::spinner(size, width, color, delta))))
        .into_any_element()
}

/// Der Körper als Fläche in der Farbe des Schemas (mit Deckkraft) und, je nach Zustand, den Überlagerungen.
pub fn view(b: &Body, t: &Theme, a: &Appearance) -> Div {
    let base = div().relative().size_full().overflow_hidden().bg(a.panel_fill(t.bg));
    let roomy = b.height.is_none_or(|h| h > a.s(24.));
    let text = |s: String, c: Hsla| roomy.then(|| label(s, c, a));
    match b.kind {
        BodyKind::MissingFolder => base.child(fill(shade(0.72))).children(text(trd!("ORDNER FEHLT · %@", b.short_cwd), t.error)),
        BodyKind::StartFailed(code) => {
            base.child(fill(shade(0.35))).child(lines_view(b.lines, b.height, t, a)).children(text(trd!("START FEHLGESCHLAGEN · EXIT %@", code.to_string()), t.error))
        }
        BodyKind::Ended => {
            base.child(fill(shade(0.72))).child(hatch(a)).child(lines_view(b.lines, b.height, t, a)).children(text(trd!("BEENDET · KLICK SETZT FORT"), t.muted))
        }
        BodyKind::Preview => base.children(text(trd!("VORSCHAU · NICHT GESTARTET · ⏎ ODER KLICK STARTET"), t.muted)),
        BodyKind::Starting => base.child(spinner(t, a)).children(text(trd!("STARTET …"), t.muted)),
        BodyKind::Elsewhere => base.children(text(trd!("IN ANDEREM FENSTER · KLICK HOLT HIERHER"), t.muted)),
        BodyKind::Lines => base.child(lines_view(b.lines, b.height, t, a)),
        BodyKind::Terminal => base,
    }
}
