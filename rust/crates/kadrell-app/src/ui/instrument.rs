//! Dialog-Bausteine im Instrument-Look (`Instrument+SwiftUI.swift`, `Theme+Instrument.swift`, `Theme+SwiftUI.swift`):
//! Tasten mit Rolle, gesperrtes Versal-Label, Tastaturring, eingelassenes Feld, Listenauswahl. Alle Maße in logischen
//! Punkten mal UI-Größe (`overlay::scale`).

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use super::chrome::{key_edge, seam_dark, seam_light};
use super::overlay::{self, border_px, hairline, scale};
use crate::theme::Theme;

/// Radien in logischen Punkten (`Instrument`): Taste und Feld, Zeilenauswahl, Gruppe.
pub const KEY_RADIUS: f32 = 5.;
pub const KEY_DEPTH: f32 = 2.;
pub const ROW_RADIUS: f32 = 6.;
pub const GROUP_RADIUS: f32 = 9.;
/// Sperrung der Versalien-Titel (`Instrument.labelKern`).
pub const LABEL_KERN: f32 = 0.8;
/// Umriss der Listenauswahl in logischen Punkten.
pub const OUTLINE_WIDTH: f32 = 1.5;
/// Breite des Tastaturrings.
pub const RING_WIDTH: f32 = 2.;

/// Rolle einer Taste: Primär in `accent`, destruktiv in `error`, alles andere neutral auf der Kappe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRole {
    Primary,
    Neutral,
    Destructive,
}

/// Füllung der Kappe (`KeyRole.fill`, neutral `keyCap` = `surface`).
pub fn key_fill(role: KeyRole, t: &Theme) -> Hsla {
    match role {
        KeyRole::Primary => t.accent,
        KeyRole::Neutral => t.surface,
        KeyRole::Destructive => t.error,
    }
}

/// Lesbare Beschriftung der Kappe (`KeyRole.label`): mit Füllung `pillText(on:)`, neutral `fg` mit mindestens 4,5:1.
pub fn key_ink(role: KeyRole, t: &Theme) -> Hsla {
    match role {
        KeyRole::Neutral => t.key_label(),
        _ => t.pill_text_on(key_fill(role, t)),
    }
}

/// Schrift und Innenabstand einer Taste, unskaliert (`InstrumentKeyStyle`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyMetrics {
    pub font: f32,
    pub bold: bool,
    pub pad_x: f32,
    pub pad_y: f32,
}

/// Normal: Schrift 12 (fett außer neutral), Abstand 12/5. Kompakt: 11 regulär, 6/2.
pub fn key_metrics(role: KeyRole, compact: bool) -> KeyMetrics {
    if compact {
        KeyMetrics { font: 11., bold: false, pad_x: 6., pad_y: 2. }
    } else {
        KeyMetrics { font: 12., bold: role != KeyRole::Neutral, pad_x: 12., pad_y: 5. }
    }
}

/// Die Kappe samt Kante: Kante 1 px rundum und 3 px unten (1 plus Tiefe 2), die Kappe darin mit Radius 4. Der
/// Innenabstand der Kappe ist um die Kante kleiner, so bleibt das Gesamtmaß wie in der Mac-App.
fn cap(content: impl IntoElement, role: KeyRole, compact: bool, enabled: bool, t: &Theme) -> Div {
    let m = key_metrics(role, compact);
    let s = scale();
    let ink = if enabled { key_ink(role, t) } else { t.muted };
    let face = div()
        .flex()
        .items_center()
        .justify_center()
        .px(px((m.pad_x - 1.) * s))
        .py(px((m.pad_y - 1.) * s))
        .rounded(px((KEY_RADIUS - 1.) * s))
        .bg(key_fill(role, t))
        .text_color(ink)
        .text_size(px(m.font * s))
        .whitespace_nowrap()
        .child(content);
    let face = if m.bold { face.font_weight(FontWeight::BOLD) } else { face };
    div().flex_none().pt(px(s)).px(px(s)).pb(px((1. + KEY_DEPTH) * s)).rounded(px(KEY_RADIUS * s)).bg(key_edge(t)).child(face)
}

/// Taste ohne Klickzustand (Aufrufer hängen `id` und Klick selbst an). Kein gedrückter Zustand: dafür `key_pressable`.
pub fn key(content: impl IntoElement, role: KeyRole, compact: bool, enabled: bool, t: &Theme) -> Div {
    let el = cap(content, role, compact, enabled, t);
    if enabled { el.cursor_pointer() } else { el }
}

/// Taste mit Druckzustand: die Kappe sinkt 1 pt ein (Dicke nur noch 1 pt) und die Beschriftung mit ihr. Gesperrt: Schrift
/// gedämpft, kein Zeiger, kein Druck.
pub fn key_pressable(id: impl Into<ElementId>, content: impl IntoElement, role: KeyRole, compact: bool, enabled: bool, t: &Theme) -> Stateful<Div> {
    let s = scale();
    let el = cap(content, role, compact, enabled, t).id(id);
    if !enabled {
        return el;
    }
    el.cursor_pointer().active(move |st| st.pt(px(2. * s)).pb(px((1. + KEY_DEPTH - 1.) * s)))
}

/// Klickbare Taste in einem Zug: `key_pressable` mit Klick.
pub fn key_button_with(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    role: KeyRole,
    compact: bool,
    t: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    key_pressable(id, content, role, compact, true, t).on_click(on_click)
}

/// Taste mit Tastaturfokus: ein Tab-Stopp (`dialog_keys::control`), Enter und Leertaste lösen sie aus, mit Tastaturfokus
/// zeichnet sie den Ring (`kbdFocusRing`). `id` muss im Dialog eindeutig sein.
#[allow(clippy::too_many_arguments)]
pub fn key_focusable(
    id: &'static str,
    content: impl IntoElement,
    role: KeyRole,
    compact: bool,
    t: &Theme,
    window: &mut Window,
    cx: &mut App,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let key = key_pressable(id, content, role, compact, true, t).relative();
    let (key, ring) = super::dialog_keys::control(key, id, window, cx, Rc::new(on_click));
    if ring { key.child(focus_ring(t)) } else { key }
}

/// Gesperrtes Versal-Label (`capsLabel`): 11, Sperrung 0,8, in `color` (gedämpft, mit Akzent `accent_text`). GPUI kennt
/// keinen Buchstabenabstand, deshalb steht jedes Zeichen in einer eigenen Zelle mit rechtem Abstand. Der Text kommt in
/// Versalien an; Leerzeichen werden geschützt, damit sie nicht zusammenfallen.
pub fn caps_label(text: &str, bold: bool, color: Hsla) -> Div {
    let s = scale();
    kerned(text, 11. * s, LABEL_KERN * s, bold, color)
}

/// Text mit Buchstabenabstand (`NSAttributedString.kern`, `Text.kerning`): Schriftgröße `size` und Sperrung `kern`
/// in fertigen Punkten. Der einzige Helfer dafür; GPUI hat keinen Buchstabenabstand, jedes Zeichen steht deshalb in
/// einer eigenen Zelle mit rechtem Abstand (wie bei AppKit hängt die Sperrung auch hinter dem letzten Zeichen).
/// Leerzeichen werden geschützt, damit sie nicht zusammenfallen. Ein Umbruch oder eine Auslassung gibt es nicht,
/// wer kürzt, tut das vorher (`kerned_width`).
pub fn kerned(text: &str, size: f32, kern: f32, bold: bool, color: Hsla) -> Div {
    let widths = kerned_cells(text.chars().count(), size * super::tree_rules::MONO_ADVANCE + kern, overlay::hairline());
    let cells = text.chars().zip(widths).map(|(c, w)| {
        let shown = if c == ' ' { '\u{a0}' } else { c };
        div().flex_none().w(px(w)).child(shown.to_string())
    });
    let label = div().flex().flex_none().whitespace_nowrap().text_size(px(size)).text_color(color).children(cells);
    if bold { label.font_weight(FontWeight::BOLD) } else { label }
}

/// Zellbreiten für `count` Zeichen mit Schritt `step` (Zeichenbreite plus Sperrung) in Punkten, auf Gerätepixel `px_pt`
/// (ein Pixel in Punkten) gerastet. GPUI rundet jede Zelle einzeln auf ganze Gerätepixel (Textbreite hoch, Abstand
/// kaufmännisch), das machte jede Zelle am Mac 0,6 pt zu breit. Hier summieren sich die Zellen auf `count * step`: die
/// Kante jeder Zelle liegt auf dem Gerätepixel, das ihrer wahren Position am nächsten ist.
pub fn kerned_cells(count: usize, step: f32, px_pt: f32) -> Vec<f32> {
    let px_pt = px_pt.max(0.01);
    let mut prev = 0.;
    (1..=count)
        .map(|i| {
            let edge = ((i as f32 * step) / px_pt).round() * px_pt;
            let w = edge - prev;
            prev = edge;
            w
        })
        .collect()
}

/// Breite von `text` mit Sperrung: die Breite ohne Sperrung plus `kern` je Zeichen.
pub fn kerned_width(plain_width: f32, text: &str, kern: f32) -> f32 {
    plain_width + text.chars().count() as f32 * kern
}

/// Abschnittstitel (`sectionTitle`): fettes Versal-Label in Akzent-Textfarbe.
pub fn section_label(text: &str, t: &Theme) -> Div {
    caps_label(text, true, t.accent_text)
}

/// Tastaturring (`kbdFocusRing`): 2 pt Akzent, Radius (5 + 2) auf der Linienmitte, 2 pt außerhalb der Fläche. Liegt
/// absolut im Elternelement und verschiebt nichts; das Elternelement muss `relative` sein.
pub fn focus_ring(t: &Theme) -> Div {
    let s = scale();
    let out = -(RING_WIDTH + 1.) * s;
    border_px(div().absolute().top(px(out)).bottom(px(out)).left(px(out)).right(px(out)).rounded(px((KEY_RADIUS + RING_WIDTH + 1.) * s)), RING_WIDTH * s)
        .border_color(t.accent)
}

/// Eingelassenes Feld (`instrumentWell`): Abstand 8/3, Fläche `well`, Radius 5. Unfokussiert mit Fugenring, fokussiert
/// innen am Rand ein Akzentring von 2 pt statt der Fuge.
pub fn well_frame(content: impl IntoElement, focused: bool, t: &Theme) -> Div {
    let s = scale();
    let r = KEY_RADIUS * s;
    let edge = if focused { overlay::ring_inside(t.accent, r, RING_WIDTH * s) } else { overlay::seam_ring_overlay(t, r) };
    div().relative().flex().items_center().rounded(px(r)).bg(overlay::well(t)).px(px(8. * s)).py(px(3. * s)).child(edge).child(content)
}

/// Zeilenauswahl in Dialoglisten (`instrumentRow(selected:)`): Fläche `bg` mit 1,5 pt Umriss in `fg`, ohne Akzentring.
/// Der Umriss liegt als Overlay innen an der Fläche und verschiebt nichts.
pub fn selected_row(row: Div, selected: bool, t: &Theme) -> Div {
    let s = scale();
    let row = row.relative().rounded(px(ROW_RADIUS * s));
    if selected { row.bg(t.bg).child(overlay::ring_inside(t.fg, ROW_RADIUS * s, OUTLINE_WIDTH * s)) } else { row }
}

/// Radius der einen runden Ecke, auf die kürzere Kante der Fläche begrenzt (`topLeftRounded`).
pub fn clamp_corner(radius: f32, width: f32, height: f32) -> f32 {
    radius.max(0.).min(width.max(0.)).min(height.max(0.))
}

/// Rahmenbreiten je Kante setzen (oben, rechts, unten, links).
fn edges<E: Styled>(mut el: E, top: f32, right: f32, bottom: f32, left: f32) -> E {
    let px_of = |v: f32| gpui::AbsoluteLength::Pixels(px(v));
    let b = &mut el.style().border_widths;
    b.top = Some(px_of(top));
    b.right = Some(px_of(right));
    b.bottom = Some(px_of(bottom));
    b.left = Some(px_of(left));
    el
}

// Die Kacheln legen ihren Rand als eigene Ebenen (`tile.rs`: `border_layer`, `inner_rim`, Ecke mit `tile_edge::clamp_corner`).
/// Rand einer eingesetzten Fläche ohne Unterkante (`Instrument.rim`): links, rechts und mit `top` oben außen eine
/// Haarlinie `seamDark`, direkt innen eine Haarlinie `seamLight`. Mit `radius` folgt die Ecke oben links dem Bogen der
/// Modul-Ecke (`topLeftRounded`, nur diese Ecke rund). Das Element nimmt den Inhalt in seine Mitte auf.
pub fn rim(content: impl IntoElement, top: bool, radius: f32, t: &Theme) -> Div {
    let h = hairline();
    let t_edge = if top { h } else { 0. };
    let outer = edges(div().relative().flex().flex_col().rounded_tl(px(radius.max(0.))), t_edge, h, 0., h).border_color(seam_dark(t));
    let inner = edges(div().absolute().top_0().left_0().right_0().bottom_0().rounded_tl(px((radius - h).max(0.))), t_edge, h, 0., h)
        .border_color(seam_light(t));
    outer.child(inner).child(content)
}

/// Gruppe (`instrumentGroup`): Fläche `bg`, Radius 9, geschlossener Fugenring.
pub fn group_frame(content: impl IntoElement, t: &Theme) -> Div {
    let s = scale();
    let r = GROUP_RADIUS * s;
    div().relative().rounded(px(r)).bg(t.bg).child(overlay::seam_ring_overlay(t, r)).child(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kerned_cells_add_up_to_the_exact_width() {
        let cells = kerned_cells(17, 11. * 0.6 + 0.8, 0.5);
        assert!((cells.iter().sum::<f32>() - 17. * 7.4).abs() <= 0.25);
        assert!(cells.iter().all(|w| *w == 7. || *w == 7.5));
    }

    #[test]
    fn key_style_follows_the_role() {
        let n = key_metrics(KeyRole::Neutral, false);
        assert_eq!((n.font, n.bold, n.pad_x, n.pad_y), (12., false, 12., 5.));
        let p = key_metrics(KeyRole::Primary, false);
        assert!(p.bold);
        assert!(key_metrics(KeyRole::Destructive, false).bold);
        let c = key_metrics(KeyRole::Primary, true);
        assert_eq!((c.font, c.bold, c.pad_x, c.pad_y), (11., false, 6., 2.));
    }

    #[test]
    fn key_fills_and_ink_use_theme_roles() {
        let t = Theme::mocha();
        assert_eq!(key_fill(KeyRole::Primary, &t), t.accent);
        assert_eq!(key_fill(KeyRole::Neutral, &t), t.surface);
        assert_eq!(key_fill(KeyRole::Destructive, &t), t.error);
        assert_eq!(key_ink(KeyRole::Neutral, &t), t.key_label());
        assert_eq!(key_ink(KeyRole::Destructive, &t), t.pill_text_on(t.error));
    }

    #[test]
    fn corner_radius_is_clamped_to_the_short_side() {
        assert_eq!(clamp_corner(14., 100., 50.), 14.);
        assert_eq!(clamp_corner(14., 10., 50.), 10.);
        assert_eq!(clamp_corner(14., 100., 6.), 6.);
        assert_eq!(clamp_corner(-3., 100., 50.), 0.);
    }

    #[test]
    fn rim_builds_with_and_without_top() {
        let t = Theme::mocha();
        let _ = rim(div(), true, 14., &t);
        let _ = rim(div(), false, 0., &t);
    }

    #[test]
    fn caps_label_keeps_every_character() {
        let _ = caps_label("NEUE SESSION", false, Theme::mocha().muted);
    }
}
