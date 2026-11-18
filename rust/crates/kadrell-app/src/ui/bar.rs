//! Obere Leiste (Tasten links, Nutzung und Status rechts) und unter Windows die eigene Titelzeile darüber
//! (Menüs links, Titel in der Mitte, Fensterknöpfe rechts). Auf macOS liefert das System die Titelleiste.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Entity, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, Window, WindowControlArea, div,
    px,
};

use super::icons;
use super::layout::LayoutMode;
use super::chrome::{BAR_HEIGHT, h_seam, window_controls};
use super::menubar::{MenuBar, TITLE_HEIGHT};
use super::model::Status;
use super::pill::{Pill, PillKind};
use super::theme;

const LEFT: [&str; 8] = ["▦", "‹", "AUTO COL", "›", "AUTO", "SYNC", "GRP", "A-Z"];

const LEFT_INSET: f32 = 8.;

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Zustand und Aktionen der Layout-Tasten: Symbol (reihum), ‹ und › (Spaltenzahl im Grid).
#[derive(Clone)]
pub struct LayoutCtl {
    pub mode: LayoutMode,
    pub cols_label: String,
    pub cycle: Handler,
    pub dec: Handler,
    pub inc: Handler,
}

fn with_click(pill: Pill, h: &Handler) -> Pill {
    let h = h.clone();
    pill.on_click(move |e, w, c| h(e, w, c))
}

/// Eine Taste der linken Leiste: Layout-Symbol und Pfeile sind gezeichnet, der Rest Text.
fn left_pill(i: usize, label: &'static str, ink: Hsla, ctl: &LayoutCtl) -> Pill {
    let pill = Pill::new(("bar", i), label);
    match label {
        "▦" => with_click(pill.icon(icons::layout(ctl.mode, ink)).min_width(26.), &ctl.cycle),
        "‹" => with_click(pill.icon(icons::arrow(true, ink)).min_width(22.), &ctl.dec),
        "›" => with_click(pill.icon(icons::arrow(false, ink)).min_width(22.), &ctl.inc),
        "AUTO COL" => Pill::new(("bar", i), ctl.cols_label.clone()),
        "A-Z" => pill.kind(PillKind::Active),
        _ => pill,
    }
}

fn left_pills(ink: Hsla, ctl: &LayoutCtl) -> impl IntoElement + use<> {
    let pills = LEFT.iter().enumerate().map(|(i, l)| left_pill(i, l, ink, ctl));
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

/// Die freie Fläche der Titelzeile mit dem Fenstertitel: zieht das Fenster.
fn title_area(cx: &App) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .h_full()
        .min_w(px(24.))
        .text_color(theme(cx).sub)
        .window_control_area(WindowControlArea::Drag)
        .child("Kadrell")
}

/// Windows: eigene Titelzeile im Kadrell-Stil, getrennt von der Leiste darunter. Sonst `None`.
pub fn titlebar(menubar: &Entity<MenuBar>, window: &Window, cx: &App) -> Option<impl IntoElement + use<>> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let t = theme(cx);
    let row = div()
        .flex()
        .items_center()
        .h(px(TITLE_HEIGHT - 2.))
        .w_full()
        .child(menubar.clone())
        .child(title_area(cx))
        .child(window_controls(window, cx));
    Some(div().flex().flex_col().flex_none().bg(t.bg).child(row).child(h_seam(&t)))
}

/// Die Leiste mit Layout-Tasten, Nutzung und Status, erste Zeile unter der Titelleiste.
pub fn bar(clock: &str, ctl: &LayoutCtl, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let row = div()
        .flex()
        .items_center()
        .h(px(BAR_HEIGHT - 2.))
        .w_full()
        .pl(px(LEFT_INSET))
        .child(left_pills(t.sub, ctl))
        .child(div().flex_1().min_w(px(24.)))
        .child(right_side(clock, cx));
    div().flex().flex_col().flex_none().bg(t.panel).child(row).child(h_seam(&t))
}
