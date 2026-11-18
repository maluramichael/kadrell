//! Linke Seitenleiste im Stil des klassischen Baums der Mac-App: Gruppenzeile mit Pfad darunter, Sessions
//! ohne Einrückung, Auswahlbalken in Gruppenfarbe, Zeilenfläche in `bg` mit Umriss in `fg`.

use kadrell_i18n::trd;
use gpui::{
    Context, Div, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};

use super::chrome::v_seam;
use super::icons;
use super::layout::Selection;
use super::model::{Group, Session, Status};
use super::theme;
use crate::Shell;
use crate::theme::Theme;

const SIDEBAR_WIDTH: f32 = 260.;
/// Mac-App zeigt im Baum standardmäßig keinen Pfad unter dem Gruppennamen (mac-ist-3.png); `true` zeigt ihn wie im klassischen Baum.
const SHOW_GROUP_PATH: bool = false;
const GROUP_ROW: f32 = if SHOW_GROUP_PATH { 38. } else { 24. };
const SESSION_ROW: f32 = 24.;
/// Rand der Zeilenfläche: 6 px seitlich, 1 px oben und unten (`rowInset` der Mac-App).
const ROW_INSET_X: f32 = 6.;
const ROW_INSET_Y: f32 = 1.;
const OUTLINE: f32 = 1.5;

/// Zeilenrahmen mit optionalem Auswahlbalken (3 px links, volle Zeilenhöhe) und der Fläche darin.
fn row_frame(id: impl Into<gpui::ElementId>, height: f32, bar: Option<Hsla>) -> Stateful<Div> {
    let mut row = div()
        .id(id)
        .relative()
        .flex_none()
        .w_full()
        .h(px(height))
        .px(px(ROW_INSET_X))
        .py(px(ROW_INSET_Y))
        .cursor_pointer();
    if let Some(color) = bar {
        row = row.child(div().absolute().left_0().top_0().w(px(3.)).h(px(height)).bg(color));
    }
    row
}

fn outline(el: Div, color: Hsla) -> Div {
    let w = px(OUTLINE);
    el.border_t(w).border_b(w).border_l(w).border_r(w).border_color(color)
}

fn chevron(collapsed: bool, t: &Theme) -> impl IntoElement + use<> {
    icons::chevron(!collapsed, t.muted)
}

fn group_label(group: &Group, t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .text_size(px(10.5))
        .font_weight(FontWeight::BOLD)
        .text_color(t.sub)
        .whitespace_nowrap()
        .child(format!("{} · {}", group.name.to_uppercase(), group.sessions.len()))
}

/// Chevron als eigene Klickfläche: klappt die Gruppe auf oder zu, ohne sie zu markieren.
fn chevron_button(g: usize, group: &Group, t: &Theme, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    div()
        .id(("chevron", g))
        .flex_none()
        .child(chevron(group.collapsed, t))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.toggle_group(g, cx);
        }))
}

fn group_head(group: &Group, t: &Theme, chevron: gpui::AnyElement) -> impl IntoElement + use<> {
    div()
        .flex()
        .items_center()
        .h(px(if SHOW_GROUP_PATH { GROUP_ROW - 2. * ROW_INSET_Y - 14. } else { GROUP_ROW - 2. * ROW_INSET_Y }))
        .child(chevron)
        .child(div().ml(px(4.)).mr(px(5.)).size(px(6.)).flex_none().rounded(px(1.5)).bg(t.group(group.color)))
        .child(group_label(group, t))
}

fn group_path(group: &Group, t: &Theme) -> impl IntoElement + use<> {
    div()
        .ml(px(36.))
        .h(px(14.))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(10.5))
        .text_color(t.muted)
        .child(group.path.clone())
}

fn group_header(
    g: usize,
    group: &Group,
    holds_selection: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let t = theme(cx);
    let bar = holds_selection.then(|| t.group(group.color));
    let face = div()
        .size_full()
        .rounded(px(6.))
        .flex()
        .flex_col()
        .hover(move |s| s.bg(t.bg))
        .child(group_head(group, &t, chevron_button(g, group, &t, cx).into_any_element()));
    let face = if SHOW_GROUP_PATH { face.child(group_path(group, &t)) } else { face };
    row_frame(("group", g), GROUP_ROW, bar)
        .on_click(cx.listener(move |this, _, window, cx| this.select_group(g, window, cx)))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            let id = this.groups[g].id.clone();
            if !id.is_empty() {
                let items = this.group_menu_items(&id);
                this.open_context(ev.position, items, cx);
            }
        }))
        .child(face)
}

fn new_badge(status: Status, t: &Theme) -> impl IntoElement + use<> {
    let face = if status == Status::Waiting { t.waiting } else { t.running };
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(16.))
        .px(px(5.))
        .rounded(px(3.))
        .bg(face)
        .text_color(t.pill_text_on(face))
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .child(trd!("neu"))
}

fn session_face(session: &Session, selected: bool, t: &Theme) -> Div {
    let title_color = if selected || session.is_new { t.fg } else { t.sub };
    let title = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(title_color).child(session.title.clone());
    let title = if session.is_new { title.font_weight(FontWeight::BOLD) } else { title };
    let mut face = div()
        .size_full()
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(4.5))
        .pr(px(2.5))
        .child(div().size(px(8.)).flex_none().rounded_full().bg(session.status.color(t)))
        .child(title);
    if session.is_new {
        face = face.child(new_badge(session.status, t));
    }
    face.child(div().flex_none().text_size(px(10.5)).text_color(t.muted).child(session.age.clone()))
}

fn session_row(
    g: usize,
    s: usize,
    group: &Group,
    selected: bool,
    focused: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let t = theme(cx);
    let session = &group.sessions[s];
    let mut face = session_face(session, selected, &t).hover(move |st| st.bg(t.bg).text_color(t.fg));
    // Der Rand steht immer, nur seine Farbe wechselt: Inhalt springt bei der Auswahl nicht.
    // Fokus-Session in `fg`, die übrigen markierten in `muted`.
    let edge = match (focused, selected) {
        (true, _) => t.fg,
        (false, true) => t.muted,
        _ => gpui::transparent_black(),
    };
    if selected {
        face = face.bg(t.bg);
    }
    face = outline(face, edge);
    let bar = selected.then(|| t.group(group.color));
    row_frame(("session", g * 100 + s), SESSION_ROW, bar)
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

fn group_block(
    g: usize,
    group: &Group,
    sel: &Selection,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let mut block = div().flex().flex_col().flex_none().child(group_header(g, group, sel.focus.0 == g, cx));
    if group.collapsed {
        return block;
    }
    for s in 0..group.sessions.len() {
        block = block.child(session_row(g, s, group, sel.contains((g, s)), sel.focus == (g, s), cx));
    }
    block
}

pub fn sidebar(groups: &[Group], sel: &Selection, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let t = theme(cx);
    let mut col = div()
        .id("sidebar")
        .flex()
        .flex_col()
        .flex_none()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .pt(px(6.))
        .pb(px(8.))
        .overflow_y_scroll()
        .bg(t.panel)
        .text_size(px(12.));
    for (g, group) in groups.iter().enumerate() {
        col = col.child(group_block(g, group, sel, cx));
    }
    div().flex().flex_none().h_full().bg(t.panel).child(col).child(v_seam(&t))
}
