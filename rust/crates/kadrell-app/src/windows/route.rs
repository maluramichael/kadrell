//! Der Baum eines weiteren Fensters ist derselbe wie im Hauptfenster (`ui::sidebar`), nur mit dem Zustand dieses Fensters:
//! Auswahl und Fokus aus seiner `View`, eigene Breite, eigenes Scrollen. Die Handler des Baums laufen auf der `Shell`;
//! hier steht, wie sie für ein weiteres Fenster ankommen: Befehle, Dialoge und Ablagen im Hauptfenster (`current::on_main`,
//! dort ist `current` das weitere Fenster), Klicks und Menüs im Fenster selbst.

use std::rc::Rc;

use gpui::{App, AppContext as _, Context, Pixels, Point, Window};

use super::current::on_main;
use crate::Shell;
use crate::ui::context_menu::Item;
use crate::ui::layout::Selection;
use crate::ui::model::{Group, flat_index, position};

/// Handler eines Ereignisses `E`, wie GPUI ihn an Elemente hängt.
pub type Handler<E> = Box<dyn Fn(&E, &mut Window, &mut App)>;

/// Handler für den Baum: im Hauptfenster (`win` ist `None`) direkt auf der `Shell`, in einem weiteren Fenster im Hauptfenster
/// mit diesem Fenster als aktuellem (Dialoge und Terminals gehören dem Hauptfenster).
pub fn routed<E: Clone + 'static>(
    cx: &mut Context<Shell>,
    win: Option<usize>,
    f: impl Fn(&mut Shell, &E, &mut Window, &mut Context<Shell>) + 'static,
) -> Handler<E> {
    let Some(index) = win else { return Box::new(cx.listener(f)) };
    let (shell, f) = (cx.entity(), Rc::new(f));
    Box::new(move |ev, _, app| {
        let (ev, f) = (ev.clone(), f.clone());
        on_main(&shell, index, false, app, move |sh, window, scx| f(sh, &ev, window, scx));
    })
}

/// Auswahl des Baums aus der Auswahl eines Fensters (Schlüssel). Fokus ohne Treffer: die erste markierte Session.
pub fn selection_of(groups: &[Group], selected: &[String], focus: Option<&str>) -> Selection {
    let flat = flat_index(groups);
    let mut items: Vec<(usize, usize)> = selected.iter().filter_map(|k| position(groups, k)).collect();
    items.sort_by_key(|i| flat.iter().position(|f| f == i).unwrap_or(usize::MAX));
    let focus = focus.and_then(|f| position(groups, f)).filter(|f| items.contains(f)).or_else(|| items.first().copied());
    let focus = focus.unwrap_or((0, 0));
    Selection { items, focus, anchor: focus }
}

/// Wunsch eines weiteren Fensters, ein Menü bei sich zu öffnen (Rechtsklick im Baum). Das Fenster holt ihn im nächsten Bild.
pub struct MenuRequest {
    pub index: usize,
    pub at: Point<Pixels>,
    pub items: Vec<Item>,
}

impl Shell {
    /// Gruppierung und Sortierung des Baums (Einstellungen, in jedem Fenster gleich), für GRP und SORT der Leiste.
    pub(crate) fn tree_look(&self) -> (bool, kadrell_core::SidebarSort) {
        (self.state.grouped(), self.state.sort())
    }

    /// Rechtsklick im Baum: das Menü öffnet im Hauptfenster oder in dem weiteren Fenster, in dem geklickt wurde.
    pub(crate) fn open_menu_for(&mut self, win: Option<usize>, at: Point<Pixels>, items: Vec<Item>, cx: &mut Context<Self>) {
        match win {
            Some(index) => self.request_window_menu(index, at, items, cx),
            None => self.open_context(at, items, cx),
        }
    }

    /// Rechtsklick im Baum eines weiteren Fensters: das Menü gehört diesem Fenster.
    pub(crate) fn request_window_menu(&mut self, index: usize, at: Point<Pixels>, items: Vec<Item>, cx: &mut Context<Self>) {
        if !items.is_empty() {
            self.win.menu_request = Some(MenuRequest { index, at, items });
            cx.notify();
        }
    }

    /// Klick im Baum von Fenster `index`: nur diese Session, Strg nimmt sie dazu oder weg, Umschalt markiert den Bereich.
    pub(crate) fn click_session_in(&mut self, index: usize, g: usize, s: usize, m: gpui::Modifiers, cx: &mut Context<Self>) {
        let view = self.state.wins.get(index).cloned().unwrap_or_default();
        let current = selection_of(&self.groups, &view.selected, view.focus.as_deref());
        let flat = flat_index(&self.groups);
        let next = crate::ui::tree_rules::click_selection(&current, (g, s), m.shift, m.secondary(cx), &flat);
        self.select_in(index, &next, cx);
    }

    /// Klick auf eine Gruppenzeile im Baum von Fenster `index`: alle ihre Sessions, mit Umschalt oder Strg dazu.
    pub(crate) fn click_group_in(&mut self, index: usize, g: usize, m: gpui::Modifiers, cx: &mut Context<Self>) {
        let keys: Vec<String> = self.groups[g].sessions.iter().map(|s| s.key.clone()).collect();
        self.state.view_select(index, &keys, m.shift || m.secondary(cx));
        self.state.view_clear_zoom(index);
        self.sync_in_main(cx);
        cx.notify();
    }

    /// Nach einer Auswahl in einem weiteren Fenster: die Shell hält Terminals und Start, ihr Fenster ist das Hauptfenster.
    /// Der Abgleich läuft dort, nicht mitten im Klick.
    fn sync_in_main(&self, cx: &mut Context<Self>) {
        let Some(main) = self.main_window else { return };
        cx.spawn(async move |this, cx| {
            let _ = cx.update_window(main, |_, window, app| {
                let _ = this.update(app, |sh, scx| sh.sync(window, scx));
            });
        })
        .detach();
    }

    fn select_in(&mut self, index: usize, next: &Selection, cx: &mut Context<Self>) {
        let key = |id: &(usize, usize)| self.groups[id.0].sessions[id.1].key.clone();
        let keys: Vec<String> = next.items.iter().map(key).collect();
        let focus = keys.iter().find(|k| **k == key(&next.focus)).cloned();
        self.state.view_select(index, &keys, false);
        if let Some(focus) = focus {
            self.state.view_focus(index, &focus);
        }
        self.state.view_clear_zoom(index);
        self.sync_in_main(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::model::test_group;

    fn groups() -> Vec<Group> {
        vec![test_group("a", &["a1", "a2"]), test_group("b", &["b1"])]
    }

    fn keys(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_tree_marks_what_the_window_shows() {
        let g = groups();
        let sel = selection_of(&g, &keys(&["b1", "a2"]), Some("b1"));
        assert_eq!(sel.items, [(0, 1), (1, 0)], "in Baumreihenfolge");
        assert_eq!(sel.focus, (1, 0));
    }

    #[test]
    fn focus_outside_the_selection_falls_back_to_the_first() {
        let g = groups();
        assert_eq!(selection_of(&g, &keys(&["a2"]), Some("b1")).focus, (0, 1));
        assert!(selection_of(&g, &keys(&["gone"]), None).items.is_empty());
    }
}
