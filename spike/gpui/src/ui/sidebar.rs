//! Linke Seitenleiste im Stil des klassischen Baums der Mac-App: Gruppenzeile mit Pfad darunter, Sessions
//! ohne Einrückung, Auswahlbalken in Gruppenfarbe, Zeilenfläche in `bg` mit Umriss in `fg`.

use gpui::{
    Context, Div, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, px, rgb,
};

use super::chrome::v_seam;
use super::model::{Group, Session};
use super::theme;
use crate::Shell;
use crate::theme::Theme;

const SIDEBAR_WIDTH: f32 = 280.;
const GROUP_ROW: f32 = 38.;
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
    div()
        .w(px(16.))
        .flex_none()
        .text_size(px(9.))
        .text_color(t.muted)
        .flex()
        .justify_center()
        .child(if collapsed { "▸" } else { "▾" })
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

fn group_head(group: &Group, t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .items_center()
        .h(px(GROUP_ROW - 2. * ROW_INSET_Y - 14.))
        .child(chevron(group.collapsed, t))
        .child(div().ml(px(4.)).mr(px(5.)).size(px(6.)).flex_none().rounded(px(1.5)).bg(rgb(group.color)))
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
        .child(group.path)
}

fn group_header(
    g: usize,
    group: &Group,
    holds_selection: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let t = theme(cx);
    let bar = holds_selection.then(|| rgb(group.color).into());
    let face = div()
        .size_full()
        .rounded(px(6.))
        .flex()
        .flex_col()
        .hover(move |s| s.bg(t.bg))
        .child(group_head(group, &t))
        .child(group_path(group, &t));
    row_frame(("group", g), GROUP_ROW, bar)
        .on_click(cx.listener(move |this, _, _, cx| this.toggle_group(g, cx)))
        .child(face)
}

fn new_badge(t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(16.))
        .px(px(5.))
        .rounded(px(3.))
        .bg(t.running)
        .text_color(t.pill_text())
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .child("neu")
}

fn session_face(session: &Session, selected: bool, t: &Theme) -> Div {
    let title_color = if selected || session.is_new { t.fg } else { t.sub };
    let title = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(title_color).child(session.title);
    let title = if session.is_new { title.font_weight(FontWeight::BOLD) } else { title };
    let mut face = div()
        .size_full()
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(6.))
        .pr(px(4.))
        .child(div().size(px(8.)).flex_none().rounded_full().bg(session.status.color(t)))
        .child(title);
    if session.is_new {
        face = face.child(new_badge(t));
    }
    face.child(div().flex_none().text_size(px(10.5)).text_color(t.muted).child(session.age))
}

fn session_row(
    g: usize,
    s: usize,
    group: &Group,
    selected: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let t = theme(cx);
    let session = &group.sessions[s];
    let mut face = session_face(session, selected, &t).hover(move |st| st.bg(t.bg).text_color(t.fg));
    // Der Rand steht immer, nur seine Farbe wechselt: Inhalt springt bei der Auswahl nicht.
    let edge = if selected { t.fg } else { gpui::transparent_black() };
    if selected {
        face = face.bg(t.bg);
    }
    face = outline(face, edge);
    let bar = selected.then(|| rgb(group.color).into());
    row_frame(("session", g * 100 + s), SESSION_ROW, bar)
        .on_click(cx.listener(move |this, _, window, cx| this.select(g, s, window, cx)))
        .child(face)
}

fn group_block(
    g: usize,
    group: &Group,
    selected: (usize, usize),
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let mut block = div().flex().flex_col().flex_none().child(group_header(g, group, selected.0 == g, cx));
    if group.collapsed {
        return block;
    }
    for s in 0..group.sessions.len() {
        block = block.child(session_row(g, s, group, selected == (g, s), cx));
    }
    block
}

pub fn sidebar(groups: &[Group], selected: (usize, usize), cx: &mut Context<Shell>) -> impl IntoElement + use<> {
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
        col = col.child(group_block(g, group, selected, cx));
    }
    div().flex().flex_none().h_full().bg(t.panel).child(col).child(v_seam(&t))
}
