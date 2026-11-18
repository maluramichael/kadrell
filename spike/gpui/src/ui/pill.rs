//! Pille wie die Tastenkappen der Mac-App: Kante in `key_edge`, Kappe darin, unten 2 px Dicke, Radius 5.

use gpui::{
    App, ClickEvent, ElementId, FontWeight, Hsla, IntoElement, RenderOnce, SharedString, Window, div, prelude::*,
    px,
};

use super::chrome::{key_edge, screen_face, screen_ink};
use super::theme;

#[derive(Clone, Copy, PartialEq)]
pub enum PillKind {
    Normal,
    Active,
    Green,
    /// Schwarzes Display (Laufanzeige), kein Knopf.
    Dark,
}

type ClickFn = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Pill {
    id: ElementId,
    label: SharedString,
    kind: PillKind,
    dot: Option<Hsla>,
    on_click: Option<ClickFn>,
}

impl Pill {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self { id: id.into(), label: label.into(), kind: PillKind::Normal, dot: None, on_click: None }
    }

    pub fn kind(mut self, kind: PillKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn dot(mut self, color: Hsla) -> Self {
        self.dot = Some(color);
        self
    }

    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Pill {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        if self.kind == PillKind::Dark {
            return display(self, screen_face(&t)).into_any_element();
        }
        let (face, ink) = match self.kind {
            PillKind::Active => (t.accent, t.pill_text()),
            PillKind::Green => (t.idle, t.pill_text()),
            _ => (t.surface, t.fg),
        };
        key(self, face, ink, cx).into_any_element()
    }
}

fn dot_el(color: Hsla) -> impl IntoElement {
    div().size(px(7.)).flex_none().rounded_full().bg(color)
}

/// Taste: Außenmaß 20 px, Kante 1 px, unten 3 px (Kante plus 2 px Tiefe).
fn key(pill: Pill, face: Hsla, ink: Hsla, cx: &App) -> impl IntoElement {
    let t = theme(cx);
    let mut cap = div()
        .flex()
        .items_center()
        .gap(px(5.))
        .h(px(16.))
        .px(px(8.))
        .rounded(px(4.))
        .bg(face)
        .text_color(ink)
        .text_size(px(10.))
        .font_weight(FontWeight::BOLD)
        .whitespace_nowrap();
    if let Some(c) = pill.dot {
        cap = cap.child(dot_el(c));
    }
    let mut outer = div()
        .id(pill.id)
        .flex_none()
        .pt(px(1.))
        .px(px(1.))
        .pb(px(3.))
        .rounded(px(5.))
        .bg(key_edge(&t))
        .cursor_pointer()
        .active(|s| s.pt(px(2.)).pb(px(2.)));
    if let Some(f) = pill.on_click {
        outer = outer.on_click(move |e, w, c| f(e, w, c));
    }
    outer.child(cap.child(pill.label))
}

/// Display: dunkle Fläche mit heller Schrift, in allen Schemata gleich.
fn display(pill: Pill, face: Hsla) -> impl IntoElement {
    let mut el = div()
        .id(pill.id)
        .flex_none()
        .flex()
        .items_center()
        .gap(px(7.))
        .h(px(20.))
        .px(px(10.))
        .rounded(px(5.))
        .bg(face)
        .text_color(screen_ink())
        .text_size(px(10.5))
        .font_weight(FontWeight::BOLD)
        .whitespace_nowrap();
    if let Some(c) = pill.dot {
        el = el.child(dot_el(c));
    }
    el.child(pill.label)
}
