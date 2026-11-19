//! Kontextmenü auf Zeilen des Baums (Rechtsklick, ⇧F10) und die Aktionen für die fokussierte Session, die Menü, Palette
//! und Kontextmenü gemeinsam nutzen. Der Inhalt der Session- und Gruppenmenüs steht in `groups::menu`; hier liegen das
//! Datenmodell (mit Untermenüs, Häkchen, ausgegrauten Einträgen), die Tastaturführung und das Zeichnen.

use gpui::{
    App, Context, FocusHandle, InteractiveElement, IntoElement, KeyBinding, MouseButton, ParentElement, Pixels, Point,
    StatefulInteractiveElement, Styled, Window, actions, div, px, rgb,
};

use super::overlay::raised;
use super::rename::Target;
use super::theme;
use crate::Shell;
use crate::state::Run;

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Resume(String),
    Stop(String),
    Remove(String),
    ShowOnly(String),
    HideFromView(String),
    MoveTo { key: String, group: String },
    /// Mehrere Sessions (Auswahl) in eine Gruppe.
    MoveMany { keys: Vec<String>, group: String },
    Rename(Target),
    CopyPath(String),
    ToggleFavorite(String),
    CloseGroup(String),
    /// Neue Claude-Session in dieser Gruppe (Ordner der Gruppe).
    NewSessionIn(String),
    /// Neues Terminal ohne Claude in dieser Gruppe.
    NewTerminalIn(String),
    /// Neue Session im Ordner dieser Session, in ihrer Gruppe.
    NewSessionHere(String),
    /// Alle Sessions der Gruppe zeigen.
    ShowGroup(String),
    /// Dialog „Gruppe bearbeiten“.
    EditGroup(String),
    /// Ordner im Explorer/Finder zeigen.
    Reveal(String),
    /// Ordner im Editor der Einstellungen öffnen.
    OpenEditor(String),
    /// Aktion eines Extension-Panels (`ui.action` an die Extension `tab`).
    Ext { tab: String, id: String },
    /// Konto-Menü der Leiste (Wechseln, Auto-Wechsel, Hinzufügen, Verwalten).
    Account(crate::usage::AccountCmd),
    /// Layout aus dem Menü der Layout-Taste der Leiste.
    Layout(crate::ui::layout::LayoutMode),
    /// Markierten Terminaltext der Kachel kopieren (Rechtsklick auf markierten Text).
    TermCopy(String),
    /// Zwischenablage in das Terminal der Kachel einsetzen.
    TermPaste(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Separator,
    Entry(String, Cmd),
    /// Ausgegraut, ohne Befehl.
    Off(String),
    /// Eintrag mit Zustand: Häkchen, Farbpunkt (Gruppenfarbe als `0xrrggbb`), ausgegraut wenn nicht `enabled`.
    Flag { label: String, cmd: Cmd, checked: bool, enabled: bool, dot: Option<u32> },
    /// Untermenü.
    Sub(String, Vec<Item>),
}

impl Item {
    /// Kann der Cursor hier stehen?
    fn selectable(&self) -> bool {
        match self {
            Item::Entry(..) | Item::Sub(..) => true,
            Item::Flag { enabled, .. } => *enabled,
            Item::Separator | Item::Off(_) => false,
        }
    }

    /// Höhe der Zeile.
    fn height(&self) -> f32 {
        if matches!(self, Item::Separator) { SEPARATOR } else { ROW }
    }
}

actions!(kadrell_menu, [MenuUp, MenuDown, MenuLeft, MenuRight, MenuAccept, MenuClose]);

const CONTEXT: &str = "KadrellMenu";

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", MenuUp, c),
        KeyBinding::new("down", MenuDown, c),
        KeyBinding::new("left", MenuLeft, c),
        KeyBinding::new("right", MenuRight, c),
        KeyBinding::new("enter", MenuAccept, c),
        KeyBinding::new("space", MenuAccept, c),
        KeyBinding::new("escape", MenuClose, c),
    ]);
}

/// Das offene Menü an einer Stelle des Fensters.
pub struct ContextMenu {
    pub pos: Point<Pixels>,
    nav: MenuNav,
    /// Gibt dem Menü die Tastatur (Pfeile, Enter, Esc).
    focus: FocusHandle,
    focus_pending: bool,
    /// Wer die Tastatur hatte, bevor das Menü aufging (Terminal, Panel einer Extension): dorthin geht sie zurück.
    prev_focus: Option<FocusHandle>,
}

impl std::ops::Deref for ContextMenu {
    type Target = MenuNav;
    fn deref(&self) -> &MenuNav {
        &self.nav
    }
}

impl std::ops::DerefMut for ContextMenu {
    fn deref_mut(&mut self) -> &mut MenuNav {
        &mut self.nav
    }
}

/// Einträge und Cursor, ohne Oberfläche: die Tastaturführung ist so testbar.
pub struct MenuNav {
    pub items: Vec<Item>,
    /// Cursor je offener Ebene: `path[0]` in `items`, `path[1]` im Untermenü dort, und so weiter. Steht der Cursor
    /// auf einem Untermenü, ist es aufgeklappt.
    pub path: Vec<usize>,
}

const ROW: f32 = 24.;
const SEPARATOR: f32 = 7.;
const WIDTH: f32 = 300.;
const PAD: f32 = 4.;

fn entry(label: &str, cmd: Cmd) -> Item {
    Item::Entry(label.to_string(), cmd)
}

/// Index des ersten wählbaren Eintrags ab `from` in Richtung `step`, ohne Umbruch (`None` am Ende).
fn seek(items: &[Item], from: isize, step: isize) -> Option<usize> {
    let mut i = from;
    while (0..items.len() as isize).contains(&i) {
        if items[i as usize].selectable() {
            return Some(i as usize);
        }
        i += step;
    }
    None
}

impl ContextMenu {
    pub fn new(pos: Point<Pixels>, items: Vec<Item>, focus: FocusHandle) -> Self {
        Self { pos, nav: MenuNav::new(items), focus, focus_pending: true, prev_focus: None }
    }
}

impl MenuNav {
    pub fn new(items: Vec<Item>) -> Self {
        Self { items, path: Vec::new() }
    }

    /// Einträge der Ebene `depth` (0 = Hauptmenü), `None` wenn dort kein Untermenü offen ist.
    pub fn level(&self, depth: usize) -> Option<&[Item]> {
        let mut items: &[Item] = &self.items;
        for d in 0..depth {
            match items.get(*self.path.get(d)?)? {
                Item::Sub(_, sub) => items = sub,
                _ => return None,
            }
        }
        Some(items)
    }

    /// Anzahl der sichtbaren Ebenen: das Hauptmenü, dazu jedes Untermenü, auf dem ein Cursor steht.
    pub fn depth(&self) -> usize {
        let mut depth = 1;
        while self.level(depth).is_some() {
            depth += 1;
        }
        depth
    }

    /// Eintrag unter dem Cursor.
    pub fn current(&self) -> Option<&Item> {
        let depth = self.path.len().checked_sub(1)?;
        self.level(depth)?.get(*self.path.last()?)
    }

    /// Der Cursor steht auf Ebene `depth` auf diesem Eintrag.
    pub fn cursor_at(&self, depth: usize) -> Option<usize> {
        self.path.get(depth).copied()
    }

    /// Ebene, auf der der Cursor arbeitet: die tiefste mit Cursor, oder das Hauptmenü.
    fn active_depth(&self) -> usize {
        self.path.len().saturating_sub(1)
    }

    /// Pfeil nach unten (`step` 1) oder oben (-1) auf der aktiven Ebene, mit Umbruch.
    pub fn step(&mut self, step: isize) {
        let depth = self.active_depth();
        let Some(items) = self.level(depth) else { return };
        let n = items.len() as isize;
        let start = match self.path.get(depth) {
            Some(&i) => i as isize + step,
            None if step > 0 => 0,
            None => n - 1,
        };
        let found = seek(items, start, step).or_else(|| seek(items, if step > 0 { 0 } else { n - 1 }, step));
        if let Some(i) = found {
            self.path.truncate(depth + 1);
            match self.path.get_mut(depth) {
                Some(slot) => *slot = i,
                None => self.path.push(i),
            }
        }
    }

    /// Pfeil nach rechts: ins Untermenü unter dem Cursor.
    pub fn right(&mut self) {
        let depth = self.active_depth();
        if !matches!(self.current(), Some(Item::Sub(..))) {
            return;
        }
        if let Some(first) = self.level(depth + 1).and_then(|sub| seek(sub, 0, 1)) {
            self.path.push(first);
        }
    }

    /// Pfeil nach links: aus dem Untermenü zurück; im Hauptmenü gibt es nichts zu tun.
    pub fn left(&mut self) -> bool {
        if self.path.len() > 1 {
            self.path.pop();
            return true;
        }
        false
    }

    /// Maus über Eintrag `idx` der Ebene `depth`: der Cursor folgt, ein Untermenü klappt auf.
    pub fn hover(&mut self, depth: usize, idx: usize) {
        if depth > self.path.len() {
            return;
        }
        if self.level(depth).and_then(|l| l.get(idx)).is_some_and(Item::selectable) {
            self.path.truncate(depth);
            self.path.push(idx);
        } else {
            self.path.truncate(depth);
        }
    }

    /// Enter: Befehl unter dem Cursor; auf einem Untermenü klappt es stattdessen auf.
    pub fn accept(&mut self) -> Option<Cmd> {
        match self.current()? {
            Item::Entry(_, cmd) | Item::Flag { cmd, enabled: true, .. } => Some(cmd.clone()),
            Item::Sub(..) => {
                self.right();
                None
            }
            _ => None,
        }
    }

    /// Obere Kante der Ebene `depth` und ihre Zeile `idx` (für die Lage von Untermenüs).
    fn row_top(&self, depth: usize, idx: usize, top: f32) -> f32 {
        let Some(items) = self.level(depth) else { return top };
        top + PAD + items.iter().take(idx).map(Item::height).sum::<f32>()
    }
}

impl Shell {
    /// Einträge für eine Session (Rechtsklick, ⇧F10), nach `AppDelegate+SessionMenu.swift`.
    pub fn session_menu_items(&self, key: &str) -> Vec<Item> {
        use super::groups::menu::{SessionMenu, session_menu};
        let Some(session) = self.state.session(key) else { return Vec::new() };
        let state = &self.state;
        let known = |k: &str| state.session(k).is_some();
        let m = SessionMenu {
            session,
            running: state.run(key) == Run::Running,
            selected: &state.selected,
            groups: &state.store.groups.groups,
            editor: !state.settings.editor_command().trim().is_empty(),
            known: &known,
            home: &state.home(),
        };
        session_menu(&m)
    }

    /// Einträge für eine Gruppe (Rechtsklick auf die Gruppenzeile), nach `AppDelegate+GroupMenu.swift`.
    pub fn group_menu_items(&self, id: &str) -> Vec<Item> {
        let state = &self.state;
        let Some(group) = state.store.groups.group(id) else { return Vec::new() };
        let known = |k: &str| state.session(k).is_some();
        super::groups::menu::group_menu(group, !state.settings.editor_command().trim().is_empty(), &known)
    }

    pub fn open_context(&mut self, pos: Point<Pixels>, items: Vec<Item>, cx: &mut Context<Self>) {
        if items.is_empty() {
            return;
        }
        let focus = cx.focus_handle();
        self.context = Some(ContextMenu::new(pos, items, focus));
        cx.notify();
    }

    /// Schließt das Menü, ohne der Oberfläche die Tastatur zurückzugeben (nur wo kein Fenster zur Hand ist).
    pub fn close_context(&mut self, cx: &mut Context<Self>) {
        if self.context.take().is_some() {
            cx.notify();
        }
    }

    /// Nimmt das Menü weg und gibt die Tastatur zurück, die es beim Öffnen bekommen hat.
    fn take_context(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.context.take() else { return false };
        if let Some(prev) = menu.prev_focus {
            window.focus(&prev, cx);
        }
        true
    }

    /// Schließt das Menü und gibt die Tastatur an die zurück, die sie vorher hatte (Fokus-Kachel, Panel).
    pub fn dismiss_context(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.take_context(window, cx) {
            self.sync(window, cx);
        }
    }

    /// Führt einen Eintrag aus und schließt das Menü.
    pub fn run_cmd(&mut self, cmd: Cmd, window: &mut Window, cx: &mut Context<Self>) {
        self.take_context(window, cx);
        match cmd {
            Cmd::Rename(target) => self.open_rename(target, window, cx),
            Cmd::Account(cmd) => self.run_account_cmd(cmd, window, cx),
            Cmd::TermCopy(key) => self.tile_copy(&key, cx),
            Cmd::TermPaste(key) => self.tile_paste(&key, cx),
            Cmd::Layout(mode) => {
                self.set_mode(mode, cx);
                self.sync(window, cx);
            }
            Cmd::Ext { tab, id } => {
                self.ext_action(&tab, &id);
                self.sync(window, cx);
            }
            other => self.run_tree_cmd(other, window, cx),
        }
    }

    // MARK: Aktionen für die fokussierte Session (Menü, Palette, Tastatur)

    fn focused_key(&self) -> Option<String> {
        self.state.cur_focus()
    }

    pub fn stop_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = self.focused_key() {
            self.run_cmd(Cmd::Stop(key), window, cx);
        }
    }

    pub fn resume_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = self.focused_key() {
            self.run_cmd(Cmd::Resume(key), window, cx);
        }
    }

    pub fn remove_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = self.focused_key() {
            self.run_cmd(Cmd::Remove(key), window, cx);
        }
    }

    pub fn rename_focused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(key) = self.focused_key() {
            self.open_rename(Target::Session(key), window, cx);
        }
    }

    pub fn rename_focused_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let group = self.focused_key().and_then(|k| self.state.store.groups.group_for_session(&k).map(|g| g.id.clone()));
        if let Some(id) = group {
            self.open_rename(Target::Group(id), window, cx);
        }
    }

    /// Palette `> verschieben`: die fokussierte Session in die Gruppe an dieser Position der Ansicht.
    pub fn move_focused_to(&mut self, group_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(key), Some(group)) = (self.focused_key(), self.groups.get(group_index).map(|g| g.id.clone())) else { return };
        self.run_cmd(Cmd::MoveTo { key, group }, window, cx);
    }

    pub fn toggle_zoom(&mut self, cx: &mut Context<Self>) {
        self.state.zoom = !self.state.zoom;
        cx.notify();
    }

    // MARK: Zeichnen

    /// Das offene Menü samt Fläche, die Klicks daneben abfängt. Das Menü holt sich beim ersten Zeichnen die Tastatur.
    pub fn context_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let menu = self.context.as_mut()?;
        if std::mem::take(&mut menu.focus_pending) {
            menu.prev_focus = window.focused(cx);
            menu.focus.focus(window, cx);
        }
        let menu = self.context.as_ref()?;
        let t = theme(cx);
        let view = window.viewport_size();
        let panels = panels(menu, (view.width.as_f32(), view.height.as_f32()), &t, cx);
        let (close, close_right) = (cx.listener(|this, _, w, cx| this.dismiss_context(w, cx)), cx.listener(|this, _, w, cx| this.dismiss_context(w, cx)));
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .key_context(CONTEXT)
                .track_focus(&menu.focus)
                .on_action(cx.listener(|this, _: &MenuDown, _, cx| this.menu_key(|m| m.step(1), cx)))
                .on_action(cx.listener(|this, _: &MenuUp, _, cx| this.menu_key(|m| m.step(-1), cx)))
                .on_action(cx.listener(|this, _: &MenuRight, _, cx| this.menu_key(|m| m.right(), cx)))
                .on_action(cx.listener(|this, _: &MenuLeft, w, cx| {
                    if !this.context.as_mut().is_some_and(|m| m.left()) {
                        this.dismiss_context(w, cx);
                    } else {
                        cx.notify();
                    }
                }))
                .on_action(cx.listener(|this, _: &MenuAccept, w, cx| this.menu_accept(w, cx)))
                .on_action(cx.listener(|this, _: &MenuClose, w, cx| this.dismiss_context(w, cx)))
                .on_mouse_down(MouseButton::Left, close)
                .on_mouse_down(MouseButton::Right, close_right)
                .children(panels),
        )
    }

    fn menu_key(&mut self, f: impl FnOnce(&mut ContextMenu), cx: &mut Context<Self>) {
        if let Some(menu) = self.context.as_mut() {
            f(menu);
            cx.notify();
        }
    }

    fn menu_accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cmd = self.context.as_mut().and_then(|m| m.accept());
        match cmd {
            Some(cmd) => self.run_cmd(cmd, window, cx),
            None => cx.notify(),
        }
    }
}

/// Alle sichtbaren Ebenen als Flächen: das Hauptmenü bei `pos`, jedes Untermenü neben der Zeile, die es öffnet.
fn panels(menu: &ContextMenu, view: (f32, f32), t: &crate::theme::Theme, cx: &mut Context<Shell>) -> Vec<gpui::AnyElement> {
    let mut out = Vec::new();
    let (mut x, mut y) = (menu.pos.x.as_f32(), menu.pos.y.as_f32());
    let height = |d: usize| menu.level(d).map_or(0., |l| l.iter().map(Item::height).sum::<f32>() + 2. * PAD);
    x = x.min(view.0 - WIDTH - 4.).max(0.);
    y = y.min(view.1 - height(0) - 4.).max(0.);
    for depth in 0..menu.depth() {
        let Some(items) = menu.level(depth) else { break };
        out.push(panel(menu, depth, items, (x, y), t, cx));
        let row_y = menu.row_top(depth, menu.cursor_at(depth).unwrap_or(0), y);
        // Nächste Ebene rechts daneben, am Rand des Fensters links.
        let right = x + WIDTH - 2.;
        x = if right + WIDTH > view.0 { (x - WIDTH + 2.).max(0.) } else { right };
        y = row_y.min(view.1 - height(depth + 1) - 4.).max(0.);
    }
    out
}

fn panel(menu: &ContextMenu, depth: usize, items: &[Item], at: (f32, f32), t: &crate::theme::Theme, cx: &mut Context<Shell>) -> gpui::AnyElement {
    let marks = items.iter().any(|i| matches!(i, Item::Flag { .. }));
    let cursor = menu.cursor_at(depth);
    let rows: Vec<_> = items.iter().enumerate().map(|(i, item)| row(depth, i, item, cursor == Some(i), marks, t, cx)).collect();
    div()
        .absolute()
        .left(px(at.0))
        .top(px(at.1))
        .w(px(WIDTH))
        .py(px(PAD))
        .rounded(px(8.))
        .bg(raised(t))
        .border_1()
        .border_color(t.line)
        .text_size(px(12.))
        .text_color(t.fg)
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(rows)
        .into_any_element()
}

fn label_row(label: &str, color: gpui::Hsla, marks: bool, checked: bool, dot: Option<u32>) -> gpui::Div {
    let mut row = div().flex().flex_1().min_w_0().items_center().gap(px(6.)).text_color(color);
    if marks {
        row = row.child(div().w(px(12.)).flex_none().text_size(px(11.)).child(if checked { "✓" } else { "" }));
    }
    if let Some(c) = dot {
        row = row.child(div().size(px(8.)).flex_none().rounded_full().bg(rgb(c)));
    }
    row.child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label.to_string()))
}

fn row(depth: usize, i: usize, item: &Item, on: bool, marks: bool, t: &crate::theme::Theme, cx: &mut Context<Shell>) -> gpui::AnyElement {
    let id = ("ctx", depth * 1000 + i);
    let face = |d: gpui::Stateful<gpui::Div>| {
        let d = d.flex().items_center().h(px(ROW)).mx(px(PAD)).px(px(8.)).rounded(px(5.));
        if on { d.bg(t.bg) } else { d }
    };
    let hover = cx.listener(move |this, hovered: &bool, _, cx| {
        if *hovered {
            this.menu_key(|m| m.hover(depth, i), cx);
        }
    });
    match item {
        Item::Separator => div().h(px(1.)).my(px(3.)).mx(px(8.)).bg(t.line).into_any_element(),
        Item::Off(label) => face(div().id(id)).child(label_row(label, t.muted, marks, false, None)).into_any_element(),
        Item::Entry(label, cmd) => {
            let cmd = cmd.clone();
            face(div().id(id))
                .cursor_pointer()
                .on_hover(hover)
                .on_click(cx.listener(move |this, _, window, cx| this.run_cmd(cmd.clone(), window, cx)))
                .child(label_row(label, t.fg, marks, false, None))
                .into_any_element()
        }
        Item::Flag { label, cmd, checked, enabled, dot } => {
            let cmd = cmd.clone();
            let base = face(div().id(id)).on_hover(hover);
            let color = if *enabled { t.fg } else { t.muted };
            let base = base.child(label_row(label, color, true, *checked, *dot));
            if !*enabled {
                return base.into_any_element();
            }
            base.cursor_pointer().on_click(cx.listener(move |this, _, window, cx| this.run_cmd(cmd.clone(), window, cx))).into_any_element()
        }
        Item::Sub(label, _) => face(div().id(id))
            .cursor_pointer()
            .on_hover(hover)
            .on_click(cx.listener(move |this, _, _, cx| this.menu_key(|m| m.hover(depth, i), cx)))
            .child(label_row(label, t.fg, marks, false, None))
            .child(super::icons::chevron(false, t.muted))
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(label: &str) -> Item {
        entry(label, Cmd::Stop(label.into()))
    }

    fn flag(label: &str, enabled: bool) -> Item {
        Item::Flag { label: label.into(), cmd: Cmd::Stop(label.into()), checked: !enabled, enabled, dot: None }
    }

    fn nav() -> MenuNav {
        MenuNav::new(vec![e("a"), Item::Separator, Item::Sub("sub".into(), vec![flag("x", false), flag("y", true), e("z")]), Item::Off("aus".into()), e("b")])
    }

    #[test]
    fn arrows_step_over_separators_and_wrap() {
        let mut n = nav();
        n.step(1);
        assert_eq!(n.path, [0]);
        n.step(1);
        assert_eq!(n.path, [2], "Trenner übersprungen");
        n.step(1);
        assert_eq!(n.path, [4], "ausgegrauter Eintrag übersprungen");
        n.step(1);
        assert_eq!(n.path, [0], "Umbruch");
        n.step(-1);
        assert_eq!(n.path, [4]);
    }

    #[test]
    fn up_from_nothing_starts_at_the_end() {
        let mut n = nav();
        n.step(-1);
        assert_eq!(n.path, [4]);
    }

    #[test]
    fn right_opens_submenu_and_left_closes_it() {
        let mut n = nav();
        n.step(1);
        n.step(1);
        assert_eq!(n.depth(), 2, "Cursor auf Untermenü klappt es auf");
        n.right();
        assert_eq!(n.path, [2, 1], "erster wählbarer Eintrag");
        assert_eq!(n.current(), Some(&flag("y", true)));
        n.step(1);
        assert_eq!(n.path, [2, 2]);
        n.step(1);
        assert_eq!(n.path, [2, 1], "Umbruch bleibt in der Ebene");
        assert!(n.left());
        assert_eq!(n.path, [2]);
        assert!(!n.left(), "im Hauptmenü bleibt links wirkungslos");
    }

    #[test]
    fn accept_runs_entries_and_opens_submenus() {
        let mut n = nav();
        n.step(1);
        assert_eq!(n.accept(), Some(Cmd::Stop("a".into())));
        n.step(1);
        assert_eq!(n.accept(), None);
        assert_eq!(n.path, [2, 1]);
        assert_eq!(n.accept(), Some(Cmd::Stop("y".into())));
    }

    #[test]
    fn hover_moves_cursor_and_ignores_disabled_rows() {
        let mut n = nav();
        n.hover(0, 2);
        assert_eq!(n.path, [2]);
        assert_eq!(n.depth(), 2);
        n.hover(1, 2);
        assert_eq!(n.path, [2, 2]);
        n.hover(1, 0);
        assert_eq!(n.path, [2], "ausgegrauter Eintrag nimmt den Cursor nicht");
        n.hover(0, 4);
        assert_eq!(n.path, [4]);
        assert_eq!(n.depth(), 1);
        n.hover(3, 0);
        assert_eq!(n.path, [4], "Ebene, die es nicht gibt");
    }

    #[test]
    fn seek_skips_unselectable_items() {
        let items = vec![Item::Separator, Item::Off("a".into()), e("b"), Item::Separator, e("c")];
        assert_eq!(seek(&items, 0, 1), Some(2));
        assert_eq!(seek(&items, 3, 1), Some(4));
        assert_eq!(seek(&items, 3, -1), Some(2));
        assert_eq!(seek(&items, 5, 1), None);
        let flag = Item::Flag { label: "x".into(), cmd: Cmd::Stop("x".into()), checked: true, enabled: false, dot: None };
        assert!(!flag.selectable());
    }

    #[test]
    fn heights_add_up() {
        assert_eq!(Item::Separator.height(), SEPARATOR);
        assert_eq!(e("a").height(), ROW);
    }
}
