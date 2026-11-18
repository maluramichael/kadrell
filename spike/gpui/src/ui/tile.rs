//! Rechte Kachel: Kopfzeile (Status, Titel, Zweig, Gruppe, Alter) und Terminal der gewählten Session.

use gpui::{App, FontWeight, IntoElement, ParentElement, Styled, Window, div, px, rgb};

use super::chrome::h_seam;
use super::model::{Group, Session};
use super::theme;
use crate::theme::Theme;

fn group_tag(group: &Group, t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .child(div().size(px(6.)).rounded(px(1.5)).bg(rgb(group.color)))
        .child(div().text_size(px(10.5)).font_weight(FontWeight::BOLD).text_color(t.sub).child(group.name.to_uppercase()))
}

fn header(group: &Group, session: &Session, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .h(px(26.))
        .px(px(9.))
        .child(div().size(px(8.)).flex_none().rounded_full().bg(session.status.color(&t)))
        .child(
            div()
                .flex_none()
                .text_size(px(11.5))
                .font_weight(FontWeight::BOLD)
                .text_color(t.fg)
                .child(session.title),
        )
        .child(div().flex_none().text_size(px(10.5)).text_color(t.muted).child(session.branch))
        .child(group_tag(group, &t))
        .child(div().flex_1())
        .child(div().flex_none().text_size(px(10.5)).text_color(t.muted).child(session.age))
}

pub fn tile(group: &Group, session: &Session, window: &Window, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let focused = session.pane.as_ref().is_some_and(|p| p.read(cx).focus.is_focused(window));
    let border = if focused { t.accent } else { t.line };
    let mut body = div().flex_1().min_h_0().overflow_hidden().bg(t.bg);
    if let Some(pane) = &session.pane {
        body = body.child(pane.clone());
    }
    div()
        .flex()
        .flex_col()
        .size_full()
        .rounded(px(6.))
        .border_2()
        .border_color(border)
        .bg(t.surface)
        .overflow_hidden()
        .child(header(group, session, cx))
        .child(h_seam(&t))
        .child(body)
}
