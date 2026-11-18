//! Kleine, zustandslose Bausteine des Einstellungsdialogs im Instrument-Look (`Instrument+SwiftUI.swift`,
//! `Theme+Instrument.swift`): Abschnittstitel, Tabelle mit Fugen, Zeile, eingelassenes Feld, Dropdown-Fläche,
//! Schieberegler, Taste, Kürzel-Feld und Popover-Liste. Ereignisse hängt der Dialog selbst an.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, BoxShadow, Div, FontWeight, Hsla, IntoElement, ParentElement, Pixels, Styled, canvas, div, hsla,
    point, prelude::*, px, relative,
};

use super::model::Icon;
use crate::theme::Theme;
use crate::ui::chrome::{h_seam, seam_dark, seam_light};
use crate::ui::instrument::{self, KEY_RADIUS, KeyRole, OUTLINE_WIDTH, RING_WIDTH, section_label};
use crate::ui::overlay::{SHADOW_DEPTH, border_px, hairline, raised, scale, shadow_color};

/// Gemessene Rechtecke (Fensterkoordinaten) von Feldern, Reglern und Zeilen aus dem letzten Frame.
pub type Geometry = Rc<RefCell<HashMap<String, Bounds<Pixels>>>>;

/// Merkt sich beim Zeichnen das Rechteck des Elternelements unter `key`.
pub fn measure(geometry: &Geometry, key: String) -> impl IntoElement + use<> {
    let sink = geometry.clone();
    canvas(move |bounds, _, _| sink.borrow_mut().insert(key.clone(), bounds), |_, _, _, _| ()).absolute().size_full()
}

/// Fläche mit Fugenring: außen eine dunkle, innen eine helle Haarlinie (`Instrument.seamRing`).
pub fn seam_ring(t: &Theme, radius: f32, fill: Hsla, inner: Div) -> Div {
    let h = hairline();
    let inner = border_px(inner.flex_1().min_w_0().rounded(px(radius - h)), h).border_color(seam_light(t));
    border_px(div().flex().flex_col().rounded(px(radius)), h).border_color(seam_dark(t)).bg(fill).child(inner)
}

// MARK: Abschnitt und Tabelle

/// Bereichsüberschrift: gesperrte Versalien in Akzent-Textfarbe, optional ein Hinweis daneben.
pub fn section_heading(title: &str, note: &str, t: &Theme) -> Div {
    // Gesperrt wie `sectionTitle()`: bold, Akzent-Textfarbe, Sperrung 0,8.
    let label = section_label(&title.to_uppercase(), t);
    let note = (!note.is_empty()).then(|| div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(11.)).text_color(t.muted).child(note.to_string()));
    div().flex().items_baseline().gap(px(8.)).px(px(16.)).pt(px(14.)).pb(px(6.)).child(label).children(note)
}

/// Zwischentitel („Navigation“): Versalien in Fließtext-Gewicht, gedämpft.
pub fn subtitle(text: &str, first: bool, t: &Theme) -> Div {
    let label = div().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(t.muted).child(text.to_string());
    div().px(px(16.)).pt(px(if first { 0. } else { 14. })).pb(px(4.)).child(label)
}

/// Tabelle: eingesetzte Fläche in `bg` mit Fugenring, zwischen den Zeilen eine Fuge.
pub fn table(rows: Vec<AnyElement>, t: &Theme) -> Div {
    let mut inner = div().flex().flex_col();
    for (i, row) in rows.into_iter().enumerate() {
        if i > 0 {
            inner = inner.child(h_seam(t));
        }
        inner = inner.child(row);
    }
    div().px(px(16.)).child(seam_ring(t, 9., t.bg, inner))
}

/// Zeile: Beschriftung links (höchstens zwei Zeilen), Steuerelement rechts in fester Breite.
pub fn row(title: &str, control: AnyElement, control_width: f32, t: &Theme) -> Div {
    let label = div().flex_1().min_w_0().text_color(t.fg).child(title.to_string());
    let control = div().flex_none().w(px(control_width)).child(control);
    div().flex().items_center().gap(px(12.)).px(px(10.)).py(px(5.)).text_size(px(12.)).child(label).child(control)
}

/// Zeile über die ganze Breite (Hinweis, Modell).
pub fn wide_row(content: Div) -> Div {
    content.flex().items_center().gap(px(10.)).px(px(10.)).py(px(5.)).text_size(px(12.))
}

// MARK: Felder

/// Eingelassenes Feld, 22 px hoch. Mit Fokus trägt es 2 px Akzentring statt der Fuge.
pub fn well(t: &Theme, focused: bool, content: Div) -> Div {
    // Abstand 8 seitlich wie `instrumentWell`; der Rand (Fuge oder Akzentring) liegt innerhalb davon.
    let s = scale();
    let edge = if focused { RING_WIDTH * s } else { hairline() * 2. };
    let content = content.flex().items_center().h_full().px(px(8. * s - edge)).text_size(px(12. * s));
    if focused {
        let inner = content.rounded(px((KEY_RADIUS - RING_WIDTH).max(0.) * s));
        return border_px(div().h(px(22. * s)).flex().flex_col().rounded(px(KEY_RADIUS * s)), edge).border_color(t.accent).bg(t.well()).child(inner.flex_1());
    }
    seam_ring(t, KEY_RADIUS * s, t.well(), content).h(px(22. * s))
}

/// Dropdown-Fläche: Text links, Auf-Ab-Pfeile rechts.
pub fn select_face(text: &str, focused: bool, t: &Theme) -> Div {
    let label = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(t.fg).child(text.to_string());
    well(t, focused, div().justify_between().gap(px(4.)).child(label).child(chevrons(t.muted)))
}

/// Text in einer Info-Fläche des Wertes (Kürzel, Pfad im Feld).
pub fn value_face(text: &str, color: Hsla, bold: bool, focused: bool, t: &Theme) -> Div {
    let label = div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(color).when(bold, |d| d.font_weight(FontWeight::BOLD)).child(text.to_string());
    well(t, focused, div().child(label))
}

// MARK: Tasten

/// Taste im Instrument-Look: Kante als Dicke (unten 2 px), darauf die Kappe. Klein: 11 pt.
pub fn key_cap(content: impl IntoElement, destructive: bool, t: &Theme) -> Div {
    let role = if destructive { KeyRole::Destructive } else { KeyRole::Neutral };
    instrument::key(content, role, true, true, t)
}

/// Inhalt einer kleinen Taste: Text oder Symbol.
pub fn key_content(label: &str, icon: Option<Icon>, color: Hsla) -> AnyElement {
    match icon {
        Some(icon) => icon_view(icon, color).into_any_element(),
        None => div().child(label.to_string()).into_any_element(),
    }
}

/// Akzentring um ein Steuerelement, das die Tastatur hat (liegt außen, verschiebt nichts).
pub fn keyboard_ring(t: &Theme) -> Div {
    // 2 pt Akzent, Linienmitte mit Radius 5 + 2, 2 pt außerhalb der Fläche (`kbdFocusRing`): außen Radius 8, Rand -3.
    instrument::focus_ring(t)
}

// MARK: Schieberegler

/// Schieberegler: Spur mit Füllung in Akzent und Knopf, rechts der Wert. `key` misst die Spur für die Maus.
pub fn slider(frac: f32, value_text: &str, geometry: (&Geometry, String), ticks: usize, t: &Theme) -> (Div, Div) {
    let (geometry, key) = geometry;
    let frac = frac.clamp(0., 1.);
    let knob_shadow = BoxShadow { color: hsla(0., 0., 0., 0.35), offset: point(px(0.), px(1.)), blur_radius: px(2.), spread_radius: px(0.), inset: false };
    let rail = div().absolute().left(px(0.)).right(px(0.)).top(px(6.)).h(px(4.)).rounded(px(2.)).bg(t.line);
    let fill = div().absolute().left(px(0.)).top(px(6.)).h(px(4.)).w(relative(frac)).rounded(px(2.)).bg(t.accent);
    let knob = div().absolute().top(px(2.)).left(relative(frac)).ml(px(-6.)).size(px(12.)).rounded_full().bg(hsla(0., 0., 1., 1.)).border_1().border_color(hsla(0., 0., 0., 0.18)).shadow(vec![knob_shadow]);
    // Das native Mac-Control zeichnet bei Schrittweite je Schritt einen Punkt auf die Spur (bis 100 Schritte).
    let dots = tick_fractions(ticks).into_iter().map(|f| div().absolute().left(relative(f)).ml(px(-0.5)).top(px(7.5)).size(px(1.)).rounded_full().bg(t.muted.opacity(0.55)));
    let track = div().relative().flex_1().h(px(16.)).cursor_pointer().child(measure(geometry, key)).child(rail).children(dots).child(fill).child(knob);
    let value = div().flex_none().w(px(60.)).text_align(gpui::TextAlign::Right).text_color(t.fg).child(value_text.to_string());
    (track, value)
}

/// Stellen (0 bis 1) der Schrittpunkte auf der Spur; mehr als 100 Schritte zeichnet auch das Mac-Control nicht.
pub fn tick_fractions(steps: usize) -> Vec<f32> {
    if steps == 0 || steps > 100 {
        return Vec::new();
    }
    (0..=steps).map(|i| i as f32 / steps as f32).collect()
}

// MARK: Popover

/// Liste des Dropdowns: erhabene Fläche mit Fuge und festem Schatten, höchstens 280 px hoch.
pub fn popover(width: f32, t: &Theme) -> Div {
    let shadow = BoxShadow { color: shadow_color(t), offset: point(px(0.), px(SHADOW_DEPTH * scale())), blur_radius: px(0.), spread_radius: px(0.), inset: false };
    border_px(div().flex().flex_col().w(px(width)).max_h(px(280.)).py(px(4.)).rounded(px(7.)).bg(raised(t)), hairline()).border_color(seam_dark(t)).shadow(vec![shadow]).text_size(px(12.)).text_color(t.fg)
}

/// Zeile der Popover-Liste: gewählt mit Fläche `bg` und 1,5-px-Umriss in `fg`, die Zeile unter dem Zeiger heller.
pub fn popover_item(label: &str, current: bool, hovered: bool, t: &Theme) -> Div {
    let outline = if current { t.fg } else { gpui::transparent_black() };
    // Umriss 1,5 pt in `fg` (`instrumentRow(selected:)`), ohne Akzentring.
    let face = border_px(div().flex().items_center().h(px(24.)).px(px(8.)).mx(px(4.)).rounded(px(6.)), OUTLINE_WIDTH).border_color(outline).when(current || hovered, |d| d.bg(t.bg)).child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label.to_string()));
    div().flex_none().child(face)
}

// MARK: Symbole

fn chevrons(color: Hsla) -> impl IntoElement {
    paint_icon(9., 12., vec![(vec![(1.5, 4.5), (4.5, 1.8), (7.5, 4.5)], false), (vec![(1.5, 7.5), (4.5, 10.2), (7.5, 7.5)], false)], color, 1.2)
}

fn icon_view(icon: Icon, color: Hsla) -> impl IntoElement {
    let strokes: Vec<(Vec<(f32, f32)>, bool)> = match icon {
        Icon::Folder => vec![(vec![(1., 3.), (5., 3.), (6.5, 4.8), (13., 4.8), (13., 12.), (1., 12.)], true)],
        Icon::Image => vec![
            (vec![(1., 2.), (13., 2.), (13., 12.), (1., 12.)], true),
            (vec![(1.5, 10.5), (5., 6.5), (8., 9.5), (10., 7.5), (12.5, 10.5)], false),
        ],
        Icon::Cross => vec![(vec![(3., 3.), (11., 11.)], false), (vec![(11., 3.), (3., 11.)], false)],
    };
    paint_icon(14., 14., strokes, color, 1.2)
}

/// Gezeichnetes Strichsymbol in Symbolkoordinaten (Ursprung oben links).
fn paint_icon(w: f32, h: f32, strokes: Vec<(Vec<(f32, f32)>, bool)>, color: Hsla, width: f32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let mut builder = gpui::PathBuilder::stroke(px(width));
            for (points, closed) in &strokes {
                let pts: Vec<_> = points.iter().map(|p| point(bounds.origin.x + px(p.0), bounds.origin.y + px(p.1))).collect();
                builder.add_polygon(&pts, *closed);
            }
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        },
    )
    .w(px(w))
    .h(px(h))
    .flex_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_cover_the_track_in_steps() {
        assert_eq!(tick_fractions(4), vec![0., 0.25, 0.5, 0.75, 1.]);
        assert_eq!(tick_fractions(0), Vec::<f32>::new());
        assert_eq!(tick_fractions(101), Vec::<f32>::new());
        assert_eq!(tick_fractions(48).len(), 49);
    }
}
