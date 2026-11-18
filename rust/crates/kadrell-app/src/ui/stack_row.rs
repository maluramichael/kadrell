//! Stack-Zeile (i3-Akkordeon), Port von `WorkspaceView.drawStackRow`: eine Zeile je Kachel, 26 pt hoch, die aktive Kachel
//! hat ihren Körper direkt darunter. Fläche `surface` bei Fokus oder Sync, sonst `panel`; links Statuspunkt, Titel (fett bei
//! Fokus), rechts von außen nach innen Symbole beim Überfahren, Alter, Gruppe, Pfad. Die Spalte ist eine eingesetzte Fläche:
//! jede Zeile trägt links und rechts den Rand, die erste auch oben (dem Bogen der Modul-Ecke folgend).

use gpui::{AnyElement, App, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, div, px};

use super::chrome::{h_seam, seam_dark, seam_light};
use super::groups::drag::DragTile;
use super::groups::rows::DragGhost;
use super::model::{Group, Session};
use super::theme;
use super::tile::{IconClick, dot, group_label, text_width};
use super::tile_icons;
use super::tile_look::{self, Icon};
use crate::appearance::{Appearance, a11y};
use crate::theme::Theme;
use gpui::AppContext;

/// Breiten der Textteile einer Zeile, in logischen Punkten.
#[derive(Clone, Copy, Debug, Default)]
pub struct RowIn {
    pub width: f32,
    pub age: f32,
    /// Breite des Gruppen-Labels (schon auf `width / 3` gekürzt), 0 ohne Gruppenname.
    pub label: f32,
    pub hovered: bool,
    pub show_path: bool,
    /// Natürliche Breite des (schon gekürzten) Pfads.
    pub path: f32,
}

/// Wo was in der Zeile steht (x ab linker Kante).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowOut {
    pub close_x: Option<f32>,
    pub pen_x: Option<f32>,
    pub age_x: f32,
    /// (x des Akzents (Mitte), x des Labels)
    pub group: Option<(f32, f32)>,
    /// (x, Breite) des Pfads.
    pub path: Option<(f32, f32)>,
    pub title_w: f32,
}

/// Rechte Seite von rechts nach links wie `drawStackRowRight`: 10 Rand, beim Überfahren zwei Symbole (44 Reserve), das Alter,
/// die Gruppe mit Akzent, der Pfad. Der Titel bekommt den Rest.
pub fn row_layout(i: RowIn) -> RowOut {
    let mut rx = i.width - 10.;
    let (mut close_x, mut pen_x) = (None, None);
    if i.hovered {
        close_x = Some(rx - 16.);
        pen_x = Some(rx - 36.);
        rx -= 44.;
    }
    rx -= i.age;
    let age_x = rx;
    let mut group = None;
    if i.label > 0. {
        rx -= 8. + i.label;
        group = Some((rx - 8., rx));
        rx -= 11.;
    }
    let mut path = None;
    if i.show_path {
        let max = ((rx - 16. - 28.) / 2.).max(0.);
        let w = i.path.min(max);
        rx -= 16. + w;
        path = Some((rx, w));
    }
    RowOut { close_x, pen_x, age_x, group, path, title_w: (rx - 10. - 28.).max(0.) }
}

/// Gruppen-Label der Zeile, mit Auslassung auf höchstens `max_width` gekürzt: gesperrte Versalien sind breit und
/// würden sonst in den Titel laufen (`stackGroupLabel`).
pub fn fit_label(name: &str, max_width: f32, width_of: &dyn Fn(&str) -> f32) -> String {
    let mut text = name.to_string();
    let mut label = group_label(&text);
    while width_of(&label) > max_width && !text.is_empty() {
        text.pop();
        label = group_label(&format!("{text}…"));
    }
    label
}

/// Eine Stack-Zeile samt Lage in der Spalte.
pub struct RowCell<'a> {
    pub group: &'a Group,
    pub session: &'a Session,
    pub width: f32,
    pub height: f32,
    /// Fokus oder Sync: Fläche `surface`, Titel fett.
    pub on: bool,
    pub hovered: bool,
    pub first: bool,
    /// Direkt über der Zeile liegt ein Körper: dessen Unterkante ist dort die Fuge, die Zeile bringt die helle Hälfte mit.
    pub body_above: bool,
    /// Direkt unter der Zeile liegt der Körper: keine eigene Fuge unten.
    pub body_below: bool,
    pub corner: f32,
    pub show_path: bool,
    pub on_icon: Option<IconClick>,
}

fn hairline(top: f32, color: gpui::Hsla) -> Div {
    div().absolute().left_0().right_0().top(px(top)).h(px(1.)).bg(color)
}

/// Rand der eingesetzten Fläche ohne Unterkante: außen dunkel, direkt innen hell; mit `first` auch oben, mit Bogen.
fn rim(c: &RowCell, t: &Theme) -> Vec<Div> {
    let side = |inset: f32, color: gpui::Hsla, radius: f32| {
        let d = div().absolute().left(px(inset)).right(px(inset)).top(px(if c.first { inset } else { 0. })).bottom_0().border_l_1().border_r_1().border_color(color);
        if c.first { d.border_t_1().rounded_tl(px(radius.max(0.))) } else { d }
    };
    let r = c.corner.max(0.);
    vec![side(0., seam_dark(t), r), side(1., seam_light(t), r - 1.)]
}

fn icon_button(icon: Icon, x: f32, c: &RowCell, t: &Theme, a: &Appearance) -> AnyElement {
    let (label, glyph) = match icon {
        Icon::Close => (kadrell_i18n::trd!("Schließen"), tile_icons::x(t.sub, a.scale).into_any_element()),
        _ => (kadrell_i18n::trd!("Umbenennen"), tile_icons::pen(t.sub, a.scale).into_any_element()),
    };
    // Trefferfläche wie in der Mac-App: Schließen die rechten 30 pt, Umbenennen die 20 davor, über die ganze Zeilenhöhe.
    let (zone_w, pad) = if icon == Icon::Close { (30., 10.) } else { (20., 0.) };
    let mut el = div()
        .id(SharedString::from(format!("row-{}-{icon:?}", c.session.key)))
        .absolute()
        .left(px(a.s(x - 4.)))
        .top_0()
        .w(px(a.s(zone_w)))
        .h_full()
        .pr(px(a.s(pad)))
        .flex()
        .items_center()
        .justify_end()
        .role(Role::Button)
        .aria_label(label)
        .cursor_pointer()
        .child(glyph);
    if let Some(on) = c.on_icon.clone() {
        el = el.on_click(move |ev, window, cx| {
            on(icon, ev, window, cx);
            cx.stop_propagation();
        });
    }
    el.into_any_element()
}

/// Die Zeile als Element.
pub fn row(c: &RowCell, cx: &App) -> AnyElement {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let s = c.session;
    let width = c.width / a.scale;
    let w = |text: &str, bold: bool| text_width(cx, text, 10.5, bold);
    let label = (!c.group.name.is_empty()).then(|| fit_label(&c.group.name, width / 3., &|l| w(l, true)));
    let age = w(&s.age, false);
    let probe = RowIn { width, age, label: label.as_deref().map_or(0., |l| w(l, true)), hovered: c.hovered, show_path: false, path: 0. };
    // Der Pfad bekommt höchstens die Hälfte dessen, was links neben Gruppe und Alter übrig ist.
    let rx = row_layout(probe).group.map_or(row_layout(probe).age_x, |g| g.1 - 11.);
    let path = c.show_path.then(|| tile_look::fit_path(&s.look.short_cwd, &|p| w(p, false) <= ((rx - 16. - 28.) / 2.).max(0.)));
    let layout = row_layout(RowIn { show_path: path.is_some(), path: path.as_deref().map_or(0., |p| w(p, false)), ..probe });

    let hover = c.hovered;
    let ground = if c.on { t.surface } else { t.panel };
    let mut el = div().relative().size_full().overflow_hidden().bg(a.panel_fill(ground)).rounded_tl(px(c.corner));
    if c.body_above {
        el = el.child(hairline(0., seam_light(&t)));
    }
    for rim in rim(c, &t) {
        el = el.child(rim);
    }
    if !c.body_below {
        el = el.child(div().absolute().bottom_0().left_0().right_0().child(h_seam(&t)));
    }
    el = el.child(div().absolute().left(px(a.s(12.))).top(px((c.height - a.s(8.)) / 2.)).child(dot(s, &t, &a)));
    let title_color = if c.on || hover { t.fg } else { t.sub };
    let mut title = div()
        .absolute()
        .left(px(a.s(28.)))
        .top(px((c.height - a.s(16.)) / 2.))
        .w(px(a.s(layout.title_w)))
        .h(px(a.s(16.)))
        .flex()
        .items_center()
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis()
        .text_size(px(a.s(11.5)))
        .text_color(title_color)
        .child(s.title.clone());
    if c.on {
        title = title.font_weight(FontWeight::BOLD);
    }
    el = el.child(title);
    let small = |x: f32, text: String, color: gpui::Hsla, bold: bool, max: Option<f32>| {
        let d = div().absolute().left(px(a.s(x))).top(px((c.height - a.s(14.)) / 2.)).h(px(a.s(14.))).flex().items_center().whitespace_nowrap().overflow_hidden().text_ellipsis().text_size(px(a.s(10.5))).text_color(color);
        let d = match max {
            Some(m) => d.w(px(a.s(m))),
            None => d,
        };
        (if bold { d.font_weight(FontWeight::BOLD) } else { d }).child(text)
    };
    el = el.child(small(layout.age_x, s.age.clone(), t.muted, false, None));
    if let (Some((accent, x)), Some(label)) = (layout.group, label) {
        el = el.child(div().absolute().left(px(a.s(accent - 3.))).top(px((c.height - a.s(6.)) / 2.)).size(px(a.s(6.))).rounded(px(a.s(1.5))).bg(t.group(c.group.color)));
        el = el.child(small(x, label, t.sub, true, None));
    }
    if let (Some((x, pw)), Some(path)) = (layout.path, path) {
        el = el.child(small(x, path, t.muted, false, Some(pw)));
    }
    if let (Some(x), true) = (layout.close_x, hover) {
        el = el.child(icon_button(Icon::Close, x, c, &t, &a));
    }
    if let (Some(x), true) = (layout.pen_x, hover) {
        el = el.child(icon_button(Icon::Rename, x, c, &t, &a));
    }
    let spoken = a11y::spoken(s.status, s.ended, false);
    let label = a11y::tile_label(&s.title, &spoken, &c.group.name, None);
    let drag = DragTile { key: s.key.clone(), label: s.title.clone() };
    el.id(SharedString::from(format!("stack-row-{}", s.key)))
        .role(Role::Button)
        .aria_label(label)
        .cursor_pointer()
        .on_drag(drag, |d, _, _, cx| cx.new(|_| DragGhost { label: d.label.clone() }))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> RowIn {
        RowIn { width: 600., age: 30., label: 50., hovered: false, show_path: true, path: 100. }
    }

    #[test]
    fn the_right_side_is_laid_out_from_the_right_with_10_margin() {
        let o = row_layout(base());
        assert_eq!(o.age_x, 600. - 10. - 30.);
        let (accent, label_x) = o.group.unwrap();
        assert_eq!(label_x, o.age_x - 8. - 50.);
        assert_eq!(accent, label_x - 8.);
        let (px, pw) = o.path.unwrap();
        assert_eq!(pw, 100.);
        assert_eq!(px, label_x - 11. - 16. - 100.);
        assert_eq!(o.title_w, px - 10. - 28.);
        assert!(o.close_x.is_none());
    }

    #[test]
    fn hover_reserves_44_points_for_close_and_rename() {
        let o = row_layout(RowIn { hovered: true, ..base() });
        assert_eq!(o.close_x, Some(600. - 10. - 16.));
        assert_eq!(o.pen_x, Some(600. - 10. - 36.));
        assert_eq!(o.age_x, 600. - 10. - 44. - 30.);
    }

    #[test]
    fn the_path_takes_at_most_half_of_the_room_left_of_the_group() {
        let o = row_layout(RowIn { path: 1000., ..base() });
        let rx = 600. - 10. - 30. - 8. - 50. - 11.;
        assert_eq!(o.path.unwrap().1, (rx - 16. - 28.) / 2.);
        let none = row_layout(RowIn { show_path: false, label: 0., ..base() });
        assert!(none.path.is_none() && none.group.is_none());
        assert_eq!(none.title_w, 600. - 10. - 30. - 10. - 28.);
    }

    #[test]
    fn the_group_label_shrinks_with_an_ellipsis() {
        let width = |s: &str| s.chars().count() as f32 * 10.;
        assert_eq!(fit_label("Kadrell", 100., &width), "KADRELL");
        assert_eq!(fit_label("Kadrell", 55., &width), "KADR…");
        assert_eq!(fit_label("", 0., &width), "");
    }

    #[test]
    fn narrow_rows_never_get_negative_widths() {
        let o = row_layout(RowIn { width: 100., ..base() });
        assert!(o.title_w >= 0. && o.path.is_none_or(|p| p.1 >= 0.));
    }
}
