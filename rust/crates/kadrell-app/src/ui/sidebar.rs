//! Linke Seitenleiste: der Baum der Gruppen und Sessions in einem der beiden Designs der Mac-App (Klassisch mit Pfad
//! unter dem Gruppennamen, Kompakt ohne Pfad mit Status-Punkten im eingeklappten Kopf und Fuge zwischen den Gruppen).
//! Maße kommen aus `appearance::metrics`, alles wird mit der UI-Größe multipliziert und folgt Design, Laufzeit-Anzeige
//! und Deckkraft der Einstellungen sofort. Der Baum ist für Bildschirmleser eine Gliederung (`appearance::a11y`).

use gpui::{
    Context, Div, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Role, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};
use kadrell_i18n::trd;

use super::chrome::{h_seam, v_seam};
use super::icons;
use super::layout::Selection;
use super::model::{Group, Session, Status};
use super::theme;
use crate::Shell;
use crate::appearance::metrics::{self, ROW_INSET};
use crate::appearance::{Appearance, a11y, breathing};
use crate::theme::Theme;

const OUTLINE: f32 = 1.5;

/// Zeilenrahmen mit optionalem Auswahlbalken (3 px links, volle Zeilenhöhe) und der Fläche darin.
fn row_frame(id: impl Into<gpui::ElementId>, height: f32, bar: Option<Hsla>, a: &Appearance) -> Stateful<Div> {
    let mut row = div()
        .id(id)
        .relative()
        .flex_none()
        .w_full()
        .h(px(a.s(height)))
        .px(px(a.s(ROW_INSET.0)))
        .py(px(a.s(ROW_INSET.1)))
        .cursor_pointer();
    if let Some(color) = bar {
        row = row.child(div().absolute().left_0().top_0().w(px(a.s(3.))).h(px(a.s(height))).bg(color));
    }
    row
}

fn outline(el: Div, color: Hsla, a: &Appearance) -> Div {
    let w = px(a.s(OUTLINE));
    el.border_t(w).border_b(w).border_l(w).border_r(w).border_color(color)
}

/// Gesperrtes Versal-Label mit Zahl: `KADRELL · 2`.
fn group_label(group: &Group, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .text_size(px(a.s(10.5)))
        .font_weight(FontWeight::BOLD)
        .text_color(t.sub)
        .whitespace_nowrap()
        .text_ellipsis()
        .child(format!("{} · {}", group.name.to_uppercase(), group.sessions.len()))
}

/// Chevron als eigene Klickfläche: klappt die Gruppe auf oder zu, ohne sie zu markieren.
fn chevron_button(g: usize, group: &Group, t: &Theme, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    div()
        .id(("chevron", g))
        .flex_none()
        .child(icons::chevron_scaled(!group.collapsed, t.muted, a.scale))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.toggle_group(g, cx);
        }))
}

/// Sessions der Gruppe, die auf dich warten, auch bei eingeklappter Gruppe.
fn waiting(group: &Group) -> usize {
    group.sessions.iter().filter(|s| s.status == Status::Waiting).count()
}

/// „2 ⏳“ links vom Favoriten, nur wenn etwas wartet.
fn waiting_badge(group: &Group, t: &Theme, a: &Appearance) -> Option<Div> {
    let n = waiting(group);
    (n > 0).then(|| div().flex_none().ml(px(a.s(6.))).text_size(px(a.s(10.5))).font_weight(FontWeight::BOLD).text_color(t.waiting).child(format!("{n} ⏳")))
}

/// Favoritenherz ganz rechts in der Kopfzeile.
fn favorite(group: &Group, t: &Theme, a: &Appearance) -> Option<Div> {
    group.favorite.then(|| div().flex_none().ml(px(a.s(6.))).text_size(px(a.s(11.))).text_color(t.group(group.color)).child("♥"))
}

/// Kompakt, eingeklappt: ein Punkt je Session, so bleibt der Status sichtbar. Aufgeklappt zeigen ihn die Zeilen selbst.
fn status_dots(group: &Group, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    let dots = group.sessions.iter().map(|s| div().flex_none().size(px(a.s(5.))).rounded_full().bg(s.status.color(t)));
    div().flex().flex_none().items_center().gap(px(a.s(3.))).mr(px(a.s(5.))).children(dots)
}

fn group_head(group: &Group, t: &Theme, a: &Appearance, chevron: gpui::AnyElement) -> impl IntoElement + use<> {
    let m = a.rows();
    let compact_dots = !m.path && group.collapsed;
    div()
        .flex()
        .items_center()
        .h(px(a.s(m.group_head() - 2. * ROW_INSET.1)))
        .child(chevron)
        .child(div().ml(px(a.s(4.))).mr(px(a.s(5.))).size(px(a.s(6.))).flex_none().rounded(px(a.s(1.5))).bg(t.group(group.color)))
        .child(group_label(group, t, a))
        .children(compact_dots.then(|| status_dots(group, t, a)))
        .children(waiting_badge(group, t, a))
        .children(favorite(group, t, a))
}

fn group_path(group: &Group, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    div()
        .ml(px(a.s(metrics::GROUP_PATH_X - metrics::ROW_INSET.0)))
        .h(px(a.s(14.)))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(a.s(10.5)))
        .text_color(t.muted)
        .child(group.path.clone())
}

fn group_header(g: usize, group: &Group, holds_selection: bool, first: bool, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let t = theme(cx);
    let m = a.rows();
    let bar = holds_selection.then(|| t.group(group.color));
    let face = div()
        .size_full()
        .rounded(px(a.s(6.)))
        .flex()
        .flex_col()
        .hover(move |s| s.bg(t.bg))
        .child(group_head(group, &t, a, chevron_button(g, group, &t, a, cx).into_any_element()));
    let face = if m.path { face.child(group_path(group, &t, a)) } else { face };
    let label = a11y::group_label(&group.name, group.sessions.len());
    let row = row_frame(("group", g), m.group_row, bar, a)
        .role(Role::TreeItem)
        .aria_label(label)
        .aria_level(a11y::level(true, true))
        .aria_expanded(!group.collapsed)
        .on_click(cx.listener(move |this, _, window, cx| this.select_group(g, window, cx)))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            let id = this.groups[g].id.clone();
            if !id.is_empty() {
                let items = this.group_menu_items(&id);
                this.open_context(ev.position, items, cx);
            }
        }))
        .child(face);
    // Kompakt trennt eine Fuge die Gruppen, nur die erste hat keine.
    div().flex().flex_col().flex_none().children((!m.path && !first).then(|| h_seam(&t))).child(row)
}

fn new_badge(status: Status, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    let face = if status == Status::Waiting { t.waiting } else { t.running };
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(a.s(16.)))
        .px(px(a.s(5.)))
        .rounded(px(a.s(3.)))
        .bg(face)
        .text_color(t.pill_text_on(face))
        .text_size(px(a.s(10.)))
        .font_weight(FontWeight::BOLD)
        .child(trd!("neu"))
}

/// Statuspunkt; arbeitende Sessions bekommen den atmenden Punkt (Warten und Fehler behalten den Punkt).
pub(crate) fn status_dot(session: &Session, t: &Theme, a: &Appearance) -> gpui::AnyElement {
    let color = session.status.color(t);
    if session.status == Status::Running {
        return div().flex_none().child(breathing::breathing_dot(color, a.s(metrics::DOT_SIZE), a.s(breathing::GLOW_RADIUS))).into_any_element();
    }
    div().size(px(a.s(metrics::DOT_SIZE))).flex_none().rounded_full().bg(color).into_any_element()
}

fn session_face(session: &Session, selected: bool, t: &Theme, a: &Appearance) -> Div {
    let title_color = if selected || session.is_new { t.fg } else { t.sub };
    let title = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(title_color).child(session.title.clone());
    let title = if session.is_new { title.font_weight(FontWeight::BOLD) } else { title };
    let mut face = div()
        .size_full()
        .rounded(px(a.s(6.)))
        .flex()
        .items_center()
        .gap(px(a.s(6.)))
        .pl(px(a.s(4.5)))
        .pr(px(a.s(2.5)))
        .child(status_dot(session, t, a))
        .child(title);
    if session.is_new {
        face = face.child(new_badge(session.status, t, a));
    }
    if a.show_age {
        face = face.child(div().flex_none().text_size(px(a.s(10.5))).text_color(t.muted).child(session.age.clone()));
    }
    face
}

/// Zustand einer Session-Zeile: ausgewählt, Fokus, Baum gruppiert.
#[derive(Clone, Copy)]
struct RowState {
    selected: bool,
    focused: bool,
    grouped: bool,
    /// Der Baum hat die Tastatur (⌘1): die Fokus-Session trägt die Akzentfarbe.
    keys: bool,
}

fn session_row(g: usize, s: usize, group: &Group, st: RowState, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let RowState { selected, focused, grouped, keys } = st;
    let t = theme(cx);
    let session = &group.sessions[s];
    let mut face = session_face(session, selected, &t, a).hover(move |st| st.bg(t.bg).text_color(t.fg));
    // Der Rand steht immer, nur seine Farbe wechselt: Inhalt springt bei der Auswahl nicht.
    // Fokus-Session in `fg`, die übrigen markierten in `muted`. Hat der Baum die Tastatur (⌘1), trägt die Fokus-Session
    // die Akzentfarbe.
    let edge = match (focused, selected) {
        (true, _) if keys => t.accent,
        (true, _) => t.fg,
        (false, true) => t.muted,
        _ => gpui::transparent_black(),
    };
    if selected {
        face = face.bg(t.bg);
    }
    face = outline(face, edge, a);
    let bar = selected.then(|| t.group(group.color));
    let spoken = a11y::spoken(session.status, session.ended, false);
    row_frame(("session", g * 100 + s), a.rows().session_row, bar, a)
        .role(Role::TreeItem)
        .aria_label(a11y::session_label(&session.title))
        .aria_value(a11y::session_value(&spoken, session.is_new, selected))
        .aria_level(a11y::level(false, grouped))
        .aria_selected(selected)
        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
            this.click_session(g, s, ev.modifiers(), window, cx)
        }))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            let key = this.groups[g].sessions[s].key.clone();
            let items = this.session_menu_items(&key);
            this.open_context(ev.position, items, cx);
        }))
        .child(face)
}

fn group_block(g: usize, group: &Group, sel: &Selection, grouped: bool, keys: bool, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let m = a.rows();
    // Klassisch: Luft vor jeder Gruppe außer der ersten.
    let gap = if g > 0 { a.s(m.group_gap) } else { 0. };
    let mut block = div().flex().flex_col().flex_none().mt(px(gap)).child(group_header(g, group, sel.focus.0 == g, g == 0, a, cx));
    if group.collapsed {
        return block;
    }
    for s in 0..group.sessions.len() {
        block = block.child(session_row(g, s, group, RowState { selected: sel.contains((g, s)), focused: sel.focus == (g, s), grouped, keys }, a, cx));
    }
    block
}

/// `keys`: die Tastatur gehört dem Baum (⌘1).
pub fn sidebar(groups: &[Group], sel: &Selection, keys: bool, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let grouped = groups.len() != 1 || !groups[0].id.is_empty();
    let mut col = div()
        .id("sidebar")
        .role(Role::Tree)
        .aria_label(trd!("Sessions"))
        .flex()
        .flex_col()
        .flex_none()
        .w(px(metrics::sidebar_width(a.scale)))
        .h_full()
        .pt(px(a.s(a.rows().top_inset)))
        .pb(px(a.s(8.)))
        .overflow_y_scroll()
        .bg(a.panel_fill(t.panel))
        .text_size(px(a.s(12.)));
    for (g, group) in groups.iter().enumerate() {
        col = col.child(group_block(g, group, sel, grouped, keys, &a, cx));
    }
    div().flex().flex_none().h_full().child(col).child(v_seam(&t))
}
