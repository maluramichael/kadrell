//! Obere Leiste: Pillen links, Nutzung und Status rechts.

use gpui::{App, IntoElement, ParentElement, Styled, div, px};

use super::model::Status;
use super::pill::{Pill, PillKind};
use super::theme;

const LEFT: [&str; 8] = ["▦", "‹", "AUTO COL", "›", "AUTO", "SYNC", "GRP", "A-Z"];

fn left_pills() -> impl IntoElement + use<> {
    let pills = LEFT.iter().enumerate().map(|(i, l)| {
        let kind = if *l == "A-Z" { PillKind::Active } else { PillKind::Normal };
        Pill::new(("bar", i), *l).kind(kind)
    });
    div().flex().items_center().gap(px(6.)).children(pills)
}

fn right_side(clock: &str, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let running = Pill::new("running", format!("1 RUNNING  {clock}"))
        .kind(PillKind::Dark)
        .dot(Status::Running.color(&t));
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().text_color(t.sub).child("7d 14%/1%"))
        .child(div().text_color(t.sub).child("fable 0%"))
        .child(Pill::new("konto", "konto 1"))
        .child(Pill::new("detach", "detach 6").kind(PillKind::Green))
        .child(running)
}

pub fn bar(clock: &str, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .h(px(30.))
        .px(px(8.))
        .bg(t.panel)
        .border_b_1()
        .border_color(t.line)
        .text_size(px(11.))
        .child(left_pills())
        .child(right_side(clock, cx))
}
