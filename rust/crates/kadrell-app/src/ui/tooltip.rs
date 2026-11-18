//! Einfacher Text-Tooltip im Stil der Dialoge (Fläche `surface`, kleine Schrift), für die Leiste.

use gpui::{AnyView, App, AppContext, Context, IntoElement, ParentElement, Render, SharedString, Styled, Window, div, px};

use super::theme;

struct TextTip(SharedString);

impl Render for TextTip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        div()
            .max_w(px(420.))
            .px(px(8.))
            .py(px(5.))
            .rounded(px(4.))
            .border_1()
            .border_color(t.line)
            .bg(t.surface)
            .text_color(t.fg)
            .text_size(px(11.))
            .child(self.0.clone())
    }
}

/// Baut den Tooltip zu einem Text, wie `StatefulInteractiveElement::tooltip` ihn erwartet.
pub fn text(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |_, cx| cx.new(|_| TextTip(text.clone())).into()
}
