//! Anbindung der Suche an die `Shell`: Momentaufnahme der Sessions beim Öffnen, Öffnen und Schließen, Wirkung der
//! Auswahl (Session zeigen, Gruppe, Layout, Menü-Aktion, Treffer im Terminal).

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gpui::{AppContext, Context, Entity, Window};

use super::model::{GroupRow, SessionRow, Snapshot};
use super::{MockHistory, Palette, PaletteEvent};
use crate::Shell;
use crate::ui::folders;
use crate::ui::layout::Selection;
use crate::ui::model::Group;

fn path_of(s: &crate::ui::model::Session) -> String {
    s.cwd.replace('\\', "/")
}

/// Gruppen und Sessions in der Form, die die Palette braucht (ohne Verweis auf die Shell).
fn snapshot(groups: &[Group], focused: Option<(usize, usize)>, home: &str) -> Snapshot {
    let rows = groups.iter().enumerate().map(|(g, group)| {
        let sessions: Vec<SessionRow> = group
            .sessions
            .iter()
            .enumerate()
            .map(|(s, session)| {
                let path = path_of(session);
                SessionRow {
                    id: (g, s),
                    title: session.title.clone(),
                    status: session.status,
                    short: folders::short_path(&path, home),
                    path,
                    has_pane: session.pane.is_some(),
                }
            })
            .collect();
        let path = if group.cwd.is_empty() { sessions.first().map(|s| s.path.clone()).unwrap_or_default() } else { group.cwd.replace('\\', "/") };
        GroupRow { index: g, name: group.name.clone(), short: folders::short_path(&path, home), path, sessions }
    });
    Snapshot { groups: rows.collect(), focused }
}

impl Shell {
    /// Strg+P / ⌘P: öffnet die Suche. Ist sie schon offen, schaltet es auf `>` (wie in der Mac-App); mit
    /// Präfix (Strg+Umschalt+F: `/`) wechselt die offene Suche in diesen Modus.
    pub fn open_palette(&mut self, prefix: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.palette.clone() {
            let text = if prefix.is_empty() { ">" } else { prefix };
            open.update(cx, |p, cx| p.set_input(text, window, cx));
            return;
        }
        if self.overlay.is_some() {
            return;
        }
        let panes: HashMap<_, _> = self
            .groups
            .iter()
            .enumerate()
            .flat_map(|(g, group)| group.sessions.iter().enumerate().map(move |(s, session)| ((g, s), session)))
            .filter_map(|(id, session)| Some((id, session.pane.clone()?)))
            .collect();
        let snap = snapshot(&self.groups, self.selected, &folders::home_dir());
        let history = Rc::new(MockHistory::new());
        let palette = cx.new(|cx| Palette::new(snap, panes, history, prefix, window, cx));
        let ext = self.ext_palette_commands();
        palette.update(cx, |p, cx| p.add_commands(ext, cx));
        cx.subscribe_in(&palette, window, Self::on_palette_event).detach();
        self.palette = Some(palette);
        cx.notify();
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_OVERLAY=palette`, Text aus `KADRELL_DEV_QUERY` (mit Präfix),
    /// `KADRELL_DEV_DELAY_MS` wartet vorher, bis die Terminals etwas ausgegeben haben.
    pub fn open_palette_from_env(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if std::env::var("KADRELL_DEV_OVERLAY").as_deref() != Ok("palette") {
            return;
        }
        let query = std::env::var("KADRELL_DEV_QUERY").unwrap_or_default();
        let delay = std::env::var("KADRELL_DEV_DELAY_MS").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        if delay == 0 {
            self.open_for_dev(&query, window, cx);
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(delay)).await;
            let _ = this.update_in(cx, |shell, window, cx| shell.open_for_dev(&query, window, cx));
        })
        .detach();
    }

    /// Öffnet die Suche und führt auf Wunsch gleich die Zeile `KADRELL_DEV_ACTIVATE=<n>` aus (⏎ ohne Tastatur).
    fn open_for_dev(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_palette(query, window, cx);
        let row = std::env::var("KADRELL_DEV_ACTIVATE").ok().and_then(|v| v.parse::<usize>().ok());
        if let (Some(row), Some(open)) = (row, self.palette.clone()) {
            open.update(cx, |p, cx| p.activate(row, window, cx));
        }
    }

    /// Schließt die Suche; die Tastatur geht zurück an das Terminal der gewählten Session.
    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.sync(window, cx);
    }

    /// Zeigt nur diese Session (wie `workspace.select([key], add: false)`).
    fn show_only(&mut self, id: (usize, usize), window: &mut Window, cx: &mut Context<Self>) {
        self.selection = Selection::single(id);
        self.apply_selection(window, cx);
    }

    fn on_palette_event(&mut self, _: &Entity<Palette>, ev: &PaletteEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PaletteEvent::Close => self.close_palette(window, cx),
            PaletteEvent::Session(id) => {
                self.close_palette(window, cx);
                self.show_only(*id, window, cx);
            }
            PaletteEvent::Group(g) => {
                self.close_palette(window, cx);
                if self.groups.get(*g).is_some_and(|gr| !gr.sessions.is_empty()) {
                    self.select_group(*g, window, cx);
                }
            }
            PaletteEvent::MoveFocused(g) => {
                self.close_palette(window, cx);
                self.move_focused_to(*g, window, cx);
            }
            PaletteEvent::Layout(mode) => {
                self.close_palette(window, cx);
                self.layout.mode = *mode;
                cx.notify();
            }
            PaletteEvent::Menu(action) => {
                self.close_palette(window, cx);
                window.dispatch_action(action.boxed_clone(), cx);
            }
            PaletteEvent::Hit { id, line, query } => {
                self.close_palette(window, cx);
                self.show_only(*id, window, cx);
                self.reveal_hit(*id, query, *line, cx);
            }
            PaletteEvent::Resume(_) => self.close_palette(window, cx),
            PaletteEvent::Ext { name, id } => {
                self.close_palette(window, cx);
                self.ext_action(name, id);
            }
        }
    }

    /// Scrollt das Terminal zur Treffer-Zeile und öffnet dort seine Suchleiste mit dem Begriff.
    fn reveal_hit(&mut self, id: (usize, usize), query: &str, line: i32, cx: &mut Context<Self>) {
        let pane = self.groups.get(id.0).and_then(|g| g.sessions.get(id.1)).and_then(|s| s.pane.clone());
        if let Some(pane) = pane {
            pane.update(cx, |p, cx| p.reveal_line(query, line, cx));
        }
    }
}
