//! Linke Seitenleiste: Baum aus Gruppen und Sessions.

use gpui::{
    Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, px,
    rgb,
};

use super::model::{Group, Session};
use super::theme;
use crate::Shell;

fn group_header(g: usize, group: &Group, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let t = theme(cx);
    let chevron = if group.collapsed { "▸" } else { "▾" };
    div()
        .id(("group", g))
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(24.))
        .px(px(6.))
        .rounded(px(6.))
        .cursor_pointer()
        .hover(move |s| s.bg(t.surface.opacity(0.6)))
        .on_click(cx.listener(move |this, _, _, cx| this.toggle_group(g, cx)))
        .child(div().w(px(10.)).text_color(t.muted).child(chevron))
        .child(div().size(px(9.)).rounded(px(2.)).bg(rgb(group.color)))
        .child(div().text_color(t.fg).child(group.name.to_uppercase()))
        .child(div().text_color(t.muted).child(format!("· {}", group.sessions.len())))
}

fn new_badge(cx: &Context<Shell>) -> impl IntoElement + use<> {
    let t = theme(cx);
    div()
        .px(px(5.))
        .rounded(px(4.))
        .bg(t.accent)
        .text_color(t.pill_text())
        .text_size(px(9.))
        .child("neu")
}

fn session_row(
    g: usize,
    s: usize,
    session: &Session,
    selected: bool,
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let t = theme(cx);
    let clear = gpui::transparent_black();
    let (border, bg) = if selected { (t.accent, t.surface) } else { (clear, clear) };
    let title = div()
        .flex_1()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(t.fg)
        .child(session.title);
    let mut row = div()
        .id(("session", g * 100 + s))
        .flex()
        .items_center()
        .gap(px(7.))
        .h(px(26.))
        .ml(px(14.))
        .px(px(7.))
        .rounded(px(6.))
        .border_1()
        .border_color(border)
        .bg(bg)
        .cursor_pointer()
        .hover(move |st| st.bg(t.surface.opacity(0.6)))
        .on_click(cx.listener(move |this, _, window, cx| this.select(g, s, window, cx)))
        .child(div().size(px(8.)).rounded_full().bg(session.status.color(&t)))
        .child(title);
    if session.is_new {
        row = row.child(new_badge(cx));
    }
    row.child(div().text_color(t.muted).child(session.age))
}

fn group_block(
    g: usize,
    group: &Group,
    selected: (usize, usize),
    cx: &mut Context<Shell>,
) -> impl IntoElement + use<> {
    let mut block = div().flex().flex_col().gap(px(2.)).child(group_header(g, group, cx));
    if group.collapsed {
        return block;
    }
    for (s, session) in group.sessions.iter().enumerate() {
        block = block.child(session_row(g, s, session, selected == (g, s), cx));
    }
    block
}

pub fn sidebar(groups: &[Group], selected: (usize, usize), cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let t = theme(cx);
    let mut col = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(260.))
        .gap(px(8.))
        .p(px(8.))
        .bg(t.panel)
        .border_r_1()
        .border_color(t.line)
        .text_size(px(12.));
    for (g, group) in groups.iter().enumerate() {
        col = col.child(group_block(g, group, selected, cx));
    }
    col
}
