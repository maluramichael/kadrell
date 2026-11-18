//! Pille wie die Tastenkappen der Mac-App: Kante in `key_edge`, Kappe darin, unten 2 px Dicke, Radius 5.

use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla, IntoElement, RenderOnce, SharedString, Window, div, prelude::*,
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
    icon: Option<AnyElement>,
    min_w: Option<f32>,
    on_click: Option<ClickFn>,
    fill: Option<Hsla>,
    tip: Option<SharedString>,
}

impl Pill {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self { id: id.into(), label: label.into(), kind: PillKind::Normal, dot: None, icon: None, min_w: None, on_click: None, fill: None, tip: None }
    }

    /// Tooltip beim Daraufzeigen.
    pub fn tooltip_text(mut self, text: impl Into<SharedString>) -> Self {
        self.tip = Some(text.into());
        self
    }

    /// Eigene Kappenfarbe (die Schrift wird passend gewählt), etwa Gelb für „N warten“.
    pub fn fill(mut self, face: Hsla) -> Self {
        self.fill = Some(face);
        self
    }

    pub fn kind(mut self, kind: PillKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn dot(mut self, color: Hsla) -> Self {
        self.dot = Some(color);
        self
    }

    /// Gezeichnetes Symbol statt Text (Pfadsymbole aus `icons`).
    pub fn icon(mut self, icon: impl IntoElement) -> Self {
        self.icon = Some(icon.into_any_element());
        self
    }

    /// Mindestbreite der Kappe in px (die Umschalttaste der Leiste ist 26 breit).
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_w = Some(w);
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
            _ if self.fill.is_some() => self.fill.map_or((t.surface, t.fg), |f| (f, t.pill_text_on(f))),
            PillKind::Active => (t.sub, t.pill_text_on(t.sub)),
            PillKind::Green => (t.idle, t.pill_text_on(t.idle)),
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
        .pt(px(if pill.kind == PillKind::Active { 2. } else { 1. }))
        .px(px(1.))
        .pb(px(if pill.kind == PillKind::Active { 2. } else { 3. }))
        .rounded(px(5.))
        .bg(key_edge(&t))
        .cursor_pointer()
        .active(|s| s.pt(px(2.)).pb(px(2.)));
    if let Some(f) = pill.on_click {
        outer = outer.on_click(move |e, w, c| f(e, w, c));
    }
    if let Some(tip) = pill.tip {
        outer = outer.tooltip(super::tooltip::text(tip));
    }
    if let Some(w) = pill.min_w {
        cap = cap.min_w(px(w - 2.)).justify_center();
    }
    cap = match pill.icon {
        Some(icon) => cap.child(icon),
        None => cap.child(pill.label),
    };
    outer.child(cap)
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
