//! Zeile der Projektliste im Schnellstart (Swift: `QuickStartView.row`): Name (12 pt) über dem Pfad (11 pt, gedämpft, bei
//! Platzmangel vorn abgeschnitten), rechts die Herkunft (11 pt) über der Zeitangabe (10,5 pt). Eigene Zeile statt
//! `folder_row`, weil „Neue Session“ andere Schriftgrößen hat.

use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, ParentElement, StatefulInteractiveElement, Styled, Window, div, prelude::*, px,
};

use crate::theme::Theme;
use crate::ui::folder_row::ROW_HEIGHT;

/// Was eine Zeile zeigt.
pub struct Row {
    pub name: String,
    /// Pfad mit `~`.
    pub path: String,
    pub source: String,
    /// Fehlt, wenn die letzte Nutzung unbekannt ist.
    pub when: Option<String>,
}

pub fn row(id: impl Into<ElementId>, row: Row, on: bool, t: &Theme, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> gpui::Stateful<gpui::Div> {
    let s = crate::ui::overlay::scale();
    let outline = if on { t.fg } else { gpui::transparent_black() };
    let left = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(2. * s))
        .child(div().text_size(px(12. * s)).text_color(t.fg).overflow_hidden().whitespace_nowrap().text_ellipsis().child(row.name))
        .child(div().text_size(px(11. * s)).text_color(t.muted).overflow_hidden().whitespace_nowrap().text_ellipsis_start().child(row.path));
    let right = div()
        .flex()
        .flex_col()
        .items_end()
        .flex_none()
        .gap(px(2. * s))
        .text_color(t.muted)
        .child(div().text_size(px(11. * s)).child(row.source))
        .children(row.when.map(|w| div().text_size(px(10.5 * s)).child(w)));
    let inner = div()
        .flex()
        .items_center()
        .gap(px(10. * s))
        .size_full()
        .px(px(10. * s))
        .rounded(px(6. * s))
        .border_2()
        .border_color(outline)
        .when(on, |d| d.bg(t.bg))
        .child(left)
        .child(right);
    div().id(id).flex_none().h(px(ROW_HEIGHT * s)).px(px(6. * s)).py(px(1. * s)).cursor_pointer().on_click(on_click).child(inner)
}
