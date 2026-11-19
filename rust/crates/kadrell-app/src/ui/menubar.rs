//! Gezeichnete Menüleiste für Windows (GPUI hat dort kein App-Menü). Mit Maus und Tastatur bedienbar:
//! Alt allein öffnet und schließt, Pfeile wechseln Menü und Eintrag, Enter löst aus, Esc schließt. Untermenüs
//! („Verschieben nach“, „Kachel wählen“) klappen beim Darüberfahren oder mit → bzw. Enter nach rechts auf, ← schließt
//! sie. Der Zustand der Einträge (ausgegraut, abgehakt, Belegung der Kürzel) kommt als Schnappschuss von der Shell
//! (`set_context`). Auf macOS ersetzt die System-Menüleiste (`menus::init`) diese Ansicht, sie bleibt dort leer.

use gpui::{
    Context, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent, ModifiersChangedEvent, MouseButton,
    ParentElement, Render, StatefulInteractiveElement, Styled, Window, anchored, deferred, div, point, prelude::FluentBuilder, px,
};

use super::theme;
use crate::menu_ctx::MenuContext;
use crate::menus::{Entry, MenuDef, menus_for};

/// Höhe der Titelzeile, darunter beginnt das Dropdown.
pub const TITLE_HEIGHT: f32 = 32.;
const ROW_HEIGHT: f32 = 26.;
/// Trennlinie: 1 px Linie plus 4 px Rand oben und unten.
const SEPARATOR_HEIGHT: f32 = 9.;
const PANEL_PAD: f32 = 4.;
/// Zeichenbreite der Terminalschrift bei 12 px, damit Breiten und Lage der Dropdowns feststehen.
const CHAR_W: f32 = 7.3;
const TITLE_PAD: f32 = 10.;
const BAR_LEFT: f32 = 4.;

/// Ebene eines Eintrags: das Dropdown oder sein aufgeklapptes Untermenü.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Top,
    Sub,
}

pub struct MenuBar {
    menus: Vec<MenuDef>,
    ctx: MenuContext,
    open: Option<usize>,
    active: Option<usize>,
    /// Aufgeklapptes Untermenü: Zeile im Dropdown und markierter Eintrag darin.
    sub: Option<usize>,
    sub_active: Option<usize>,
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
    match e {
        Entry::Item { action, .. } | Entry::Choice { action, .. } => action.is_some(),
        Entry::Sub { entries, .. } => !entries.is_empty(),
        Entry::Separator => false,
    }
}

fn title_width(name: &str) -> f32 {
    name.chars().count() as f32 * CHAR_W + 2. * TITLE_PAD
}

/// Zeichen, die eine Zeile braucht: Beschriftung, Kürzel bzw. Pfeil, bei Wahlpunkten Platz für Haken und Punkt.
fn row_chars(e: &Entry) -> f32 {
    match e {
        Entry::Item { label, shortcut, .. } => (label.chars().count() + shortcut.chars().count()) as f32,
        Entry::Sub { label, .. } => (label.chars().count() + 2) as f32,
        Entry::Choice { label, .. } => (label.chars().count() + 5) as f32,
        Entry::Separator => 0.,
    }
}

fn entries_width(entries: &[Entry]) -> f32 {
    entries.iter().map(row_chars).fold(0., f32::max) * CHAR_W + 64.
}

fn panel_width(menu: &MenuDef) -> f32 {
    entries_width(&menu.entries)
}

/// Abstand der Zeile `j` vom oberen Rand der Liste (Trennlinien sind niedriger als Einträge).
fn row_offset(entries: &[Entry], j: usize) -> f32 {
    entries.iter().take(j).map(|e| if matches!(e, Entry::Separator) { SEPARATOR_HEIGHT } else { ROW_HEIGHT }).sum()
}

/// Nächster aktivierbarer Eintrag ab `from` in Richtung `dir`, reihum; `None` ohne einen.
fn next_enabled(entries: &[Entry], from: Option<usize>, dir: isize) -> Option<usize> {
    let n = entries.len() as isize;
    let mut at = from.map_or(if dir > 0 { -1 } else { n }, |a| a as isize);
    for _ in 0..n {
        at = (at + dir).rem_euclid(n);
        if is_enabled(&entries[at as usize]) {
            return Some(at as usize);
        }
    }
    None
}

impl MenuBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let ctx = MenuContext::live();
        MenuBar { menus: menus_for(&ctx), ctx, open: None, active: None, sub: None, sub_active: None, focus: cx.focus_handle(), restore: None, alt_clean: false }
    }

    /// Baut die Menüs neu, nach einem Sprachwechsel.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.menus = menus_for(&self.ctx);
        cx.notify();
    }

    /// Neuer Schnappschuss der Shell (Fokus, Gruppen, Belegung): die Menüs entstehen daraus neu. Ein offenes Menü bleibt
    /// offen. Ohne `notify`, die Shell zeichnet die Leiste ohnehin im selben Bild.
    pub fn set_context(&mut self, ctx: &MenuContext) {
        self.ctx = ctx.clone();
        self.menus = menus_for(ctx);
    }

    /// Öffnet ein Menü nach Namen, für Screenshots ohne Eingabe (`KADRELL_DEV_MENU=Datei`).
    pub fn open_named(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.menus.iter().position(|m| m.name == name || m.key == name) {
            self.open(i, window, cx);
        }
    }

    fn open(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.is_none() {
            self.restore = window.focused(cx).filter(|f| *f != self.focus);
        }
        self.open = Some(i);
        self.active = None;
        self.close_sub();
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = None;
        self.active = None;
        self.close_sub();
        if let Some(prev) = self.restore.take() {
            prev.focus(window, cx);
        }
        cx.notify();
    }

    fn close_sub(&mut self) {
        self.sub = None;
        self.sub_active = None;
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

    /// Einträge des offenen Menüs bzw. seines aufgeklappten Untermenüs.
    fn entries(&self, level: Level) -> &[Entry] {
        let Some(menu) = self.open.and_then(|i| self.menus.get(i)) else { return &[] };
        match level {
            Level::Top => &menu.entries,
            Level::Sub => match self.sub.and_then(|j| menu.entries.get(j)) {
                Some(Entry::Sub { entries, .. }) => entries,
                _ => &[],
            },
        }
    }

    /// Eintrag auslösen: ein Untermenü klappt auf, alles andere schließt die Leiste und sendet die Aktion.
    fn activate(&mut self, level: Level, j: usize, window: &mut Window, cx: &mut Context<Self>) {
        let action = match self.entries(level).get(j) {
            Some(Entry::Sub { entries, .. }) => {
                let first = (!entries.is_empty()).then(|| next_enabled(entries, None, 1));
                if let Some(first) = first {
                    self.sub = Some(j);
                    self.sub_active = first;
                    cx.notify();
                }
                return;
            }
            Some(Entry::Item { action: Some(a), .. } | Entry::Choice { action: Some(a), .. }) => a.boxed_clone(),
            _ => return,
        };
        self.close(window, cx);
        window.dispatch_action(action, cx);
    }

    /// Maus über einem Eintrag: markieren; über einem Untermenü aufklappen, über einem anderen Eintrag der Ebene zuklappen.
    fn hover(&mut self, level: Level, j: usize, cx: &mut Context<Self>) {
        match level {
            Level::Sub => self.sub_active = Some(j),
            Level::Top => {
                self.active = Some(j);
                let opens = matches!(self.entries(Level::Top).get(j), Some(Entry::Sub { entries, .. }) if !entries.is_empty());
                if opens {
                    self.sub = Some(j);
                } else {
                    self.close_sub();
                }
            }
        }
        cx.notify();
    }

    fn step_item(&mut self, dir: isize, cx: &mut Context<Self>) {
        if self.sub.is_some() {
            self.sub_active = next_enabled(self.entries(Level::Sub), self.sub_active, dir);
        } else {
            self.active = next_enabled(self.entries(Level::Top), self.active, dir);
        }
        cx.notify();
    }

    fn step_menu(&mut self, dir: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.open else { return };
        let n = self.menus.len() as isize;
        self.open(((i as isize + dir).rem_euclid(n)) as usize, window, cx);
    }

    /// →: ein Untermenü aufklappen, sonst zum nächsten Menü. ←: das Untermenü zuklappen, sonst zum vorigen Menü.
    fn step_side(&mut self, dir: isize, window: &mut Window, cx: &mut Context<Self>) {
        let on_sub = self.active.is_some_and(|j| matches!(self.entries(Level::Top).get(j), Some(Entry::Sub { .. })));
        match (dir, self.sub.is_some(), on_sub) {
            (1, false, true) => self.activate(Level::Top, self.active.unwrap_or(0), window, cx),
            (-1, true, _) => {
                self.close_sub();
                cx.notify();
            }
            _ => self.step_menu(dir, window, cx),
        }
    }

    fn on_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match (self.sub, self.sub_active, self.active) {
            (Some(_), Some(k), _) => self.activate(Level::Sub, k, window, cx),
            (None, _, Some(j)) => self.activate(Level::Top, j, window, cx),
            _ => {}
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.open.is_none() {
            return;
        }
        match ev.keystroke.key.as_str() {
            "escape" => self.close(window, cx),
            "left" => self.step_side(-1, window, cx),
            "right" => self.step_side(1, window, cx),
            "up" => self.step_item(-1, cx),
            "down" => self.step_item(1, cx),
            "enter" => self.on_enter(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn title(&self, i: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = theme(cx);
        let open = self.open == Some(i);
        let name = self.menus[i].name.clone();
        div()
            .id(("menu-title", i))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h_full()
            .w(px(title_width(&name)))
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

    /// Vorderes Ende einer Zeile: bei Wahlpunkten Haken und Farbpunkt.
    fn lead(&self, entry: &Entry, ink: gpui::Hsla, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let Entry::Choice { dot, checked, .. } = entry else { return None };
        let t = theme(cx);
        let mark = div().w(px(12.)).text_color(ink).child(if *checked { "✓" } else { "" });
        let dot = dot.map(|c| div().w(px(8.)).h(px(8.)).rounded_full().bg(t.group(c)));
        Some(div().flex().items_center().gap(px(6.)).child(mark).children(dot).into_any_element())
    }

    fn row(&self, level: Level, j: usize, entry: &Entry, cx: &mut Context<Self>) -> gpui::AnyElement {
        let t = theme(cx);
        let (label, shortcut) = match entry {
            Entry::Separator => return div().h(px(1.)).mx(px(8.)).my(px(4.)).bg(t.line).into_any_element(),
            Entry::Item { label, shortcut, .. } => (label.clone(), shortcut.clone()),
            Entry::Sub { label, .. } => (label.clone(), "▸".to_string()),
            Entry::Choice { label, .. } => (label.clone(), String::new()),
        };
        let enabled = is_enabled(entry);
        let current = if level == Level::Top { self.active } else { self.sub_active };
        let on = enabled && current == Some(j);
        let ink = if !enabled { t.muted } else if on { t.pill_text() } else { t.fg };
        let id = if level == Level::Top { "menu-row" } else { "menu-sub-row" };
        let name = div().flex().items_center().gap(px(6.)).children(self.lead(entry, ink, cx)).child(label);
        div()
            .id((id, j))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(24.))
            .h(px(ROW_HEIGHT))
            .px(px(12.))
            .text_color(ink)
            .when(on, |d| d.bg(t.accent))
            .child(name)
            .child(div().text_color(if on { ink } else { t.muted }).child(shortcut))
            .when(enabled, |row| {
                row.on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| this.activate(level, j, window, cx)))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.hover(level, j, cx);
                        }
                    }))
            })
            .into_any_element()
    }

    /// Eine Liste von Einträgen als Panel.
    fn panel(&self, level: Level, entries: &[Entry], width: f32, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let t = theme(cx);
        let rows: Vec<_> = entries.iter().enumerate().map(|(j, e)| self.row(level, j, e, cx)).collect();
        div()
            .id(if level == Level::Top { "menu-panel" } else { "menu-sub-panel" })
            .occlude()
            .flex()
            .flex_col()
            .w(px(width))
            .py(px(PANEL_PAD))
            .bg(t.panel)
            .border_1()
            .border_color(t.line)
            .shadow_lg()
            .children(rows)
    }

    /// Das aufgeklappte Untermenü rechts neben seiner Zeile.
    fn flyout(&self, x: f32, width: f32, menu: &MenuDef, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let j = self.sub?;
        let Some(Entry::Sub { entries, .. }) = menu.entries.get(j) else { return None };
        let y = TITLE_HEIGHT - 1. + 1. + PANEL_PAD + row_offset(&menu.entries, j) - PANEL_PAD;
        let panel = self.panel(Level::Sub, entries, entries_width(entries), cx);
        Some(deferred(anchored().position(point(px(x + width - 2.), px(y))).child(panel)).with_priority(3))
    }

    fn dropdown(&self, i: usize, window: &Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let x = BAR_LEFT + self.menus[..i].iter().map(|m| title_width(&m.name)).sum::<f32>();
        let menu = &self.menus[i];
        let width = panel_width(menu);
        let panel = self.panel(Level::Top, &menu.entries, width, cx);
        let view = window.viewport_size();
        let blocker = div()
            .w(view.width)
            .h(view.height - px(TITLE_HEIGHT))
            .on_any_mouse_down(cx.listener(|this, _, window, cx| this.close(window, cx)));
        div()
            .child(deferred(anchored().position(point(px(0.), px(TITLE_HEIGHT))).child(blocker)).with_priority(1))
            .child(deferred(anchored().position(point(px(x), px(TITLE_HEIGHT - 1.))).child(panel)).with_priority(2))
            .children(self.flyout(x, width, menu, cx))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::{Entry, Quit};

    fn item(enabled: bool) -> Entry {
        Entry::Item { key: "x", label: "x".into(), shortcut: String::new(), action: enabled.then(|| Box::new(Quit) as Box<dyn gpui::Action>) }
    }

    #[test]
    fn stepping_skips_separators_and_disabled_entries_and_wraps() {
        let entries = vec![item(false), item(true), Entry::Separator, item(true), item(false)];
        assert_eq!(next_enabled(&entries, None, 1), Some(1));
        assert_eq!(next_enabled(&entries, Some(1), 1), Some(3));
        assert_eq!(next_enabled(&entries, Some(3), 1), Some(1), "am Ende wieder vorn");
        assert_eq!(next_enabled(&entries, None, -1), Some(3));
        assert_eq!(next_enabled(&[item(false)], None, 1), None);
    }

    #[test]
    fn a_submenu_without_entries_is_off() {
        assert!(!is_enabled(&Entry::Sub { label: "s".into(), entries: Vec::new() }));
        assert!(is_enabled(&Entry::Sub { label: "s".into(), entries: vec![item(true)] }));
    }

    #[test]
    fn row_offsets_count_separators_lower_than_entries() {
        let entries = vec![item(true), Entry::Separator, item(true)];
        assert_eq!(row_offset(&entries, 0), 0.);
        assert_eq!(row_offset(&entries, 1), ROW_HEIGHT);
        assert_eq!(row_offset(&entries, 2), ROW_HEIGHT + SEPARATOR_HEIGHT);
    }

    #[test]
    fn panels_are_as_wide_as_their_widest_row() {
        let narrow = vec![item(true)];
        let wide = vec![Entry::Choice { label: "eine sehr lange Gruppe".into(), dot: None, checked: false, action: None }];
        assert!(entries_width(&wide) > entries_width(&narrow));
    }
}
