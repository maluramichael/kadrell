//! Suchleiste über der Terminalfläche, der Leiste des Systems (NSTextFinder) auf dem Mac nachempfunden: über die
//! volle Breite am oberen Rand, links das Eingabefeld mit Lupe und „Keine Treffer“, daneben Zurück und Weiter, ganz
//! rechts „Fertig“.

use gpui::{App, ClickEvent, Div, ElementId, Hsla, InteractiveElement, ParentElement, Stateful, StatefulInteractiveElement, Styled, Window, div, px};

use crate::theme::TermTheme;

const HEIGHT: f32 = 30.;
const FIELD_HEIGHT: f32 = 22.;
const FIELD_MAX_WIDTH: f32 = 320.;

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Was die Leiste anzeigt.
pub struct Content<'a> {
    pub query: &'a str,
    /// Der Begriff steht, aber nichts passt.
    pub no_hits: bool,
}

/// Die drei Schaltflächen der Leiste.
pub struct Actions {
    pub previous: OnClick,
    pub next: OnClick,
    pub done: OnClick,
}

fn text_button(id: impl Into<ElementId>, label: String, theme: &TermTheme, on_click: OnClick) -> Stateful<Div> {
    let hover: Hsla = Hsla { a: 0.35, ..theme.sub };
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(FIELD_HEIGHT))
        .min_w(px(FIELD_HEIGHT))
        .px(px(6.))
        .rounded(px(5.))
        .text_color(theme.fg)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .on_click(move |ev, window, cx| {
            cx.stop_propagation();
            on_click(ev, window, cx);
        })
        .child(label)
}

fn field(content: &Content, theme: &TermTheme) -> Div {
    let text = if content.query.is_empty() {
        div().text_color(theme.sub).child(kadrell_i18n::trd!("Suchen"))
    } else {
        div().text_color(theme.fg).child(format!("{}▏", content.query))
    };
    let status = content.no_hits.then(|| div().flex_none().text_color(theme.sub).child(kadrell_i18n::trd!("Keine Treffer")));
    div()
        .flex()
        .flex_1()
        .items_center()
        .gap(px(6.))
        .max_w(px(FIELD_MAX_WIDTH))
        .h(px(FIELD_HEIGHT))
        .px(px(8.))
        .rounded(px(5.))
        .bg(theme.bg)
        .child(div().flex_none().text_color(theme.sub).child("⌕"))
        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(text))
        .children(status)
}

/// Die Leiste, absolut am oberen Rand des Terminals.
pub fn view(content: &Content, actions: Actions, theme: &TermTheme) -> Div {
    let Actions { previous, next, done } = actions;
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(HEIGHT))
        .flex()
        .items_center()
        .gap(px(6.))
        .px(px(8.))
        .bg(theme.line)
        .text_color(theme.fg)
        .text_size(px(12.))
        .child(field(content, theme))
        .child(text_button("search-previous", "‹".into(), theme, previous))
        .child(text_button("search-next", "›".into(), theme, next))
        .child(div().flex_1())
        .child(text_button("search-done", kadrell_i18n::trd!("Fertig").to_string(), theme, done))
}
