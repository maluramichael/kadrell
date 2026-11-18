//! Rechte Kachel: Kopfzeile (Status, Titel, Zweig, Gruppe, Alter) und Terminal der gewählten Session.
//! Mit einer einzigen sichtbaren Kachel füllt sie die Arbeitsfläche randlos (`Flat`), erst mehrere Kacheln
//! bekommen Abstand, Rahmen und Fokuskante (`Bordered`), wie in der Mac-App.

use gpui::{AnyElement, App, AppContext, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, div, px};

use super::chrome::h_seam;
use super::groups::drag::DragTile;
use super::groups::rows::DragGhost;
use super::model::{Group, Session};
use super::theme;
use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq)]
pub enum TileStyle {
    /// Einzelne Kachel: füllt die Fläche, flacher Kopf, kein Rahmen.
    Flat,
    /// Mehrere Kacheln: 8 px Abstand, 2 px Fokuskante in Akzentfarbe, runde Ecken.
    Bordered,
}

impl TileStyle {
    pub fn for_count(visible: usize) -> Self {
        if visible > 1 { TileStyle::Bordered } else { TileStyle::Flat }
    }
}

const HEADER_HEIGHT: f32 = 24.;

fn group_tag(group: &Group, t: &Theme) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .child(div().size(px(6.)).rounded(px(1.5)).bg(t.group(group.color)))
        .child(div().text_size(px(10.5)).font_weight(FontWeight::BOLD).text_color(t.sub).child(group.name.to_uppercase()))
}

fn header(group: &Group, session: &Session, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let mut row = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(8.))
        .h(px(HEADER_HEIGHT))
        .px(px(9.))
        .bg(t.surface)
        .child(div().size(px(8.)).flex_none().rounded_full().bg(session.status.color(&t)))
        .child(
            div()
                .flex_none()
                .text_size(px(11.5))
                .font_weight(FontWeight::BOLD)
                .text_color(t.fg)
                .child(session.title.clone()),
        );
    if let Some(branch) = &session.branch {
        row = row.child(div().flex_none().text_size(px(10.5)).text_color(t.muted).child(branch.clone()));
    }
    let drag = DragTile { key: session.key.clone(), label: session.title.clone() };
    row.child(group_tag(group, &t))
        .child(div().flex_1())
        .child(div().flex_none().text_size(px(10.5)).text_color(t.muted).child(session.age.clone()))
        // Ziehen an der Titelzeile sortiert um (Ablage in `layout::placed`).
        .id(SharedString::from(format!("tile-head-{}", session.key)))
        .cursor_grab()
        .on_drag(drag, |d, _, _, cx| cx.new(|_| DragGhost { label: d.label.clone() }))
}

/// Rahmen der Kachel: Fokus in Akzentfarbe, sonst Linienfarbe. Eine reine Titelzeile (Stack) trägt nur 1 px,
/// damit Kopf und Rahmen in die Zeilenhöhe passen.
fn frame(style: TileStyle, focused: bool, body: bool, t: &Theme) -> Div {
    let base = div().flex().flex_col().size_full().overflow_hidden();
    match style {
        TileStyle::Flat => base,
        TileStyle::Bordered => {
            let border = if focused { t.accent } else { t.line };
            let base = base.rounded(px(6.)).border_color(border);
            if body { base.border_2() } else { base.border_1() }
        }
    }
}

/// Eine Kachel: Kopfzeile und, wenn `body`, das Terminal. `focused` ist die Tastaturkachel.
pub fn tile(group: &Group, session: &Session, style: TileStyle, focused: bool, body: bool, cx: &App) -> AnyElement {
    let t = theme(cx);
    let frame = frame(style, focused, body, &t).child(header(group, session, cx));
    if !body {
        return frame.into_any_element();
    }
    let mut area = div().flex_1().min_h_0().overflow_hidden().bg(t.bg);
    if let Some(pane) = &session.pane {
        area = area.child(pane.clone());
    }
    frame.child(h_seam(&t)).child(area).into_any_element()
}
