//! Obere Leiste und Titelleiste in einem: Tasten links, Nutzung und Status rechts, freie Fläche zieht das Fenster.

use gpui::{App, InteractiveElement, IntoElement, ParentElement, Styled, Window, WindowControlArea, div, px};

use super::chrome::{BAR_HEIGHT, h_seam, window_controls};
use super::model::Status;
use super::pill::{Pill, PillKind};
use super::theme;

const LEFT: [&str; 8] = ["▦", "‹", "AUTO COL", "›", "AUTO", "SYNC", "GRP", "A-Z"];

/// Links neben den Ampelknöpfen von macOS (inset) oder am Fensterrand.
const LEFT_INSET: f32 = if cfg!(target_os = "macos") { 78. } else { 8. };

fn left_pills() -> impl IntoElement + use<> {
    let pills = LEFT.iter().enumerate().map(|(i, l)| {
        let kind = if *l == "A-Z" { PillKind::Active } else { PillKind::Normal };
        Pill::new(("bar", i), *l).kind(kind)
    });
    div().flex().flex_none().items_center().gap(px(4.)).children(pills)
}

fn usage_text(text: &'static str, cx: &App) -> impl IntoElement + use<> {
    div().flex_none().text_size(px(11.5)).text_color(theme(cx).muted).child(text)
}

fn right_side(clock: &str, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let running = Pill::new("running", format!("1 RUNNING  {clock}"))
        .kind(PillKind::Dark)
        .dot(Status::Running.color(&t));
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.))
        .pr(px(if cfg!(target_os = "windows") { 6. } else { 8. }))
        .child(usage_text("7d 14%/1%", cx))
        .child(usage_text("fable 0%", cx))
        .child(Pill::new("konto", "konto 1"))
        .child(Pill::new("detach", "detach 6").kind(PillKind::Green))
        .child(running)
}

/// Die freie Fläche zwischen links und rechts: unter Windows Titelleisten-Zugriff zum Verschieben.
fn drag_area() -> impl IntoElement + use<> {
    div().flex_1().h_full().min_w(px(24.)).window_control_area(WindowControlArea::Drag)
}

pub fn bar(clock: &str, window: &Window, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let mut row = div()
        .flex()
        .items_center()
        .h(px(BAR_HEIGHT - 2.))
        .w_full()
        .pl(px(LEFT_INSET))
        .child(left_pills())
        .child(drag_area())
        .child(right_side(clock, cx));
    if cfg!(target_os = "windows") {
        row = row.child(window_controls(window, cx));
    }
    div().flex().flex_col().flex_none().bg(t.panel).child(row).child(h_seam(&t))
}
