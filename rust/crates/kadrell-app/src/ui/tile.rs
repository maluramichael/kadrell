//! Rechte Kachel: Kopfzeile (Status, Titel, Zweig, Gruppe, Alter) und Terminal der gewählten Session.
//! Mit einer einzigen sichtbaren Kachel füllt sie die Arbeitsfläche randlos (`Flat`), erst mehrere Kacheln
//! bekommen Abstand, Rahmen und Fokuskante (`Bordered`), wie in der Mac-App. Maße folgen der UI-Größe, die Flächen der
//! Deckkraft (Hintergrundbild scheint durch), der Rahmen blendet bei Fokuswechsel in 120 ms über (`CellView`).

use std::time::Duration;

use gpui::{Animation, AnimationExt, AnyElement, App, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Role, StatefulInteractiveElement, Styled, div, px};

use super::chrome::h_seam;
use super::model::{Group, Session};
use super::theme;
use crate::appearance::{Appearance, a11y, breathing};
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

fn group_tag(group: &Group, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(a.s(5.)))
        .child(div().size(px(a.s(6.))).rounded(px(a.s(1.5))).bg(t.group(group.color)))
        .child(div().text_size(px(a.s(10.5))).font_weight(FontWeight::BOLD).text_color(t.sub).child(group.name.to_uppercase()))
}

/// Statuspunkt der Kopfzeile: arbeitende Sessions atmen statt zu stehen.
fn dot(session: &Session, t: &Theme, a: &Appearance) -> AnyElement {
    let color = session.status.color(t);
    if session.status == super::model::Status::Running {
        return div().flex_none().child(breathing::breathing_dot(color, a.s(8.), a.s(breathing::GLOW_RADIUS))).into_any_element();
    }
    div().size(px(a.s(8.))).flex_none().rounded_full().bg(color).into_any_element()
}

fn header(group: &Group, session: &Session, a: &Appearance, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let mut row = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(a.s(8.)))
        .h(px(a.s(HEADER_HEIGHT)))
        .px(px(a.s(9.)))
        .bg(a.panel_fill(t.surface))
        .child(dot(session, &t, a))
        .child(div().flex_none().text_size(px(a.s(11.5))).font_weight(FontWeight::BOLD).text_color(t.fg).child(session.title.clone()));
    if let Some(branch) = &session.branch {
        row = row.child(div().flex_none().text_size(px(a.s(10.5))).text_color(t.muted).child(branch.clone()));
    }
    row.child(group_tag(group, &t, a))
        .child(div().flex_1())
        .child(div().flex_none().text_size(px(a.s(10.5))).text_color(t.muted).child(session.age.clone()))
}

/// Rahmenfarbe: Fokus in Akzentfarbe, sonst Linienfarbe.
fn border_color(focused: bool, t: &Theme) -> gpui::Hsla {
    if focused { t.accent } else { t.line }
}

/// Rahmen der Kachel. Eine reine Titelzeile (Stack) trägt nur 1 px, damit Kopf und Rahmen in die Zeilenhöhe passen.
fn frame(style: TileStyle, focused: bool, body: bool, t: &Theme, a: &Appearance) -> Div {
    let base = div().flex().flex_col().size_full().overflow_hidden();
    match style {
        TileStyle::Flat => base,
        TileStyle::Bordered => {
            let base = base.rounded(px(a.s(6.))).border_color(border_color(focused, t));
            if body { base.border_2() } else { base.border_1() }
        }
    }
}

/// Eine Kachel: Kopfzeile und, wenn `body`, das Terminal. `focused` ist die Tastaturkachel.
pub fn tile(group: &Group, session: &Session, style: TileStyle, focused: bool, body: bool, cx: &App) -> AnyElement {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let frame = frame(style, focused, body, &t, &a).child(header(group, session, &a, cx));
    let content = if body {
        let mut area = div().flex_1().min_h_0().overflow_hidden();
        // Das Terminal malt seinen Grund selbst (mit der Deckkraft), ein zweiter Grund darunter deckte doppelt.
        match &session.pane {
            Some(pane) => area = area.child(pane.clone()),
            None => area = area.bg(a.panel_fill(t.bg)),
        }
        frame.child(h_seam(&t)).child(area)
    } else {
        frame
    };
    let spoken = a11y::spoken(session.status, session.ended, false);
    let label = a11y::tile_label(&session.title, &spoken, &group.name, session.branch.as_deref());
    let node = content.id(gpui::ElementId::Name(format!("tile-{}", session.key).into())).role(Role::Group).aria_label(label);
    if style == TileStyle::Flat {
        return node.into_any_element();
    }
    // Wechselt der Fokus, ändert sich die Kennung und der Rahmen blendet von der alten zur neuen Farbe über.
    let (from, to) = (border_color(!focused, &t), border_color(focused, &t));
    node.with_animation(
        gpui::ElementId::Name(format!("tile-border-{}-{focused}", session.key).into()),
        Animation::new(Duration::from_millis(120)),
        move |el, delta| el.border_color(breathing::fade(from, to, Duration::from_secs_f32(delta * 0.12))),
    )
    .into_any_element()
}
