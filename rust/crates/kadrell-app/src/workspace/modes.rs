//! Die Betriebsarten der Arbeitsfläche an der Shell: Vorschau (⌥J/⌥K), Sync (⌥I), Auto (AUTO), Tastatur im Baum
//! (⌘1) und das seitliche Scrollen. Die Regeln selbst stehen in `preview`, `auto`, `sync` und `tree_keys`.

use gpui::{Context, Keystroke, ScrollEvent, Window, px};
use kadrell_core::SessionStatus;
use kadrell_term::Forward;
use serde_json::json;

use super::actions::AUTO_KEY;
use super::auto::Filter;
use super::preview::{Ended, Keyboard, neighbor_key};
use super::sync;
use super::tree_keys::{self, TreeKey};
use crate::Shell;
use crate::state::AppState;
use crate::ui::layout::{Id, LayoutMode, Selection};
use crate::ui::model::{flat_index, position};
use crate::ui::settings::shortcut::{self, KeyInput};
use crate::ui::tile::Marks;

fn matches_auto(state: &AppState, filter: &Filter, key: &str) -> bool {
    match state.display_status(key) {
        Some(SessionStatus::Waiting) => filter.waiting,
        Some(SessionStatus::Running) => filter.running,
        _ => false,
    }
}

impl Shell {
    // --- Anzeige ---

    /// Die Kachel der Vorschau, falls eine läuft und die Session noch da ist.
    pub(crate) fn preview_id(&self) -> Option<Id> {
        self.ws.preview.shown().and_then(|k| position(&self.groups, k))
    }

    /// Wie die Kachel `id` aussieht: Tastaturkachel, Fokuskante (im Sync-Modus alle), Zoom-Badge, Vorschau.
    pub(crate) fn tile_marks(&self, id: Id, visible: usize) -> Marks {
        let active = Some(id) == self.selected && self.preview_id().is_none();
        Marks {
            active,
            ring: (active || self.ws.sync) && visible > 1,
            zoomed: self.state.zoom && self.tiles().len() > 1,
            preview: self.preview_id() == Some(id),
        }
    }

    // --- Vorschau ---

    /// Sichtbare Sessions des Baums in Reihenfolge, ohne die zugeklappter Gruppen.
    fn tree_keys(&self) -> Vec<String> {
        self.groups.iter().filter(|g| !g.collapsed).flat_map(|g| g.sessions.iter().map(|s| s.key.clone())).collect()
    }

    fn keyboard_owner(&self) -> Keyboard {
        if self.ws.tree_focus { Keyboard::Tree } else { Keyboard::Tile }
    }

    /// ⌥J / ⌥K: im Baum weiterblättern und die Session dort allein zeigen. Die Tastatur verlässt das (verdeckte)
    /// Terminal der Fokus-Kachel, damit nichts bei einem unsichtbaren Claude ankommt.
    pub(crate) fn preview_step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.tree_keys();
        let focus = self.state.focused.clone();
        let owner = self.keyboard_owner();
        if self.ws.preview.step(&ids, focus.as_deref(), owner, delta).is_some() {
            self.idle_focus.focus(window, cx);
            cx.notify();
        }
    }

    /// ⏎ übernimmt die Vorschau als Auswahl, Esc zeigt wieder die alte. Nur ohne Modifier.
    pub(crate) fn preview_key(&mut self, input: &KeyInput, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let plain = shortcut::modifier_bits(input, shortcut::current()) == 0;
        if !self.ws.preview.is_active() || !plain || !matches!(input.key.as_str(), "enter" | "escape") {
            return false;
        }
        self.end_preview(input.key == "enter", window, cx);
        true
    }

    pub(crate) fn end_preview(&mut self, commit: bool, window: &mut Window, cx: &mut Context<Self>) {
        match self.ws.preview.end(commit) {
            None => {}
            Some(Ended::Commit(key)) => {
                if let Some(id) = position(&self.groups, &key) {
                    self.selection = Selection::single(id);
                    self.state.zoom = false;
                    self.apply_selection(window, cx);
                }
            }
            Some(Ended::Cancel(Keyboard::Tile)) => self.sync(window, cx),
            Some(Ended::Cancel(Keyboard::Tree)) => cx.notify(),
        }
    }

    // --- Sync ---

    /// ⌥I: Eingaben im Fokus-Terminal gehen an alle Kacheln. Bewusst nicht gespeichert.
    pub(crate) fn toggle_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ws.sync = !self.ws.sync;
        self.sync(window, cx);
    }

    /// Sync: dieselbe Taste an alle anderen Kacheln. Nur wenn das Fokus-Terminal selbst die Tastatur hat, nicht bei
    /// Dialogen, im Baum oder in der Vorschau.
    pub(crate) fn forward_sync(&mut self, ks: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ws.sync {
            return;
        }
        let Some(source) = self.state.focused.clone() else { return };
        let owns_keyboard = self.panes.get(&source).is_some_and(|p| p.read(cx).focus.is_focused(window));
        let forward = kadrell_term::forward_of(ks);
        if !owns_keyboard || forward == Forward::Ignore {
            return;
        }
        let state = &self.state;
        let targets = sync::targets(true, &self.tile_keys(), &source, |k| state.term(k).is_some());
        for key in targets {
            match &forward {
                Forward::Bytes(bytes) => self.state.term(&key).iter().for_each(|t| t.write(bytes)),
                Forward::Paste => self.panes.get(&key).iter().for_each(|p| p.update(cx, |p, cx| p.paste(cx))),
                Forward::Ignore => {}
            }
        }
    }

    // --- Auto ---

    /// AUTO: aus der Auswahl (oder allen Sessions) nur wartende bzw. arbeitende Kacheln zeigen.
    pub(crate) fn toggle_auto(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on = !self.ws.auto.on;
        self.ws.auto.set(on);
        self.state.save_workspace_setting(AUTO_KEY, Some(json!(on)));
        self.state.zoom = false;
        self.refresh_auto();
        self.sync(window, cx);
    }

    /// Woraus der Auto-Modus filtert: je nach Einstellung alle Sessions in Baumreihenfolge oder nur die Auswahl.
    fn auto_pool(&self, filter: &Filter) -> Vec<String> {
        if filter.all_sessions {
            flat_index(&self.groups).iter().map(|&(g, s)| self.groups[g].sessions[s].key.clone()).collect()
        } else {
            self.selection.items.iter().map(|&(g, s)| self.groups[g].sessions[s].key.clone()).collect()
        }
    }

    /// Gleicht den Auto-Modus mit dem Zustand ab (Status, Nachlauf, Fokus). Baut die Ansicht neu, wenn sich etwas ändert.
    pub(crate) fn refresh_auto(&mut self) {
        self.follow_control_layout();
        if !self.ws.auto.on {
            return;
        }
        let filter = Filter::from_settings(&self.state.settings);
        let pool = self.auto_pool(&filter);
        let state = &self.state;
        let mut changed = self.ws.auto.update(&pool, |k| matches_auto(state, &filter, k), crate::state::now_secs());
        let focus = self.state.focused.clone();
        if let Some(key) = self.ws.auto.refocus(focus.as_deref(), self.ws.preview.is_active()) {
            self.state.focus_any(&key);
            changed = true;
        }
        if changed {
            self.rebuild();
        }
    }

    /// `kadrell layout` ändert nur den Zustand: die Ansicht zieht nach.
    fn follow_control_layout(&mut self) {
        let mode = crate::ui_layout(self.state.layout);
        if mode != self.layout.mode {
            self.layout.mode = mode;
            self.layout.reveal = true;
        }
    }

    /// Takt für den Nachlauf: ohne Ereignis (kein Poll) muss die Kachel nach 3 s trotzdem verschwinden.
    pub(crate) fn start_auto_tick(&self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                let alive = this.update_in(cx, |shell, window, cx| {
                    if shell.ws.auto.lingering() {
                        let before = shell.ws.auto.shown.clone();
                        shell.refresh_auto();
                        if shell.ws.auto.shown != before {
                            shell.sync(window, cx);
                        }
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    // --- Tastatur im Baum ---

    /// ⌘1: die Tastatur gehört dem Baum, ↑↓ wählt die Session, ohne dass Tasten bei Claude ankommen.
    pub(crate) fn focus_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ws.tree_focus = true;
        self.idle_focus.focus(window, cx);
        cx.notify();
    }

    /// ⌘2: die Tastatur zurück an die Fokus-Kachel; ist rechts nichts offen, die erste Session im Baum.
    pub(crate) fn focus_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ws.tree_focus = false;
        if self.selected.is_some() {
            self.sync(window, cx);
        } else if !self.jump(0, window, cx) {
            cx.notify();
        }
    }

    /// Eine Baumtaste. `true`: verbraucht.
    pub(crate) fn tree_key(&mut self, input: &KeyInput, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(key) = tree_keys::classify(input, shortcut::current()) else { return false };
        match key {
            TreeKey::Step(d) => self.tree_step(d, window, cx),
            TreeKey::Collapse => self.tree_fold(true, cx),
            TreeKey::Expand => self.tree_fold(false, cx),
            TreeKey::ContextMenu => self.show_context_menu_for_focus(window, cx),
            TreeKey::Open => self.focus_workspace(window, cx),
            TreeKey::PassToSession => self.pass_to_session(window, cx),
            TreeKey::Close => self.remove_focused(window, cx),
            TreeKey::Move(d) => self.tree_move(d, window, cx),
        }
        true
    }

    /// ↑↓: die nächste Session zeigen, die Tastatur bleibt im Baum.
    fn tree_step(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let focus = self.state.focused.clone();
        let Some(next) = neighbor_key(&self.tree_keys(), focus.as_deref(), delta) else { return };
        let Some(id) = position(&self.groups, &next) else { return };
        self.selection = Selection::single(id);
        self.state.zoom = false;
        self.ws.preview.clear();
        self.commit_selection(window, cx);
    }

    /// ← klappt die Gruppe der Fokus-Session zu, → auf.
    fn tree_fold(&mut self, collapse: bool, cx: &mut Context<Self>) {
        let Some((g, _)) = self.selected else { return };
        if self.groups[g].id.is_empty() || self.groups[g].collapsed == collapse {
            return;
        }
        self.toggle_group(g, cx);
    }

    /// ⌃⏎ (Mac) bzw. Alt+⏎ (Windows): die Tastatur zurück an die Fokus-Session, ⏎ geht mit.
    fn pass_to_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_workspace(window, cx);
        if let Some(term) = self.state.focused.clone().and_then(|k| self.state.term(&k)) {
            term.write(b"\r");
        }
    }

    /// ⌥⌘↑/↓: die Fokus-Session im Baum verschieben, gruppiert innerhalb ihrer Gruppe, flach baumweit.
    fn tree_move(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.state.focused.clone() else { return };
        let ids: Vec<String> = if self.state.grouped() {
            self.state.store.groups.group_for_session(&key).map(|g| g.session_ids.clone()).unwrap_or_default()
        } else {
            self.tree_keys()
        };
        let Some(at) = ids.iter().position(|k| *k == key) else { return };
        let Some(target) = at.checked_add_signed(step).and_then(|i| ids.get(i)).cloned() else { return };
        self.move_session_to(&key, &target);
        self.sync(window, cx);
    }

    /// ⌥⌘ in der Baumsortierung: aus → A–Z → Status → aus (`cycleSort`).
    pub(crate) fn cycle_sort(&mut self, cx: &mut Context<Self>) {
        use kadrell_core::SidebarSort;
        let next = if self.state.sort() == SidebarSort::Off { SidebarSort::Alpha } else { SidebarSort::Status };
        self.toggle_sort(next, cx);
    }

    // --- Seitlich scrollen ---

    /// Layout Scrollen: seitliches Wischen (bzw. Umschalt + Mausrad) über der Arbeitsfläche verschiebt die Spalten,
    /// nicht das Terminal. `true`: verbraucht.
    pub(crate) fn wheel_scroll(&mut self, ev: &ScrollEvent, cx: &mut Context<Self>) -> bool {
        if self.layout.mode != LayoutMode::Scroll || self.state.zoom || self.ws.preview.is_active() || self.modal_open() {
            return false;
        }
        let delta = ev.delta.pixel_delta(px(20.));
        let (dx, dy) = (delta.x.as_f32(), delta.y.as_f32());
        let along = if dx.abs() > dy.abs() { dx } else if ev.modifiers.shift { dy } else { return false };
        self.layout.scroll_by(-along);
        cx.notify();
        true
    }
}
