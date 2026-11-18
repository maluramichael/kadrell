//! Pille wie die Tastenkappen der Mac-App: Kante in `key_edge`, Kappe darin, unten 2 px Dicke, Radius 5.

use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, Hsla, IntoElement, RenderOnce, SharedString, Window, div, prelude::*,
    px,
};

use super::chrome::{key_edge, screen_face, screen_ink};
use crate::appearance::Appearance;
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
    disabled: bool,
}

impl Pill {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self { id: id.into(), label: label.into(), kind: PillKind::Normal, dot: None, icon: None, min_w: None, on_click: None, fill: None, tip: None, disabled: false }
    }

    /// Gesperrt: Schrift gedämpft, kein Zeiger, kein Druckzustand, kein Klick (`KeyBody` liest `isEnabled`).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
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

    /// Feste Breite der Kappe samt Kante in px (die Umschalttaste der Leiste ist 26 breit).
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
            return display(self, screen_face(&t), &crate::appearance::current(cx)).into_any_element();
        }
        let (face, ink) = match self.kind {
            _ if self.fill.is_some() => self.fill.map_or((t.surface, t.fg), |f| (f, t.pill_text_on(f))),
            PillKind::Active => (t.sub, t.pill_text_on(t.sub)),
            PillKind::Green => (t.idle, t.pill_text_on(t.idle)),
            _ => (t.surface, t.key_label()),
        };
        let ink = if self.disabled { t.muted } else { ink };
        key(self, face, ink, cx).into_any_element()
    }
}

fn dot_el(color: Hsla, a: &Appearance) -> impl IntoElement {
    div().size(px(a.s(7.))).flex_none().rounded_full().bg(color)
}

/// Taste: Außenmaß 20 px, Kante 1 px, unten 3 px (Kante plus 2 px Tiefe). Text plus 16 px Breite wie `segment` der Mac-Leiste.
fn key(pill: Pill, face: Hsla, ink: Hsla, cx: &App) -> impl IntoElement {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let mut cap = div()
        .flex()
        .items_center()
        .gap(px(a.s(5.)))
        .h(px(a.s(16.)))
        .px(px(a.s(7.)))
        .rounded(px(a.s(4.)))
        .bg(face)
        .text_color(ink)
        .text_size(px(a.s(10.)))
        .font_weight(FontWeight::BOLD)
        .whitespace_nowrap();
    if let Some(c) = pill.dot {
        cap = cap.child(dot_el(c, &a));
    }
    let mut outer = div()
        .id(pill.id)
        .flex_none()
        .pt(px(a.s(if pill.kind == PillKind::Active { 2. } else { 1. })))
        .px(px(a.s(1.)))
        .pb(px(a.s(if pill.kind == PillKind::Active { 2. } else { 3. })))
        .rounded(px(a.s(5.)))
        .bg(key_edge(&t));
    if !pill.disabled {
        outer = outer.cursor_pointer().active(|s| s.pt(px(a.s(2.))).pb(px(a.s(2.))));
    }
    if let (Some(f), false) = (pill.on_click, pill.disabled) {
        outer = outer.on_click(move |e, w, c| f(e, w, c));
    }
    if let Some(tip) = pill.tip {
        outer = outer.tooltip(super::tooltip::text(tip));
    }
    if let Some(w) = pill.min_w {
        // Feste Breite samt Kante (Swift: `keyRect(width:)`), der Inhalt sitzt mittig ohne eigenen Rand.
        cap = cap.w(px(a.s(w - 2.))).px_0().justify_center();
    }
    cap = match pill.icon {
        Some(icon) => cap.child(icon),
        None => cap.child(pill.label),
    };
    outer.child(cap)
}

/// Display: dunkle Fläche mit heller Schrift, in allen Schemata gleich.
fn display(pill: Pill, face: Hsla, a: &Appearance) -> impl IntoElement {
    // Feine helle Haarlinie am oberen Rand, durch den Radius beschnitten (Swift: `Instrument.screen`).
    let hairline = div().absolute().top_0().left_0().right_0().h(px(super::overlay::hairline())).bg(gpui::hsla(0., 0., 1., 0.06));
    let mut el = div()
        .id(pill.id)
        .relative()
        .overflow_hidden()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(a.s(7.)))
        .h(px(a.s(20.)))
        .px(px(a.s(10.)))
        .rounded(px(a.s(5.)))
        .bg(face)
        .text_color(screen_ink())
        .text_size(px(a.s(10.5)))
        .font_weight(FontWeight::BOLD)
        .whitespace_nowrap();
    el = el.child(hairline);
    if let Some(c) = pill.dot {
        el = el.child(dot_el(c, a));
    }
    el.child(pill.label)
}
