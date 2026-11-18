//! Rechte Kachel: Kopfzeile und Terminal der gewählten Session.

use gpui::{App, IntoElement, ParentElement, Styled, Window, div, px, rgb};

use super::model::{Group, Session};
use super::theme;

fn header(group: &Group, session: &Session, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .h(px(26.))
        .px(px(10.))
        .child(div().size(px(8.)).rounded_full().bg(session.status.color(&t)))
        .child(div().font_weight(gpui::FontWeight::BOLD).text_color(rgb(group.color)).child(group.name))
        .child(div().flex_1().text_color(t.accent).child(session.title))
        .child(div().text_color(t.muted).child(session.age))
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
        .child(body)
}
