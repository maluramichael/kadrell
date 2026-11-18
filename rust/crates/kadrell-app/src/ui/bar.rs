//! Obere Leiste (Tasten links, Nutzung und Status rechts) und unter Windows die eigene Titelzeile darüber
//! (Menüs links, Titel in der Mitte, Fensterknöpfe rechts). Auf macOS liefert das System die Titelleiste.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Entity, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, Window, WindowControlArea, div,
    px,
};

use super::icons;
use super::layout::LayoutMode;
use super::chrome::{BAR_HEIGHT, h_seam, window_controls};
use super::menubar::{MenuBar, TITLE_HEIGHT};
use super::pill::{Pill, PillKind};
use super::usage_bar::{self, BarCtl};
use super::theme;
use kadrell_core::SidebarSort;

const LEFT: [&str; 9] = ["▦", "‹", "AUTO COL", "›", "AUTO", "SYNC", "GRP", "A-Z", "STATUS"];

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
    /// Gruppierung des Baums (GRP) und seine Sortierung (A-Z, STATUS).
    pub grouped: bool,
    pub sort: SidebarSort,
    pub toggle_grouped: Handler,
    pub toggle_alpha: Handler,
    pub toggle_status: Handler,
    /// Zähler rechts: laufende, getrennte (ohne Prozess) und wartende Sessions.
    pub running: usize,
    pub detached: usize,
    pub waiting: usize,
    /// Konten, Nutzung und Status-Display der rechten Seite (`usage_bar`).
    pub usage: Option<BarCtl>,
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
        "GRP" => with_click(toggled(pill, ctl.grouped), &ctl.toggle_grouped),
        "A-Z" => with_click(toggled(pill, ctl.sort == SidebarSort::Alpha), &ctl.toggle_alpha),
        "STATUS" => with_click(toggled(pill, ctl.sort == SidebarSort::Status), &ctl.toggle_status),
        _ => pill,
    }
}

fn toggled(pill: Pill, on: bool) -> Pill {
    if on { pill.kind(PillKind::Active) } else { pill }
}

fn left_pills(ink: Hsla, ctl: &LayoutCtl) -> impl IntoElement + use<> {
    let pills = LEFT.iter().enumerate().map(|(i, l)| left_pill(i, l, ink, ctl));
    div().flex().flex_none().items_center().gap(px(4.)).children(pills)
}

/// Geschätzte Breite der linken Tasten samt Rand (Schrift 10 px fett, dicktengleich: 6 px je Zeichen, Kappe 18 px mehr).
fn left_width(ctl: &LayoutCtl) -> f32 {
    let key = |label: &str| match label {
        "▦" => 26.,
        "‹" | "›" => 22.,
        "AUTO COL" => ctl.cols_label.chars().count() as f32 * 6. + 18.,
        other => other.chars().count() as f32 * 6. + 18.,
    };
    LEFT_INSET + LEFT.iter().map(|l| key(l) + 4.).sum::<f32>()
}

/// Rechts: Konto, „N trennen“, „N warten“, Nutzung und das Status-Display (`usage_bar`). Ohne Anbindung (nur Tests)
/// bleibt die Seite leer.
fn right_side(clock: &str, ctl: &LayoutCtl, cx: &App) -> AnyElement {
    let Some(usage) = &ctl.usage else { return div().into_any_element() };
    let b = BarCtl { left: left_width(ctl), ..usage.clone() };
    usage_bar::right_side(clock, &b, &theme(cx)).into_any_element()
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
        .child(right_side(clock, ctl, cx));
    div().flex().flex_col().flex_none().bg(t.panel).child(row).child(h_seam(&t))
}
