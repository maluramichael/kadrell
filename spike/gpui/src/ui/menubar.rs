//! Gezeichnete Menüleiste für Windows (GPUI hat dort kein App-Menü). Mit Maus und Tastatur bedienbar:
//! Alt allein öffnet und schließt, Pfeile wechseln Menü und Eintrag, Enter löst aus, Esc schließt.
//! Auf macOS ersetzt die System-Menüleiste (`menus::init`) diese Ansicht, sie bleibt dort leer.

use gpui::{
    Context, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent, ModifiersChangedEvent, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, anchored, deferred, div, point, prelude::FluentBuilder, px,
};

use super::theme;
use crate::menus::{Entry, MenuDef, menus};

/// Höhe der Titelzeile, darunter beginnt das Dropdown.
pub const TITLE_HEIGHT: f32 = 32.;
const ROW_HEIGHT: f32 = 26.;
/// Zeichenbreite der Terminalschrift bei 12 px, damit Breiten und Lage der Dropdowns feststehen.
const CHAR_W: f32 = 7.3;
const TITLE_PAD: f32 = 10.;
const BAR_LEFT: f32 = 4.;

pub struct MenuBar {
    menus: Vec<MenuDef>,
    open: Option<usize>,
    active: Option<usize>,
    focus: FocusHandle,
    restore: Option<FocusHandle>,
    alt_clean: bool,
}

impl Focusable for MenuBar {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

fn is_enabled(e: &Entry) -> bool {
    matches!(e, Entry::Item { action: Some(_), .. })
}

fn title_width(name: &str) -> f32 {
    name.chars().count() as f32 * CHAR_W + 2. * TITLE_PAD
}

fn panel_width(menu: &MenuDef) -> f32 {
    let wide = |e: &Entry| match e {
        Entry::Item { label, shortcut, .. } => (label.chars().count() + shortcut.chars().count()) as f32,
        Entry::Separator => 0.,
    };
    let chars = menu.entries.iter().map(wide).fold(0., f32::max);
    chars * CHAR_W + 64.
}

impl MenuBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        MenuBar { menus: menus(), open: None, active: None, focus: cx.focus_handle(), restore: None, alt_clean: false }
    }

    /// Öffnet ein Menü nach Namen, für Screenshots ohne Eingabe (`KADRELL_SPIKE_MENU=Datei`).
    pub fn open_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.menus.iter().position(|m| m.name == name) {
            self.open(i, window, cx);
        }
    }

    fn open(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.is_none() {
            self.restore = window.focused(cx).filter(|f| *f != self.focus);
        }
        self.open = Some(i);
        self.active = None;
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = None;
        self.active = None;
        if let Some(prev) = self.restore.take() {
            prev.focus(window, cx);
        }
        cx.notify();
    }

    fn toggle(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.open == Some(i) { self.close(window, cx) } else { self.open(i, window, cx) }
    }

    /// Alt allein (gedrückt und losgelassen, ohne andere Taste dazwischen) öffnet das erste Menü oder schließt.
    pub fn on_modifiers(&mut self, ev: &ModifiersChangedEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.modifiers;
        if m.alt {
            self.alt_clean = !m.control && !m.shift && !m.platform;
            return;
        }
        if !std::mem::take(&mut self.alt_clean) {
            return;
        }
        if self.open.is_some() { self.close(window, cx) } else { self.open(0, window, cx) }
    }

    /// Jede andere Taste macht aus Alt einen Tastenkürzel-Modifier, der die Leiste nicht öffnet.
    pub fn note_key(&mut self) {
        self.alt_clean = false;
    }

    fn activate(&mut self, j: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.open else { return };
        let Some(Entry::Item { action: Some(action), .. }) = self.menus[i].entries.get(j) else { return };
        let action = action.boxed_clone();
        self.close(window, cx);
        window.dispatch_action(action, cx);
    }

    fn step_item(&mut self, dir: isize, cx: &mut Context<Self>) {
        let Some(i) = self.open else { return };
        let entries = &self.menus[i].entries;
        let n = entries.len() as isize;
        let mut at = self.active.map_or(if dir > 0 { -1 } else { n }, |a| a as isize);
        for _ in 0..n {
            at = (at + dir).rem_euclid(n);
            if is_enabled(&entries[at as usize]) {
                self.active = Some(at as usize);
                break;
            }
        }
        cx.notify();
    }

    fn step_menu(&mut self, dir: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.open else { return };
        let n = self.menus.len() as isize;
        self.open(((i as isize + dir).rem_euclid(n)) as usize, window, cx);
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.is_none() {
            return;
        }
        match ev.keystroke.key.as_str() {
            "escape" => self.close(window, cx),
            "left" => self.step_menu(-1, window, cx),
            "right" => self.step_menu(1, window, cx),
            "up" => self.step_item(-1, cx),
            "down" => self.step_item(1, cx),
            "enter" => {
                if let Some(j) = self.active {
                    self.activate(j, window, cx);
                }
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn title(&self, i: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = theme(cx);
        let open = self.open == Some(i);
        let name = self.menus[i].name;
        div()
            .id(("menu-title", i))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h_full()
            .w(px(title_width(name)))
            .text_color(t.fg)
            .when(open, |d| d.bg(t.surface))
            .hover(move |s| s.bg(t.surface))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| this.toggle(i, window, cx)))
            .on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                if *hovered && this.open.is_some() && this.open != Some(i) {
                    this.open(i, window, cx);
                }
            }))
            .child(name)
    }

    fn row(&self, j: usize, entry: &Entry, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = theme(cx);
        let Entry::Item { label, shortcut, action } = entry else {
            return div().h(px(1.)).mx(px(8.)).my(px(4.)).bg(t.line).into_any_element();
        };
        let enabled = action.is_some();
        let on = enabled && self.active == Some(j);
        let ink = if !enabled { t.muted } else if on { t.pill_text() } else { t.fg };
        div()
            .id(("menu-row", j))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(24.))
            .h(px(ROW_HEIGHT))
            .px(px(12.))
            .text_color(ink)
            .when(on, |d| d.bg(t.accent))
            .child(*label)
            .child(div().text_color(if on { ink } else { t.muted }).child(*shortcut))
            .when(enabled, |row| {
                row.on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| this.activate(j, window, cx)))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered && this.active != Some(j) {
                            this.active = Some(j);
                            cx.notify();
                        }
                    }))
            })
            .into_any_element()
    }

    fn dropdown(&self, i: usize, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = theme(cx);
        let x = BAR_LEFT + self.menus[..i].iter().map(|m| title_width(m.name)).sum::<f32>();
        let menu = &self.menus[i];
        let rows: Vec<_> = menu.entries.iter().enumerate().map(|(j, e)| self.row(j, e, cx)).collect();
        let panel = div()
            .id("menu-panel")
            .occlude()
            .flex()
            .flex_col()
            .w(px(panel_width(menu)))
            .py(px(4.))
            .bg(t.panel)
            .border_1()
            .border_color(t.line)
            .shadow_lg()
            .children(rows);
        let view = window.viewport_size();
        let blocker = div()
            .w(view.width)
            .h(view.height - px(TITLE_HEIGHT))
            .on_any_mouse_down(cx.listener(|this, _, window, cx| this.close(window, cx)));
        div()
            .child(deferred(anchored().position(point(px(0.), px(TITLE_HEIGHT))).child(blocker)).with_priority(1))
            .child(deferred(anchored().position(point(px(x), px(TITLE_HEIGHT - 1.))).child(panel)).with_priority(2))
    }
}

impl Render for MenuBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div()
            .id("menubar")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| this.on_key(ev, window, cx)))
            .flex()
            .flex_none()
            .h_full()
            .pl(px(BAR_LEFT));
        if cfg!(target_os = "macos") {
            return row;
        }
        for i in 0..self.menus.len() {
            row = row.child(self.title(i, cx));
        }
        match self.open {
            Some(i) => row.child(self.dropdown(i, window, cx)),
            None => row,
        }
    }
}
