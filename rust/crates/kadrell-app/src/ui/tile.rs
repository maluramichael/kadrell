//! Rechte Kachel, Port von `CellView.swift`: Kopfzeile (Status, Titel, Zweig, Gruppe, Alter, Symbole beim Überfahren),
//! Körper (Terminal oder Zustandsanzeige, siehe `tile_body`), Rand und innerer Lichtrand. Der Rand liegt als Schicht über
//! dem Inhalt (2 px bei Fokus, sonst 1 px, Farbe nach `tile_look::edge`), damit ein Fokuswechsel das Terminal nicht
//! umbricht. Das Terminal sitzt 2 px innerhalb des Rahmens. Maße folgen der UI-Größe, die Flächen der Deckkraft
//! (Hintergrundbild scheint durch), der Rahmen blendet bei Fokuswechsel in 120 ms über.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, App, AppContext, ClickEvent, Div, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Role, SharedString,
    StatefulInteractiveElement, Styled, div, font, px,
};

use super::chrome::{h_seam, seam_dark, seam_light};
use super::groups::drag::DragTile;
use super::groups::rows::DragGhost;
use super::instrument::{LABEL_KERN, kerned, kerned_width};
use super::model::{Group, Session};
use super::tile_body::{self, Body};
use super::tile_edge;
use super::groups::tool_icons;
use super::tile_look::{self, Edge, HeaderIn, Icon, Run};
use super::layout::Rect;
use super::theme;
use crate::appearance::{Appearance, a11y, breathing};
use crate::theme::Theme;

/// Was an einer Kachel zu sehen ist, über Stil und Inhalt hinaus.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Marks {
    /// Die Tastaturkachel.
    pub active: bool,
    /// Fokuskante in Vordergrundfarbe: die Tastaturkachel, im Sync-Modus alle (alle bekommen die Eingabe).
    pub ring: bool,
    /// Badge „Z“: gezoomt, obwohl mehrere Sessions offen sind (wie in tmux).
    pub zoomed: bool,
    /// Vorschau aus dem Baum, ohne Tastatur.
    pub preview: bool,
}

/// Lage und Zustand der Kachel in der Arbeitsfläche.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub width: f32,
    pub height: f32,
    /// Radius der Ecke oben links, wenn die Kachel in der Modul-Ecke liegt (`tile_look::corner_radius`).
    pub corner: f32,
    /// Die Maus ist über der Kachel: Symbole, 84 pt Reserve rechts.
    pub hovered: bool,
    /// Stack: die Zeile steht für sich, die Kachel zeigt nur den Körper.
    pub header_hidden: bool,
    /// Stack: direkt unter dem Körper folgt eine Zeile, die Unterkante bleibt offen.
    pub row_below: bool,
}

impl Geometry {
    pub fn of(r: Rect) -> Geometry {
        Geometry { width: r.w, height: r.h, corner: 0., hovered: false, header_hidden: false, row_below: false }
    }
}

/// Breite von `text` in Punkten, Zeichen für Zeichen aus der Oberflächenschrift (wie `NSAttributedString.size()` ohne
/// Unterschneidung), auf ein ganzes Gerätepixel aufgerundet: GPUI rundet feste Breiten auf Gerätepixel, ein kleineres Maß kürzte den Titel mit
/// einer Auslassung. Die frühere Zugabe von 3 % und einem Pixel machte die Lücke zum Branch 2 pt zu groß.
pub fn text_width(cx: &App, text: &str, size: f32, bold: bool) -> f32 {
    if text.is_empty() {
        return 0.;
    }
    let mut f = font(crate::theme::FONT);
    if bold {
        f.weight = FontWeight::BOLD;
    }
    let system = cx.text_system();
    let id = system.resolve_font(&f);
    let sum: f32 = text.chars().map(|c| system.layout_width(id, px(size), c).as_f32()).sum();
    let d = super::overlay::hairline();
    (sum / d - 1e-3).ceil() * d
}

/// Statuspunkt wie in der Seitenleiste (`Theme.activityColor` / `statusColor`): arbeitende Sessions atmen, auch fertige
/// mit laufendem Kommando; Form und Shell-Farbe wie im Baum.
pub fn dot(session: &Session, t: &Theme, a: &Appearance) -> AnyElement {
    super::sidebar::status_dot(session, t, a)
}

/// Gruppen-Label in Versalien (10,5 fett), wie im Baum.
pub fn group_label(name: &str) -> String {
    name.to_uppercase()
}

/// Rahmenfarbe nach Rangfolge.
fn edge_color(edge: Edge, t: &Theme) -> Hsla {
    match edge {
        Edge::Drop | Edge::Focus => t.fg,
        Edge::Error | Edge::StatusError => t.error,
        Edge::Waiting => t.waiting,
        Edge::Hit => t.accent,
        Edge::Seam => seam_dark(t),
    }
}

/// Klick auf ein Symbol der Kopfzeile: der Besitzer (die Shell) führt es aus.
pub type IconClick = std::rc::Rc<dyn Fn(Icon, &ClickEvent, &mut gpui::Window, &mut App)>;

/// Eingaben der Kachel in einem Stück.
pub struct Cell<'a> {
    pub group: &'a Group,
    pub session: &'a Session,
    pub marks: Marks,
    pub geo: Geometry,
    /// Körper zeigen (false: nur die Kopfzeile, wie im alten Stack des zweiten Fensters).
    pub body: bool,
    pub elsewhere: bool,
    pub on_icon: Option<IconClick>,
    /// Aktivieren der Kopfzeile ohne Zeiger (Bedienhilfe „Drücken“): die Kachel bekommt den Fokus.
    pub on_head: Option<HeadClick>,
}

/// Die Kopfzeile wird aktiviert.
pub type HeadClick = std::rc::Rc<dyn Fn(&mut gpui::Window, &mut App)>;

/// Beschriftung der Kopfzeile für Bedienhilfen: Titel, Status, Gruppe und Zweig.
fn header_label(c: &Cell) -> String {
    let spoken = a11y::spoken(c.session.status, c.session.ended, false);
    a11y::tile_label(&c.session.title, &spoken, &c.group.name, c.session.branch.as_deref())
}

impl Cell<'_> {
    fn run(&self) -> Run {
        let s = self.session;
        Run { missing_folder: s.look.missing_folder, exit_code: s.look.exit_code, ended: s.ended, attached: s.look.attached, previewing: self.marks.preview, elsewhere: self.elsewhere }
    }

    fn edge(&self) -> Edge {
        tile_look::edge(false, self.run(), self.marks.ring, self.session.status)
    }
}

fn zoom_badge(t: &Theme, a: &Appearance) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(a.s(16.)))
        .bg(t.waiting)
        .text_color(t.pill_text_on(t.waiting))
        .text_size(px(a.s(11.)))
        .font_weight(FontWeight::BOLD)
        .child("Z")
}

fn text_at(a: &Appearance, x: f32, y: f32, w: Option<f32>) -> Div {
    let d = div().absolute().left(px(a.s(x))).top(px(a.s(y))).h(px(a.s(16.))).flex().items_center().whitespace_nowrap().overflow_hidden().text_ellipsis();
    match w {
        Some(w) => d.w(px(a.s(w.max(0.)))),
        None => d.flex_none(),
    }
}

/// Symbole beim Überfahren: schließen, umbenennen, aus der Ansicht nehmen, zoomen. Die Trefferflächen sind größer als die
/// Symbole (Kreuz 4 pt rundum, die anderen 2 seitlich und 4 oben und unten) und immer da, sichtbar nur beim Überfahren.
fn icons(c: &Cell, width: f32, t: &Theme, a: &Appearance) -> Vec<AnyElement> {
    let key = &c.session.key;
    let hovered = c.geo.hovered;
    [Icon::Close, Icon::Rename, Icon::Deselect, Icon::Zoom]
        .into_iter()
        .map(|icon| {
            let (x, y, w, h) = tile_look::icon_rect(icon, width);
            let dx = if icon == Icon::Close { 4. } else { 2. };
            let label = match icon {
                Icon::Close => kadrell_i18n::trd!("Schließen"),
                Icon::Rename => kadrell_i18n::trd!("Umbenennen"),
                Icon::Deselect => kadrell_i18n::trd!("Aus Ansicht nehmen"),
                Icon::Zoom if c.marks.zoomed => kadrell_i18n::trd!("Zoom aufheben"),
                Icon::Zoom => kadrell_i18n::trd!("Zoomen"),
            };
            let glyph = match icon {
                Icon::Close => tool_icons::x_at(16. * a.scale, t.sub).into_any_element(),
                Icon::Rename => tool_icons::pen_at(16. * a.scale, t.sub).into_any_element(),
                Icon::Deselect => tool_icons::circle_at(16. * a.scale, t.sub).into_any_element(),
                Icon::Zoom if c.marks.zoomed => tool_icons::minus_at(16. * a.scale, t.sub).into_any_element(),
                Icon::Zoom => tool_icons::square_at(16. * a.scale, t.sub).into_any_element(),
            };
            let mut el = div()
                .id(SharedString::from(format!("tile-{key}-{icon:?}")))
                .absolute()
                .left(px(a.s(x - dx)))
                .top(px(a.s(y - 4.)))
                .w(px(a.s(w + 2. * dx)))
                .h(px(a.s(h + 8.)))
                .flex()
                .items_center()
                .justify_center()
                .opacity(if hovered { 1. } else { 0. })
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
        })
        .collect()
}

/// Maße der Textteile der Kopfzeile.
fn header_in(c: &Cell, width: f32, cx: &App) -> HeaderIn {
    let s = c.session;
    let measure = |text: &str, size: f32, bold: bool| text_width(cx, text, size, bold);
    HeaderIn {
        width,
        title: measure(&s.title, 11.5, true),
        branch: s.branch.as_deref().map_or(0., |b| measure(b, 10.5, false)),
        label: if c.group.name.is_empty() { 0. } else { kerned_width(measure(&group_label(&c.group.name), 10.5, true), &c.group.name, LABEL_KERN) },
        meta: measure(&s.age, 10.5, false),
        hovered: c.geo.hovered,
        zoomed: c.marks.zoomed,
    }
}

/// Kopfzeile: neutral in `surface`, unten die Fuge zum Körper. Die Gruppe zeigt nur ihren Akzent am Label.
fn header(c: &Cell, t: &Theme, a: &Appearance, cx: &App) -> Div {
    let width = c.geo.width / a.scale;
    let layout = tile_look::header_layout(header_in(c, width, cx));
    let s = c.session;
    let mut row = div().relative().flex_none().w_full().h(px(tile_look::header_height(a.scale))).bg(a.panel_fill(t.surface));
    row = row.child(div().absolute().bottom_0().left_0().right_0().child(h_seam(t)));
    row = row.child(div().absolute().left(px(a.s(9.))).top(px(a.s(9.))).child(dot(s, t, a)));
    row = row.child(text_at(a, 24., 5., Some(layout.title_w)).text_size(px(a.s(11.5))).font_weight(FontWeight::BOLD).text_color(t.fg).child(s.title.clone()));
    if let (Some(x), Some(branch)) = (layout.branch_x, &s.branch) {
        row = row.child(text_at(a, x, 5., None).text_size(px(a.s(10.5))).text_color(t.muted).child(branch.clone()));
    }
    if let Some((accent, label_x, label_w)) = layout.group {
        row = row.child(div().absolute().left(px(a.s(accent - 3.))).top(px(a.s(10.))).size(px(a.s(6.))).rounded(px(a.s(1.5))).bg(t.group(c.group.color)));
        row = row.child(text_at(a, label_x, 5., Some(label_w)).text_size(px(a.s(10.5))).font_weight(FontWeight::BOLD).text_color(t.sub).child(kerned(&group_label(&c.group.name), a.s(10.5), a.s(LABEL_KERN), true, t.sub)));
    }
    if let Some(x) = layout.zoom_x {
        row = row.child(div().absolute().left(px(a.s(x))).top(px(a.s(5.))).child(zoom_badge(t, a)));
    }
    row = row.child(text_at(a, layout.meta_x, 5., None).text_size(px(a.s(10.5))).text_color(t.muted).child(s.age.clone()));
    row.children(icons(c, width, t, a))
}

/// Körper: Terminal mit 2 px Abstand zum Rahmen (links, rechts, unten, ohne Kopfzeile auch oben), sonst die Zustandsanzeige.
fn body_area(c: &Cell, kind: tile_look::BodyKind, t: &Theme, a: &Appearance, cx: &App) -> Div {
    let ground = a.panel_fill(t.bg);
    if kind == tile_look::BodyKind::Terminal {
        let top = if c.geo.header_hidden { 2. } else { 0. };
        // Das Terminal malt seinen Grund selbst (mit der Deckkraft), ein zweiter darunter deckte doppelt: der Abstand zum
        // Rahmen ist deshalb ein Rand in der Grundfarbe, nicht eine Fläche.
        let area = div().flex_1().min_h_0().overflow_hidden().border_l(px(2.)).border_r(px(2.)).border_b(px(2.)).border_t(px(top)).border_color(ground);
        return match &c.session.pane {
            Some(pane) => area.child(pane.clone()),
            None => area.bg(ground),
        };
    }
    let header = if c.geo.header_hidden { 0. } else { tile_look::header_height(a.scale) };
    let plain = |s: &str| text_width(cx, s, 11., false);
    let body = Body { kind, lines: &c.session.look.lines, short_cwd: &c.session.look.short_cwd, height: Some(c.geo.height - header), fit: Some((c.geo.width / a.scale, &plain)) };
    div().flex_1().min_h_0().overflow_hidden().child(tile_body::view(&body, t, a))
}

/// Innerer Lichtrand (`drawInnerRim`): 1 pt `seamLight` innerhalb des dunklen Rands. Folgt im Stack eine Zeile, bleibt die
/// Unterkante offen und die Seiten laufen bis an den Rand der Kachel.
fn inner_rim(c: &Cell, t: &Theme, a: &Appearance) -> Div {
    let open = c.geo.row_below;
    let rim = div().absolute().left(px(1.)).right(px(1.)).top(px(1.)).bottom(px(if open { 0. } else { 1. })).border_1().border_color(seam_light(t));
    let rim = if open { rim.border_b(px(0.)) } else { rim };
    rim.rounded_tl(px(tile_edge::clamp_corner(c.geo.corner - 1.5 * a.scale, c.geo.width, c.geo.height)))
}

/// Rand als Schicht über dem Inhalt; wechselt die Farbe, blendet er in 120 ms über.
fn border_layer(c: &Cell, t: &Theme, hit: Option<bool>) -> AnyElement {
    let edge = tile_look::with_hit(c.edge(), hit);
    let width = tile_look::edge_width_for(edge, c.marks.ring);
    let (to, base) = (edge_color(edge, t), edge_color(Edge::Seam, t));
    let layer = div().absolute().size_full().border(px(width)).rounded_tl(px(tile_edge::clamp_corner(c.geo.corner, c.geo.width, c.geo.height)));
    // Wechselt die Rangfolge, ändert sich die Kennung und der Rand blendet von der alten zur neuen Farbe über.
    let from = if edge == Edge::Seam { edge_color(Edge::Focus, t) } else { base };
    layer
        .border_color(to)
        .with_animation(
            gpui::ElementId::Name(format!("tile-border-{}-{edge:?}", c.session.key).into()),
            Animation::new(Duration::from_millis(120)),
            move |el, delta| el.border_color(breathing::fade(from, to, Duration::from_secs_f32(delta * 0.12))),
        )
        .into_any_element()
}

/// Eine Kachel in der Arbeitsfläche.
pub fn cell(c: &Cell, cx: &App) -> AnyElement {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let hit = tile_look::palette_hit(cx, &c.session.key);
    let kind = tile_look::body_kind_mounted(c.run(), c.session.pane.is_some() && !c.elsewhere);
    let mut col = div().relative().flex().flex_col().size_full().overflow_hidden().rounded_tl(px(c.geo.corner));
    if !c.geo.header_hidden {
        let head = header(c, &t, &a, cx);
        let drag = DragTile { key: c.session.key.clone(), label: c.session.title.clone() };
        // Ziehen an der Titelzeile sortiert um (Ablage in `layout::placed`).
        let mut head = head
            .id(SharedString::from(format!("tile-head-{}", c.session.key)))
            .role(Role::Button)
            .aria_label(header_label(c))
            .cursor_grab()
            .on_drag(drag, |d, _, _, cx| cx.new(|_| DragGhost { label: d.label.clone() }));
        if let Some(on) = c.on_head.clone() {
            head = head.on_click(move |_, window, cx| on(window, cx));
        }
        col = col.child(head);
    }
    if c.body {
        col = col.child(body_area(c, kind, &t, &a, cx));
    }
    col = col.child(inner_rim(c, &t, &a)).child(border_layer(c, &t, hit));
    // Die Gruppe trägt nur den Titel; die Kopfzeile ist der Knopf mit Status, Gruppe und Zweig (`CellView.accessibilityChildren`).
    if hit == Some(false) {
        col = col.opacity(tile_look::DIMMED_OPACITY);
    }
    col.id(gpui::ElementId::Name(format!("tile-{}", c.session.key).into())).role(Role::Group).aria_label(c.session.title.clone()).into_any_element()
}
