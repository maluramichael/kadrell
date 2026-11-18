//! Pille wie die Tastenkappen der Mac-App: Haarlinie, 1 px harter Schatten, Radius 5.

use gpui::{
    App, BoxShadow, ClickEvent, ElementId, Hsla, IntoElement, RenderOnce, SharedString, Window, div,
    hsla, point, prelude::*, px, rgb,
};

use super::theme;

#[derive(Clone, Copy, PartialEq)]
pub enum PillKind {
    Normal,
    Active,
    Green,
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

fn key_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: hsla(0., 0., 0., 0.18),
        offset: point(px(0.), px(1.)),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: false,
    }]
}

impl RenderOnce for Pill {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = theme(cx);
        let (bg, fg) = match self.kind {
            PillKind::Normal => (t.surface, t.fg),
            PillKind::Active => (t.accent, t.pill_text()),
            PillKind::Green => (t.idle, t.pill_text()),
            PillKind::Dark => (rgb(0x181825).into(), rgb(0xcdd6f4).into()),
        };
        let hover = bg.opacity(0.85);
        let mut el = div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(px(5.))
            .h(px(20.))
            .px(px(8.))
            .rounded(px(5.))
            .border_1()
            .border_color(t.line)
            .bg(bg)
            .text_color(fg)
            .text_size(px(11.))
            .whitespace_nowrap()
            .shadow(key_shadow())
            .cursor_pointer()
            .hover(move |s| s.bg(hover));
        if let Some(c) = self.dot {
            el = el.child(div().size(px(7.)).rounded_full().bg(c));
        }
        if let Some(f) = self.on_click {
            el = el.on_click(move |e, w, c| f(e, w, c));
        }
        el.child(self.label)
    }
}
