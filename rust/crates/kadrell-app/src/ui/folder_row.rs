//! Zeile einer Ordnerliste: Name, Pfad darunter, rechts ein oder zwei gedämpfte Zeilen. Gemeinsam für „Neue Session“
//! und den Schnellstart, damit beide Listen gleich aussehen und gleich anklickbar sind.

use gpui::{
    App, ClickEvent, ElementId, InteractiveElement, ParentElement, StatefulInteractiveElement, Styled,
    Window, div, px,
};

use crate::theme::Theme;
use crate::ui::instrument::selected_row;

/// Höhe einer Zeile in Pixeln (`rowHeight` in `NewSessionSheet.swift` und `QuickStartView.swift`).
pub const ROW_HEIGHT: f32 = 46.;

/// Was eine Zeile zeigt.
pub struct FolderRow {
    pub name: String,
    /// Pfad mit `~`, darf lang sein (wird abgeschnitten).
    pub path: String,
    /// Rechts, von oben nach unten.
    pub right: Vec<String>,
}

pub fn folder_row(
    id: impl Into<ElementId>,
    row: FolderRow,
    on: bool,
    t: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    let right = div()
        .flex()
        .flex_col()
        .items_end()
        .flex_none()
        .gap(px(2.))
        .text_size(px(11.))
        .text_color(t.muted)
        .children(row.right.into_iter().map(|line| div().child(line)));
    // Instrument-Zeile: Fläche `bg`, 1,5-pt-Umriss in `fg`, kein Akzentring (`instrumentRow(selected:)`).
    let inner = selected_row(div().flex().items_center().gap(px(10.)).size_full().px(px(10.)), on, t)
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(2.))
                .child(div().text_size(px(13.)).text_color(t.fg).overflow_hidden().whitespace_nowrap().child(row.name))
                .child(div().text_size(px(11.)).text_color(t.muted).overflow_hidden().whitespace_nowrap().text_ellipsis().child(row.path)),
        )
        .child(right);
    div().id(id).flex_none().h(px(ROW_HEIGHT)).px(px(6.)).py(px(1.)).cursor_pointer().on_click(on_click).child(inner)
}
