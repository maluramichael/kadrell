//! Rechte Kachel: Kopfzeile (Status, Titel, Zweig, Gruppe, Alter) und Terminal der gewählten Session.
//! Mit einer einzigen sichtbaren Kachel füllt sie die Arbeitsfläche randlos (`Flat`), erst mehrere Kacheln
//! bekommen Abstand, Rahmen und Fokuskante (`Bordered`), wie in der Mac-App. Maße folgen der UI-Größe, die Flächen der
//! Deckkraft (Hintergrundbild scheint durch), der Rahmen blendet bei Fokuswechsel in 120 ms über (`CellView`).

use std::time::Duration;

use kadrell_i18n::trd;
use gpui::{Animation, AnimationExt, AnyElement, App, AppContext, Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, div, px};

use super::chrome::h_seam;
use super::groups::drag::DragTile;
use super::groups::rows::DragGhost;
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

/// Was an einer Kachel zu sehen ist, über Stil und Inhalt hinaus.
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Marks {
    /// Die Tastaturkachel.
    pub active: bool,
    /// Fokuskante in Akzentfarbe: die Tastaturkachel, im Sync-Modus alle (alle bekommen die Eingabe).
    pub ring: bool,
    /// Badge „Z“: gezoomt, obwohl mehrere Sessions offen sind (wie in tmux).
    pub zoomed: bool,
    /// Vorschau aus dem Baum, ohne Tastatur.
    pub preview: bool,
}

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

/// Badge „Z“ in der Titelzeile einer gezoomten Kachel (`drawZoomBadge`).
fn zoom_badge(t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(a.s(16.)))
        .mr(px(a.s(8.)))
        .bg(t.waiting)
        .text_color(t.pill_text_on(t.waiting))
        .text_size(px(a.s(11.)))
        .font_weight(FontWeight::BOLD)
        .child("Z")
}

fn header(group: &Group, session: &Session, zoomed: bool, a: &Appearance, cx: &App) -> impl IntoElement + use<> {
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
    row = row.child(group_tag(group, &t, a)).child(div().flex_1());
    if zoomed {
        row = row.child(zoom_badge(&t, a));
    }
    let drag = DragTile { key: session.key.clone(), label: session.title.clone() };
    row.child(div().flex_none().text_size(px(a.s(10.5))).text_color(t.muted).child(session.age.clone()))
        // Ziehen an der Titelzeile sortiert um (Ablage in `layout::placed`).
        .id(SharedString::from(format!("tile-head-{}", session.key)))
        .cursor_grab()
        .on_drag(drag, |d, _, _, cx| cx.new(|_| DragGhost { label: d.label.clone() }))
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

/// Hinweis in einer Kachel, deren Terminal in einem anderen Fenster hängt.
fn elsewhere_hint(t: &Theme, a: &Appearance) -> Div {
    div().size_full().flex().items_center().justify_center().text_size(px(a.s(11.))).text_color(t.muted).child(kadrell_i18n::trd!("IN ANDEREM FENSTER · KLICK HOLT HIERHER"))
}

/// Hinweis mitten im Körper einer Kachel ohne Terminal: die Vorschau einer nie gestarteten Session.
fn preview_note(t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(a.s(11.)))
        .font_weight(FontWeight::BOLD)
        .text_color(t.muted)
        .child(trd!("VORSCHAU · NICHT GESTARTET · ⏎ ODER KLICK STARTET"))
}

/// Eine Kachel: Kopfzeile und, wenn `body`, das Terminal. `marks` sagt, was an der Kachel zu sehen ist, `elsewhere` sagt, dass das
/// Terminal in einem anderen Fenster hängt (statt seiner steht ein Hinweis).
pub fn tile(group: &Group, session: &Session, style: TileStyle, marks: Marks, body: bool, elsewhere: bool, cx: &App) -> AnyElement {
    let focused = marks.ring;
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let frame = frame(style, focused, body, &t, &a).child(header(group, session, marks.zoomed, &a, cx));
    let content = if body {
        let mut area = div().flex_1().min_h_0().overflow_hidden();
        // Das Terminal malt seinen Grund selbst (mit der Deckkraft), ein zweiter Grund darunter deckte doppelt.
        match &session.pane {
            _ if elsewhere => area = area.bg(a.panel_fill(t.bg)).child(elsewhere_hint(&t, &a)),
            Some(pane) => area = area.child(pane.clone()),
            None if marks.preview => area = area.bg(a.panel_fill(t.bg)).child(preview_note(&t, &a)),
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
