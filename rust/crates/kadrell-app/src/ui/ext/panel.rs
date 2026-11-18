//! Seitenleiste der Extensions (`ExtensionPanelView.swift` und `+Drawing`): ein Tab je Extension mit Panel, darunter
//! ihr Baum im Stil des Session-Baums. Tastatur (nach Strg+3 / ⌘3): ↑↓ Eintrag, ←→ Tab, ⏎ erste Aktion, Alt+⏎ alle
//! Aktionen, Esc zurück. Dieselbe Komponente dient als rechte Seitenleiste und als linkes Dock unter dem Baum.

use gpui::prelude::FluentBuilder;
use gpui::{
    ClickEvent, Context, Div, EventEmitter, FocusHandle, Focusable, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, ParentElement, Pixels, Point, Render, ScrollHandle, Stateful, StatefulInteractiveElement, Styled,
    Window, div, px,
};
use kadrell_ext::manager::ExtensionManager;
use kadrell_ext::panel::PanelAction;

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
}

impl ExtPanel {
    pub fn new(id: &'static str, manager: ExtensionManager, cx: &mut Context<Self>) -> Self {
        Self { id, manager, model: PanelModel::default(), focus: cx.focus_handle(), scroll: ScrollHandle::new() }
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

    fn tabs(&self, t: &Theme, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().flex_none().items_center().gap(px(2.)).h(px(TAB_HEIGHT - 2.)).px(px(4.));
        for (i, entry) in self.model.panels().iter().enumerate() {
            let on = self.model.selected_tab() == Some(entry.0.as_str());
            let name = entry.0.clone();
            let label = div()
                .px(px(8.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .text_size(px(11.))
                .text_color(t.muted)
                .when(on, |d| d.bg(t.surface).text_color(t.key_label()).font_weight(FontWeight::BOLD))
                .child(tab_label(entry).to_string());
            let edge = div().id((self.id, i)).flex_none().pb(px(if on { 0. } else { 2. })).rounded(px(5.)).bg(if on { key_edge(t) } else { gpui::transparent_black() }).cursor_pointer().child(label);
            row = row.child(edge.on_click(cx.listener(move |this, _, _, cx| {
                this.model.select_tab(&name);
                cx.notify();
            })));
        }
        row
    }

    fn line_row(&self, i: usize, line: &Line, focused: bool, t: &Theme, cx: &mut Context<Self>) -> Stateful<Div> {
        let cursor = self.model.cursor() == Some(i);
        let x = 12. + line.depth as f32 * 12.;
        let face = line_face(line, x, t);
        let mut row = div().id((self.id, 1000 + i)).flex_none().w_full().h(px(line.height)).px(px(4.)).child(face);
        if line.selectable() {
            let hot = t.bg;
            let ring = if cursor && focused { t.accent } else { gpui::transparent_black() };
            row = row.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.model.set_cursor(Some(i));
                this.activate(i, cx);
            }));
            row = row.on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                this.model.set_cursor(Some(i));
                if let Some((tab, actions)) = this.model.actions(i) {
                    cx.emit(PanelEvent::Menu { position: ev.position, tab, actions });
                }
                cx.notify();
            }));
            row = row.rounded(px(6.)).border_2().border_color(ring).when(cursor, |d| d.bg(hot)).hover(move |s| s.bg(hot));
        }
        row
    }
}

fn line_face(line: &Line, x: f32, t: &Theme) -> Div {
    let base = div().flex().items_center().size_full().pl(px(x - 4.)).pr(px(6.)).gap(px(6.)).min_w_0();
    match &line.kind {
        LineKind::Section { title, open } => base
            .pl(px(x - 10.))
            .child(icons::chevron(*open, t.muted))
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(t.accent_text).child(title.to_uppercase())),
        LineKind::Item { text, detail, color, .. } => {
            let mut row = base;
            if let Some(c) = color {
                row = row.child(div().size(px(8.)).flex_none().rounded_full().bg(status_color(*c, t)));
            }
            row = row.child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(t.sub).hover(move |s| s.text_color(t.fg)).child(text.clone()));
            match detail.as_ref().filter(|d| !d.is_empty()) {
                Some(d) => row.child(div().flex_none().max_w(gpui::relative(0.4)).overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(px(10.5)).text_color(t.muted).child(d.clone())),
                None => row,
            }
        }
        LineKind::Text { text, color } => base
            .text_size(px(11.))
            .text_color(color.map_or(t.muted, |c| text_color(c, t)))
            .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(text.clone())),
        LineKind::Button { label, .. } => {
            let cap = div().flex().items_center().justify_center().size_full().rounded(px(4.)).bg(t.surface).text_size(px(11.)).text_color(t.key_label()).child(label.clone());
            div().size_full().px(px(x - 8.)).py(px(4.)).child(div().size_full().pb(px(2.)).rounded(px(5.)).bg(key_edge(t)).child(cap))
        }
    }
}

impl Render for ExtPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme(cx);
        let focused = self.focus.is_focused(window);
        let rows: Vec<_> = self.model.lines().iter().enumerate().map(|(i, l)| self.line_row(i, l, focused, &t, cx)).collect();
        let body = div().id((self.id, 9999usize)).flex().flex_col().flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).pt(px(4.)).pb(px(6.)).children(rows);
        div()
            .id(self.id)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev, window, cx| this.on_key(ev, window, cx)))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .bg(t.panel)
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
