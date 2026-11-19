//! Statusleiste unten (`FooterBarView.swift`), befüllt nur von Extensions (`kadrell.footer.set`). Flache Module im
//! Instrument-Look, getrennt durch Fugen: links ausgerichtete von links, rechts ausgerichtete von rechts. Die
//! Platzverteilung steht in `footer_model`.

use gpui::{
    AnyView, App, AppContext, Context, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Render, Role, SharedString,
    Stateful, StatefulInteractiveElement, Styled, TextRun, Window, div, px,
};
use kadrell_i18n::trd;

use super::footer_model::{
    BAR_GAP, BAR_HEIGHT, BAR_WIDTH, FooterEntry, GAUGE_HEIGHT, GAUGE_WIDTH, HEIGHT, PAD, PART_GAP, Part, Placed, layout, parts,
};
use super::{status_color, text_color};
use crate::Shell;
use crate::theme::{self, Theme};
use crate::ui::chrome::{h_seam, v_seam};
use crate::ui::overlay::{dialog_frame, raised};
use crate::ui::theme as app_theme;

const TEXT_SIZE: f32 = 11.;

/// Höhe der Leiste inklusive Fuge: 22 mal UI-Größe, gerundet (`FooterBarView.height`).
pub fn height() -> f32 {
    scaled_height(crate::ui::overlay::scale())
}

fn scaled_height(scale: f32) -> f32 {
    (HEIGHT * scale).round()
}

/// Was ein Bildschirmleser zu einem Modul hört: Einträge mit Aktion sind Tasten, die übrigen Text mit dem Tooltip als Wert
/// (`FooterBarView.accessibilityChildren`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub button: bool,
    pub label: String,
    pub value: Option<String>,
}

pub fn access(entry: &FooterEntry) -> Access {
    let item = &entry.1;
    let tip = item.tip.clone().filter(|t| !t.is_empty());
    Access { button: item.action.is_some(), label: item.spoken(), value: if item.action.is_some() { None } else { tip } }
}

/// Beschriftung der ganzen Leiste.
pub fn bar_label() -> String {
    trd!("Statusleiste der Extensions")
}

/// Breite eines Textes in der Schrift der Leiste, in px.
fn measure(window: &Window, text: &str) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: gpui::font(theme::FONT),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(SharedString::from(text.to_string()), px(TEXT_SIZE), &[run], None).width.as_f32()
}

/// Tooltip mit dem, was die Leiste zeigt.
struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = app_theme(cx);
        dialog_frame(&t).px(px(8.)).py(px(4.)).bg(raised(&t)).text_size(px(TEXT_SIZE)).text_color(t.fg).child(self.0.clone())
    }
}

fn tip_view(text: String) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |_, cx| cx.new(|_| Tip(text.clone().into())).into()
}

fn with_alpha(c: Hsla, factor: f32) -> Hsla {
    Hsla { a: c.a * factor, ..c }
}

/// Je Wert ein Balken auf gedämpfter Spur, gefüllt von unten.
fn bars(values: &[f64], ink: Hsla) -> Div {
    let track = with_alpha(ink, 0.22);
    let columns = values.iter().map(|v| {
        let filled = (BAR_HEIGHT * v.clamp(0., 1.) as f32).round();
        div().relative().flex_none().w(px(BAR_WIDTH)).h(px(BAR_HEIGHT)).bg(track).child(div().absolute().bottom_0().left_0().w(px(BAR_WIDTH)).h(px(filled)).bg(ink))
    });
    div().flex().flex_none().items_center().gap(px(BAR_GAP)).children(columns)
}

/// Waagerechter Pegel: gedämpfte Spur, von links gefüllt.
fn gauge(value: f64, ink: Hsla) -> Div {
    let filled = (GAUGE_WIDTH * value.clamp(0., 1.) as f32).round();
    div().relative().flex_none().w(px(GAUGE_WIDTH)).h(px(GAUGE_HEIGHT)).bg(with_alpha(ink, 0.22)).child(div().absolute().top_0().left_0().h(px(GAUGE_HEIGHT)).w(px(filled)).bg(ink))
}

fn part_element(part: Part, entry: &FooterEntry, t: &Theme) -> Div {
    let color = entry.1.color;
    let ink = color.map_or(t.sub, |c| status_color(c, t));
    match part {
        Part::Label(s) => div().flex_none().whitespace_nowrap().text_color(t.muted).child(s),
        Part::Bars(values) => bars(&values, ink),
        Part::Gauge(v) => gauge(v, ink),
        Part::Text(s) => div().flex_none().whitespace_nowrap().text_color(color.map_or(t.fg, |c| text_color(c, t))).child(s),
    }
}

/// Ein Modul: Label, Balken, Pegel und Text nebeneinander, senkrecht mittig. Mit Aktion klickbar, immer mit Tooltip.
fn module(i: usize, entry: &FooterEntry, at: &Placed, t: &Theme, cx: &mut Context<Shell>) -> Stateful<Div> {
    let children = parts(&entry.1).into_iter().map(|p| part_element(p, entry, t));
    let mut el = div()
        .id(("footer", i))
        .absolute()
        .left(px(at.x))
        .top_0()
        .w(px(at.width))
        .h_full()
        .flex()
        .items_center()
        .gap(px(PART_GAP))
        .px(px(PAD))
        .text_size(px(TEXT_SIZE))
        .children(children)
        .tooltip(tip_view(entry.1.tip.clone().unwrap_or_else(|| format!("{}: {}", entry.0, entry.1.spoken()))));
    let a = access(entry);
    el = el.role(if a.button { Role::Button } else { Role::Label }).aria_label(a.label);
    if let Some(value) = a.value {
        el = el.aria_value(value);
    }
    if let Some(action) = entry.1.action.clone() {
        let name = entry.0.clone();
        el = el.cursor_pointer().on_click(cx.listener(move |this, _, _, _| this.ext_action(&name, &action)));
    }
    el
}

/// Fuge hinter (links) bzw. vor (rechts) einem Modul.
fn seam_at(x: f32, t: &Theme) -> Div {
    div().absolute().left(px(x)).top_0().h_full().child(v_seam(t))
}

/// Die Leiste mit den Modulen, die bei `width` Platz haben.
pub fn footer_bar(entries: &[FooterEntry], window: &Window, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let t = app_theme(cx);
    let width = window.viewport_size().width.as_f32();
    let (left, right) = layout(entries, width, &|s| measure(window, s));
    let mut body = div().relative().flex_none().w_full().h(px(height() - 2.)).text_color(t.fg);
    for p in &left {
        body = body.child(module(p.index, &entries[p.index], p, &t, cx)).child(seam_at(p.x + p.width, &t));
    }
    for p in &right {
        body = body.child(seam_at(p.x - 0., &t)).child(module(p.index, &entries[p.index], p, &t, cx));
    }
    div().id("ext-footer").role(Role::Group).aria_label(bar_label()).flex().flex_col().flex_none().w_full().bg(t.panel).child(h_seam(&t)).child(body)
}

/// Eine leere Leiste: für die Schonfrist, in der die Leiste nach dem letzten Eintrag noch steht (kein Springen).
pub fn empty_footer(cx: &App) -> impl IntoElement + use<> {
    let t = app_theme(cx);
    div().flex().flex_col().flex_none().w_full().bg(t.panel).child(h_seam(&t)).child(div().h(px(height() - 2.)))
}

#[cfg(test)]
mod tests {
    use kadrell_ext::footer::{Align, FooterItem};

    use super::*;

    fn entry(id: &str, align: Align, action: Option<&str>, tip: Option<&str>) -> FooterEntry {
        let item = FooterItem {
            id: id.into(),
            align,
            text: Some("23%".into()),
            label: Some("CPU".into()),
            action: action.map(str::to_string),
            tip: tip.map(str::to_string),
            ..FooterItem::default()
        };
        ("ext".into(), item)
    }

    /// `testClickSendsActionAndVoiceOverSeesEntries`: Aktionen sind Tasten, die übrigen Text mit dem Tooltip als Wert.
    #[test]
    fn entries_with_action_are_buttons_and_others_speak_their_tip() {
        let button = access(&entry("a", Align::Left, Some("open"), Some("CPU 23 %")));
        assert!(button.button);
        assert_eq!(button.label, "CPU 23%");
        assert_eq!(button.value, None);
        let text = access(&entry("b", Align::Right, None, Some("CPU 23 %")));
        assert!(!text.button);
        assert_eq!(text.value.as_deref(), Some("CPU 23 %"));
        assert_eq!(access(&entry("c", Align::Right, None, None)).value, None);
    }

    /// `FooterBarView.height`: 22 mal UI-Größe, gerundet.
    #[test]
    fn height_follows_ui_scale_rounded() {
        assert_eq!(scaled_height(1.), 22.);
        assert_eq!(scaled_height(1.25), 28.);
        assert_eq!(scaled_height(0.8), 18.);
    }

    #[test]
    fn bar_is_named_for_screen_readers() {
        assert_eq!(bar_label(), "Statusleiste der Extensions");
    }

    #[test]
    fn unchanged_entries_compare_equal() {
        let a = vec![entry("a", Align::Left, None, None)];
        assert_eq!(a, a.clone());
        assert_ne!(a, vec![entry("b", Align::Left, None, None)]);
    }
}
