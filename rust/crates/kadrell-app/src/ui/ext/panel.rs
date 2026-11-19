//! Seitenleiste der Extensions (`ExtensionPanelView.swift` und `+Drawing`): ein Tab je Extension mit Panel, darunter
//! ihr Baum im Stil des Session-Baums. Tastatur (nach Strg+3 / ⌘3): ↑↓ Eintrag, ←→ Tab, ⏎ erste Aktion, Alt+⏎ alle
//! Aktionen, Esc zurück. Dieselbe Komponente dient als rechte Seitenleiste und als linkes Dock unter dem Baum.

use gpui::prelude::FluentBuilder;
use gpui::{
    ClickEvent, Context, Div, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, SharedString,
    MouseButton, MouseDownEvent, MouseUpEvent, ParentElement, Pixels, Point, Render, ScrollHandle, Stateful, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use kadrell_ext::manager::ExtensionManager;
use kadrell_ext::panel::{PanelAction, ThemeColor};

use super::model::{Line, LineKind, Outcome, PanelEntry, PanelModel, TAB_HEIGHT, tab_label};
use super::{status_color, text_color};
use crate::theme::Theme;
use crate::ui::chrome::{h_seam, key_edge};
use crate::ui::icons;
use crate::ui::theme;

/// Was die Seitenleiste der Shell meldet.
pub enum PanelEvent {
    /// Esc: die Tastatur geht zurück an das Terminal.
    Leave,
    /// Rechtsklick oder Alt+⏎: alle Aktionen eines Eintrags als Menü an dieser Stelle.
    Menu { position: Point<Pixels>, tab: String, actions: Vec<PanelAction> },
}

impl EventEmitter<PanelEvent> for ExtPanel {}

pub struct ExtPanel {
    id: &'static str,
    manager: ExtensionManager,
    model: PanelModel,
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// Gedrückte Zeile (Maus unten): ein Knopf sinkt ein, bis die Maus losgelassen wird.
    pressed: Option<usize>,
}

impl ExtPanel {
    pub fn new(id: &'static str, manager: ExtensionManager, cx: &mut Context<Self>) -> Self {
        Self { id, manager, model: PanelModel::default(), focus: cx.focus_handle(), scroll: ScrollHandle::new(), pressed: None }
    }

    /// Neue Panels vom Manager. Gezeichnet wird nur bei geändertem Stand: der Manager meldet bis zu 50-mal pro Sekunde.
    pub fn set_panels(&mut self, panels: Vec<PanelEntry>, cx: &mut Context<Self>) {
        if self.model.set_panels(panels) {
            cx.notify();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.model.is_empty()
    }

    pub fn model(&self) -> &PanelModel {
        &self.model
    }

    /// Nur per Strg+3 / ⌘3, nicht per Klick: sonst blitzt der Tastaturrahmen bei jedem Klick auf.
    pub fn take_keyboard(&self, window: &mut Window, cx: &mut gpui::App) {
        self.focus.focus(window, cx);
    }

    pub fn has_keyboard(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    fn fire(&self, outcome: Outcome) {
        if let Outcome::Fire { tab, id } = outcome {
            self.manager.action(&tab, &id);
        }
    }

    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        let outcome = self.model.activate(index);
        self.fire(outcome);
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        match (ev.keystroke.key.as_str(), m.alt) {
            ("down", false) => self.model.step(1),
            ("up", false) => self.model.step(-1),
            ("left", false) => self.model.switch_tab(-1),
            ("right", false) => self.model.switch_tab(1),
            ("enter" | "space", false) => self.activate_cursor(cx),
            ("enter", true) => self.show_actions(window, cx),
            ("escape", false) => cx.emit(PanelEvent::Leave),
            _ => return,
        }
        self.scroll_to_cursor();
        cx.stop_propagation();
        cx.notify();
    }

    fn activate_cursor(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = self.model.cursor() {
            self.activate(c, cx);
        }
    }

    /// Alt+⏎: Menü unter dem Eintrag der Tastaturzeile.
    fn show_actions(&mut self, _: &Window, cx: &mut Context<Self>) {
        let Some(c) = self.model.cursor() else { return };
        let Some((tab, actions)) = self.model.actions(c) else { return };
        let top: f32 = self.model.lines()[..c].iter().map(|l| l.height).sum();
        let bounds = self.scroll.bounds();
        let y = bounds.origin.y + px(top + self.model.lines()[c].height + 4.) + self.scroll.offset().y;
        cx.emit(PanelEvent::Menu { position: Point { x: bounds.origin.x + px(20.), y }, tab, actions });
    }

    /// Hält die Tastaturzeile im sichtbaren Bereich (nur bei langen Bäumen von Belang).
    fn scroll_to_cursor(&self) {
        let Some(c) = self.model.cursor() else { return };
        let top: f32 = self.model.lines()[..c].iter().map(|l| l.height).sum();
        let bottom = top + self.model.lines()[c].height;
        let view = self.scroll.bounds().size.height.as_f32();
        let offset = -self.scroll.offset().y.as_f32();
        if view <= 0. {
            return;
        }
        if top < offset {
            self.scroll.set_offset(gpui::point(px(0.), px(-top)));
        } else if bottom > offset + view {
            self.scroll.set_offset(gpui::point(px(0.), px(-(bottom - view))));
        }
    }

    // MARK: Zeichnen

    /// Tab als Taste: der gewählte bleibt eingesunken. Die Breite ist die des fetten Textes plus 16 (`tabRects`), die
    /// Taste sitzt 2 px eingerückt (Breite hier ohne Messen des fetten Textes: Abstand 6 px je Seite).
    fn tabs(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().flex_none().items_start().h(px(TAB_HEIGHT - 2.)).pt(px(5.)).px(px(4.));
        for (i, entry) in self.model.panels().iter().enumerate() {
            let on = self.model.selected_tab() == Some(entry.0.as_str());
            let name = entry.0.clone();
            let text = tab_label(entry).to_string();
            let face = div()
                .flex()
                .items_center()
                .justify_center()
                .flex_1()
                .px(px(6.))
                .rounded(px(5.))
                .bg(t.surface)
                .text_size(px(11.))
                .text_color(if on { t.key_label() } else { t.muted })
                .when(on, |d| d.font_weight(FontWeight::BOLD))
                .whitespace_nowrap()
                .child(text);
            let edge = div()
                .id((self.id, i))
                .flex()
                .flex_col()
                .flex_none()
                .h(px(22.))
                .mx(px(2.))
                .pt(px(if on { 2. } else { 0. }))
                .pb(px(if on { 0. } else { 2. }))
                .rounded(px(5.))
                .bg(key_edge(t))
                .cursor_pointer()
                .child(face);
            row = row.child(edge.on_click(cx.listener(move |this, _, _, cx| {
                this.model.select_tab(&name);
                cx.notify();
            })));
        }
        row
    }

    /// Eine Zeile: Fläche bei Hover und Auswahl (`Instrument.rowHighlight`), darüber der Inhalt.
    fn line_row(&self, i: usize, line: &Line, focused: bool, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let cursor = self.model.cursor() == Some(i);
        let x = 12. + line.depth as f32 * 12.;
        let group = SharedString::from(format!("{}-line-{i}", self.id));
        let hot = Hot { cursor, pressed: self.pressed == Some(i), group: group.clone() };
        let mut row = div().id((self.id, 1000 + i)).group(group.clone()).relative().flex_none().w_full().h(px(line.height));
        if line.selectable() {
            row = row.child(highlight(cursor, cursor && focused, &group, t)).cursor_pointer();
            row = self.row_events(row, i, cx);
        }
        row.child(line_face(line, hot, x, t))
    }

    /// Maus auf einer wählbaren Zeile: Drücken senkt den Knopf, Loslassen darüber löst aus, Rechtsklick zeigt die Aktionen.
    fn row_events(&self, row: Stateful<Div>, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        row.on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            this.pressed = Some(i);
            cx.notify();
        }))
        .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| {
            this.pressed = None;
            cx.notify();
        }))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.pressed = None;
            this.model.set_cursor(Some(i));
            this.activate(i, cx);
        }))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            this.model.set_cursor(Some(i));
            if let Some((tab, actions)) = this.model.actions(i) {
                cx.emit(PanelEvent::Menu { position: ev.position, tab, actions });
            }
            cx.notify();
        }))
    }
}

/// Zustand einer Zeile für das Zeichnen: Tastaturzeile, gedrückt und Name der Gruppe für den Hover.
struct Hot {
    cursor: bool,
    pressed: bool,
    group: SharedString,
}

/// Fläche der Zeile, eingerückt 6/1 mit Radius 6. Mit Tastaturfokus kommt Umriss (1,5 px `fg`) und außen der Akzentring
/// (2 px, 0,5 px Luft) dazu; Hover zeigt nur die Fläche.
fn highlight(cursor: bool, ring: bool, group: &SharedString, t: &Theme) -> Div {
    if ring {
        let fill = div().size_full().rounded(px(6.)).bg(t.bg).border_1().border_color(t.fg);
        return div().absolute().left(px(3.5)).right(px(3.5)).top(px(0.5)).bottom(px(0.5)).p(px(0.5)).rounded(px(8.)).border_2().border_color(t.accent).child(fill);
    }
    let area = div().absolute().left(px(6.)).right(px(6.)).top(px(1.)).bottom(px(1.)).rounded(px(6.));
    let face = t.bg;
    if cursor { area.bg(face) } else { area.group_hover(group.clone(), move |s| s.bg(face)) }
}

/// Neutrale Taste über die ganze Zeile (Knopf), solange die Maus gedrückt ist eingesunken.
fn key_cap(label: String, pressed: bool, t: &Theme) -> Div {
    let face = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_1()
        .rounded(px(4.))
        .bg(t.surface)
        .text_size(px(11.))
        .text_color(t.key_label())
        .whitespace_nowrap()
        .overflow_hidden()
        .text_ellipsis()
        .child(label);
    div().size_full().flex().flex_col().pt(px(if pressed { 2. } else { 0. })).pb(px(if pressed { 0. } else { 2. })).rounded(px(5.)).bg(key_edge(t)).child(face)
}

fn line_face(line: &Line, hot: Hot, x: f32, t: &Theme) -> Div {
    let base = div().absolute().size_full().flex().items_center().pl(px(x)).pr(px(10.)).gap(px(6.)).min_w_0();
    match &line.kind {
        LineKind::Section { title, open } => base
            .pl(px(x - 6.))
            .gap(px(4.))
            .child(div().flex_none().child(icons::chevron(*open, t.muted)))
            .child(div().min_w_0().overflow_hidden().child(crate::ui::instrument::kerned(&title.to_uppercase(), 11., crate::ui::instrument::LABEL_KERN, true, t.accent_text))),
        LineKind::Item { text, detail, color, .. } => item_face(base, text, detail.as_deref(), *color, &hot, t),
        LineKind::Text { text, color } => base
            .text_size(px(11.))
            .text_color(color.map_or(t.muted, |c| text_color(c, t)))
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text.clone())),
        LineKind::Button { label, .. } => div().absolute().size_full().pl(px(x)).pr(px(10.)).py(px(4.)).child(key_cap(label.clone(), hot.pressed, t)),
    }
}

/// Eintrag: Punkt (8 px), Titel in `sub`, bei Hover oder Tastaturzeile in `fg`, Detail rechts (10,5, höchstens 40 %).
fn item_face(base: Div, text: &str, detail: Option<&str>, color: Option<ThemeColor>, hot: &Hot, t: &Theme) -> Div {
    let mut row = base;
    if let Some(c) = color {
        row = row.child(div().size(px(8.)).flex_none().rounded_full().bg(status_color(c, t)));
    }
    let fg = t.fg;
    let title = div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_color(if hot.cursor { t.fg } else { t.sub })
        .group_hover(hot.group.clone(), move |s| s.text_color(fg))
        .child(text.to_string());
    row = row.child(title);
    match detail.filter(|d| !d.is_empty()) {
        Some(d) => row.child(div().flex_none().max_w(gpui::relative(0.4)).overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(10.5)).text_color(t.muted).child(d.to_string())),
        None => row,
    }
}

impl Render for ExtPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let focused = self.focus.is_focused(window);
        let rows: Vec<_> = self.model.lines().iter().enumerate().map(|(i, l)| self.line_row(i, l, focused, &t, cx)).collect();
        let body = div().id((self.id, 9999usize)).flex().flex_col().flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).pt(px(4.)).pb(px(6.)).children(rows);
        let label = self.model.a11y_label();
        let value = self.model.a11y_value();
        let mut root = div().id(self.id).role(gpui::Role::List).aria_label(label);
        if let Some(value) = value {
            root = root.aria_value(value);
        }
        root
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev, window, cx| this.on_key(ev, window, cx)))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(crate::appearance::current(cx).panel_fill(t.panel))
            .text_color(t.fg)
            .child(self.tabs(&t, cx))
            .child(h_seam(&t))
            .child(body)
    }
}

impl Focusable for ExtPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}
