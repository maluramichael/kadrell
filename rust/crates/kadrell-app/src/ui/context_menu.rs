//! Kontextmenü auf Zeilen des Baums (Rechtsklick) und die Aktionen für die fokussierte Session, die Menü, Palette
//! und Kontextmenü gemeinsam nutzen. Es zeigt nur, was verdrahtet ist; Ziehen zum Umsortieren und die
//! Mehrfachauswahl-Einträge der Mac-App fehlen noch.

use gpui::{
    ClipboardItem, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, Point, StatefulInteractiveElement,
    Styled, Window, div, px,
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
    Rename(Target),
    CopyPath(String),
    ToggleFavorite(String),
    CloseGroup(String),
    /// Konto-Menü der Leiste (Wechseln, Auto-Wechsel, Hinzufügen, Verwalten).
    Account(crate::usage::AccountCmd),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Separator,
    Entry(String, Cmd),
}

pub struct ContextMenu {
    pub pos: Point<Pixels>,
    pub items: Vec<Item>,
}

const ROW: f32 = 24.;
const WIDTH: f32 = 260.;

fn entry(label: &str, cmd: Cmd) -> Item {
    Item::Entry(label.to_string(), cmd)
}

impl Shell {
    /// Einträge für eine Session. Prozess-Einträge hängen vom Lauf-Zustand ab, `Verschieben nach` listet die
    /// anderen Gruppen.
    pub fn session_menu_items(&self, key: &str) -> Vec<Item> {
        let mut items = Vec::new();
        if self.state.run(key) == Run::Running {
            items.push(entry("Stoppen", Cmd::Stop(key.into())));
        } else {
            items.push(entry("Fortsetzen", Cmd::Resume(key.into())));
        }
        items.push(entry("Umbenennen", Cmd::Rename(Target::Session(key.into()))));
        items.push(Item::Separator);
        items.push(entry("Nur diese zeigen", Cmd::ShowOnly(key.into())));
        if self.state.selected.len() > 1 && self.state.selected.iter().any(|k| k == key) {
            items.push(entry("Aus Ansicht nehmen", Cmd::HideFromView(key.into())));
        }
        let own = self.state.store.groups.group_for_session(key).map(|g| g.id.clone());
        let others = self.state.store.groups.groups.iter().filter(|g| Some(&g.id) != own.as_ref());
        let moves: Vec<Item> = others.map(|g| entry(&format!("Verschieben nach: {}", g.name), Cmd::MoveTo { key: key.into(), group: g.id.clone() })).collect();
        if !moves.is_empty() {
            items.push(Item::Separator);
            items.extend(moves);
        }
        items.push(Item::Separator);
        if let Some(s) = self.state.session(key) {
            items.push(entry("Pfad kopieren", Cmd::CopyPath(s.cwd.clone())));
        }
        items.push(entry("Entfernen", Cmd::Remove(key.into())));
        items
    }

    pub fn group_menu_items(&self, id: &str) -> Vec<Item> {
        let favorite = self.state.store.groups.group(id).is_some_and(|g| g.favorite);
        vec![
            entry("Umbenennen", Cmd::Rename(Target::Group(id.into()))),
            entry(if favorite { "Herz entfernen" } else { "Herz setzen (bleibt auch leer)" }, Cmd::ToggleFavorite(id.into())),
            Item::Separator,
            entry("Gruppe schließen", Cmd::CloseGroup(id.into())),
        ]
    }

    pub fn open_context(&mut self, pos: Point<Pixels>, items: Vec<Item>, cx: &mut Context<Self>) {
        if items.is_empty() {
            return;
        }
        self.context = Some(ContextMenu { pos, items });
        cx.notify();
    }

    pub fn close_context(&mut self, cx: &mut Context<Self>) {
        if self.context.take().is_some() {
            cx.notify();
        }
    }

    /// Führt einen Eintrag aus und schließt das Menü.
    pub fn run_cmd(&mut self, cmd: Cmd, window: &mut Window, cx: &mut Context<Self>) {
        self.context = None;
        match cmd {
            Cmd::Resume(key) => {
                let _ = self.state.request_start(&key);
            }
            Cmd::Stop(key) => {
                self.state.stop_tile(&key);
            }
            Cmd::Remove(key) => self.state.remove_sessions(&[key]),
            Cmd::ShowOnly(key) => self.state.show_only(&key),
            Cmd::HideFromView(key) => self.state.select_keys(&[key], true),
            Cmd::MoveTo { key, group } => {
                self.state.move_sessions(&[key], &group);
            }
            Cmd::Rename(target) => return self.open_rename(target, window, cx),
            Cmd::CopyPath(path) => cx.write_to_clipboard(ClipboardItem::new_string(path)),
            Cmd::ToggleFavorite(id) => self.state.store.groups.toggle_favorite(&id),
            Cmd::CloseGroup(id) => self.state.close_group(&id),
            Cmd::Account(cmd) => return self.run_account_cmd(cmd, window, cx),
        }
        self.sync(window, cx);
    }

    // MARK: Aktionen für die fokussierte Session (Menü, Palette, Tastatur)

    fn focused_key(&self) -> Option<String> {
        self.state.focused.clone()
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

    /// Das offene Menü samt Fläche, die Klicks daneben abfängt.
    pub fn context_layer(&self, window: &Window, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let menu = self.context.as_ref()?;
        let t = theme(cx);
        let view = window.viewport_size();
        let height = menu.items.len() as f32 * ROW + 8.;
        let x = menu.pos.x.as_f32().min(view.width.as_f32() - WIDTH - 4.).max(0.);
        let y = menu.pos.y.as_f32().min(view.height.as_f32() - height - 4.).max(0.);
        let rows: Vec<_> = menu.items.iter().enumerate().map(|(i, item)| row(i, item, &t, cx)).collect();
        let panel = div()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(WIDTH))
            .py(px(4.))
            .rounded(px(8.))
            .bg(raised(&t))
            .border_1()
            .border_color(t.line)
            .text_size(px(12.))
            .text_color(t.fg)
            .children(rows);
        let close = cx.listener(|this, _, _, cx| this.close_context(cx));
        let close_right = cx.listener(|this, _, _, cx| this.close_context(cx));
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(MouseButton::Left, close)
                .on_mouse_down(MouseButton::Right, close_right)
                .child(panel),
        )
    }
}

fn row(i: usize, item: &Item, t: &crate::theme::Theme, cx: &mut Context<Shell>) -> gpui::AnyElement {
    match item {
        Item::Separator => div().h(px(1.)).my(px(3.)).mx(px(8.)).bg(t.line).into_any_element(),
        Item::Entry(label, cmd) => {
            let cmd = cmd.clone();
            let bg = t.bg;
            div()
                .id(("ctx", i))
                .flex()
                .items_center()
                .h(px(ROW))
                .mx(px(4.))
                .px(px(8.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(move |s| s.bg(bg))
                .on_click(cx.listener(move |this, _, window, cx| this.run_cmd(cmd.clone(), window, cx)))
                .child(label.clone())
                .into_any_element()
        }
    }
}
