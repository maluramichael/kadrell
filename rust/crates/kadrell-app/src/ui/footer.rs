//! Fußzeile: Fuge oben, Hinweise rechtsbündig in gedämpfter Schrift.

use gpui::{App, IntoElement, ParentElement, Styled, div, px};

use super::chrome::h_seam;
use super::theme;

pub fn footer(cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    div().flex().flex_col().flex_none().bg(t.panel).child(h_seam(&t)).child(
        div()
            .flex()
            .items_center()
            .justify_end()
            .h(px(22.))
            .px(px(12.))
            .text_size(px(11.))
            .text_color(t.muted)
            .child("GPUI 0.3.13 · alacritty_terminal 0.26 · ⌥T Theme"),
    )
}
