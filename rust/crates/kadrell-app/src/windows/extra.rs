//! Weiteres Fenster (⌘⇧T): derselbe Satz Sessions, aber eigene Auswahl, eigenes Layout und eigener Fokus. Sessions und
//! Terminals gehören der `Shell` des Hauptfensters; dieses Fenster liest ihren Baum, zeigt die Kacheln seiner Auswahl
//! (Stand in `AppState::wins`, so erreichen ihn auch Steuerkanal und Menü, wenn dieses Fenster das aktuelle ist) und hat
//! wie jedes Fenster eine rechte Seitenleiste und ein Dock der Extensions. Ein Terminal hängt nur in einem Fenster
//! (`PaneOwners`): in dem anderen steht in der Kachel „IN ANDEREM FENSTER“, ein Klick holt es her. Schließen (⌘⇧W oder
//! Fensterknopf) beendet keine Session.
//!
//! Befehle, die dieses Fenster nicht selbst kennt (neue Session, Palette, Einstellungen, Menüpunkte), gibt es als Aktion an
//! das Hauptfenster weiter (`current::forward`); sie wirken dort auf dieses Fenster, weil es das aktuelle ist.

use gpui::{AppContext as _, DragMoveEvent, MouseButton, MouseUpEvent,
    AnyWindowHandle, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render,
    Styled, Window, WindowOptions, div, px, size,
};
use kadrell_core::Settings;

use super::extra_ext::{self, ExtraPanels};
use super::layout::{self, Change, Sides};
use super::model::keys_of;
use super::split;
use super::views::View;
use crate::Shell;
use crate::menus::CloseWindow;
use crate::state::attention::{patch_setting, patch_settings};
use crate::ui::context_menu::{Cmd, ContextMenu, Item, MenuHost, menu_layer, take_menu};
use crate::ui::ext::panel::PanelEvent;
use crate::ui::ext::{FocusRightSidebar, ToggleRightSidebar};
use crate::ui::layout::{LayoutMode, LayoutState, Rect, measure};
use crate::ui::model::{Group, position};
use crate::ui::tile;

pub struct ExtraWindow {
    pub(super) shell: Entity<Shell>,
    pub(super) index: usize,
    pub(super) layout: LayoutState,
    pub(super) idle_focus: FocusHandle,
    /// Aufteilung dieses Fensters: Breite des Baums (`sidebar.width.N`), rechte Seitenleiste, Dock, Baum ein/aus.
    sides: Sides,
    /// Noch nicht gespeicherte Breiten und Höhen, bis die Maus losgelassen wird.
    unsaved: Vec<Change>,
    /// Seitenleiste und Dock der Extensions dieses Fensters.
    panels: ExtraPanels,
    /// Aktionsmenü eines Panels der Extensions (Rechtsklick, Alt+Enter), im Fenster, in dem das Panel steht.
    pub(super) context: Option<ContextMenu>,
    /// Kürzel aus den Einstellungen (`workspace_key` des Hauptfensters).
    pub(super) keys: crate::workspace::keys::Table,
    /// Menüleiste, AUTO und SYNC dieses Fensters (`extra_modes`).
    pub(super) modes: super::extra_modes::Modes,
    pub(super) tree_scroll: gpui::ScrollHandle,
}

impl ExtraWindow {
    /// Öffnet Fenster `index` (aus der Einstellung oder frisch). Auswahl und Layout kommen aus den Schlüsseln mit Suffix.
    pub fn new(shell: Entity<Shell>, index: usize, window: &mut Window, cx: &mut Context<Self>) -> ExtraWindow {
        cx.observe(&shell, |_, _, cx| cx.notify()).detach();
        cx.on_release(|this, cx| this.released(cx)).detach();
        let manager = shell.read(cx).ext.clone();
        let panels = ExtraPanels::new(&manager, cx);
        let mode = shell.update(cx, |sh, _| {
            sh.state.open_view(index);
            sh.win.extra_panels.insert(index, panels.clone());
            let keys = sh.state.wins.get(index).map(|v| v.selected.clone()).unwrap_or_default();
            sh.state.start_now(&keys);
            sh.state.wins.get(index).map(|v| v.mode).unwrap_or_default()
        });
        let sides = Sides::restore(&Settings::load(&shell.read(cx).state.profile.settings_path()), index);
        super::frame::watch(window, cx, shell.read(cx).state.profile.settings_path(), index);
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                let (shell, index) = (this.shell.clone(), this.index);
                shell.update(cx, |sh, cx| sh.note_current(index, cx));
            }
        })
        .detach();
        for panel in [&panels.right, &panels.left] {
            cx.subscribe_in(panel, window, Self::on_panel_event).detach();
        }
        let settings = Settings::load(&shell.read(cx).state.profile.settings_path());
        let layout = LayoutState::from_settings(LayoutMode::from_name(mode.as_str()).unwrap_or(LayoutMode::Grid), &settings);
        let keys = crate::workspace::keys::Table::new(&settings, crate::ui::settings::shortcut::current());
        let me = ExtraWindow { shell, index, layout, idle_focus: cx.focus_handle(), sides, unsaved: Vec::new(), panels, context: None, keys, modes: super::extra_modes::Modes::new(cx), tree_scroll: gpui::ScrollHandle::new() };
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| this.sync_main(cx));
        })
        .detach();
        me
    }

    /// Ansicht dieses Fensters aus dem Zustand der App.
    pub(super) fn view(&self, cx: &gpui::App) -> View {
        self.shell.read(cx).state.wins.get(self.index).cloned().unwrap_or_default()
    }

    /// Schlüssel der sichtbaren Kacheln (die Auswahl, soweit es die Sessions noch gibt).
    pub(super) fn visible_keys(view: &View, groups: &[Group]) -> Vec<String> {
        view.selected.iter().filter(|k| position(groups, k).is_some()).cloned().collect()
    }

    /// Fenster ist zu: Terminals werden frei, die Schlüssel des Fensters verschwinden, die Liste wird gespeichert. Beim
    /// Beenden der App bleibt alles, damit die Fenster nach dem Neustart wiederkommen.
    fn released(&mut self, cx: &mut gpui::App) {
        let index = self.index;
        self.shell.update(cx, |shell, cx| {
            shell.shown.remove(&index);
            shell.win.extra_panels.remove(&index);
            if shell.down {
                return;
            }
            shell.windows.close(index);
            shell.extras.retain(|(i, _)| *i != index);
            // Wie `closeWindow`: danach gilt das zuletzt geöffnete weitere Fenster, sonst das Hauptfenster.
            let fallback = shell.extras.last().map_or(0, |(i, _)| *i);
            shell.state.close_view(index, fallback);
            if shell.win.pin.is_some_and(|p| p.index == index) {
                shell.win.pin = None;
            }
            if shell.win.host == Some(index) {
                shell.win.host = None;
            }
            let keys = keys_of(index);
            for key in &keys {
                shell.state.settings.remove(key);
            }
            let mut changes: Vec<(String, Option<serde_json::Value>)> = keys.into_iter().map(|k| (k, None)).collect();
            changes.push((super::model::OPEN_KEY.to_string(), Some(shell.windows.to_value())));
            patch_settings(&shell.state.profile.settings_path(), changes);
            cx.notify();
        });
    }

    /// Die Shell hält Terminals und Start (`reconcile`), ihr Fenster ist das Hauptfenster: dort die vorgemerkten Starts
    /// ausführen lassen.
    fn sync_main(&self, cx: &mut Context<Self>) {
        let shell = self.shell.clone();
        let Some(main) = shell.read(cx).main_window else { return };
        let _ = cx.update_window(main, |_, window, cx| {
            shell.update(cx, |sh, cx| sh.sync(window, cx));
        });
    }

    /// Ohne (noch vorhandene) Auswahl, etwa weil die Sessions erst nach dem Öffnen des Fensters entstehen: die erste
    /// Session, die das Hauptfenster nicht zeigt. Der Start läuft danach, nicht mitten im Zeichnen.
    fn pick_default(&mut self, cx: &mut Context<Self>) {
        let (shell, index) = (self.shell.clone(), self.index);
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| {
                shell.update(cx, |sh, _| sh.state.repick_view(index));
                this.sync_main(cx);
            });
        })
        .detach();
    }

    /// Nach einer Änderung der Ansicht: Starts ausführen, neu zeichnen.
    pub(super) fn changed(&mut self, cx: &mut Context<Self>) {
        self.sync_main(cx);
        cx.notify();
    }

    /// Klick oder Fokus in eine Kachel: sie wird Fokus und holt sich das Terminal, wenn es woanders hängt.
    pub(super) fn focus_tile(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.index;
        let owned = self.shell.update(cx, |sh, cx| {
            sh.owners.take(key, index);
            sh.state.view_focus(index, key);
            cx.notify();
            sh.panes.get(key).cloned()
        });
        if let Some(pane) = owned {
            pane.read(cx).focus.clone().focus(window, cx);
        }
        cx.notify();
    }

    /// Die Tastatur geht an das Terminal der fokussierten Kachel dieses Fensters zurück.
    fn focus_pane(&self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.view(cx).focus.and_then(|k| self.shell.read(cx).panes.get(&k).cloned());
        match pane {
            Some(p) => p.read(cx).focus.clone().focus(window, cx),
            None => self.idle_focus.focus(window, cx),
        }
    }

    fn on_panel_event(&mut self, _: &Entity<crate::ui::ext::panel::ExtPanel>, ev: &PanelEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PanelEvent::Leave => self.focus_pane(window, cx),
            PanelEvent::Menu { position, tab, actions } => {
                let items = actions.iter().map(|a| Item::Entry(a.label.clone(), Cmd::Ext { tab: tab.clone(), id: a.id.clone() })).collect();
                self.open_menu(*position, items, cx);
            }
        }
    }

    /// Layout dieses Fensters wählen (Menü der Layout-Taste).
    pub(super) fn set_mode(&mut self, mode: LayoutMode, cx: &mut Context<Self>) {
        let (index, mode) = (self.index, kadrell_control::LayoutMode::from_raw(mode.key()).unwrap_or_default());
        self.shell.update(cx, |sh, _| sh.state.view_set_mode(index, mode));
        cx.notify();
    }

    /// Konto wechseln, Auto-Wechsel, Hinzufügen und Verwalten laufen in der Shell; „Verwalten“ öffnet die Einstellungen dort.
    fn run_account(&mut self, cmd: crate::usage::AccountCmd, cx: &mut Context<Self>) {
        let dialog = matches!(cmd, crate::usage::AccountCmd::Manage);
        super::current::on_main(&self.shell, self.index, dialog, cx, move |sh, window, cx| sh.run_account_cmd(cmd, window, cx));
    }

    /// Öffnet das Aktionsmenü eines Panels an dieser Stelle dieses Fensters.
    pub(super) fn open_menu(&mut self, pos: gpui::Point<gpui::Pixels>, items: Vec<Item>, cx: &mut Context<Self>) {
        if items.is_empty() {
            return;
        }
        self.context = Some(ContextMenu::new(pos, items, cx.focus_handle()));
        cx.notify();
    }

    /// ⌘B: Baum dieses Fensters ein/aus.
    fn toggle_tree(&mut self, cx: &mut Context<Self>) {
        self.sides.toggle_tree();
        cx.notify();
    }

    /// Strg+Alt+B: rechte Seitenleiste dieses Fensters ein/aus (gespeichert pro Fenster).
    fn toggle_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (key, value) = self.sides.toggle_right();
        patch_setting(&self.shell.read(cx).state.profile.settings_path(), &key, value);
        if !self.sides.right_visible && self.panels.right.read(cx).has_keyboard(window) {
            self.focus_pane(window, cx);
        }
        cx.notify();
    }

    /// Strg+3: Tastatur in die rechte Seitenleiste dieses Fensters (blendet sie bei Bedarf ein).
    fn focus_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panels.right.read(cx).is_empty() {
            return;
        }
        if !self.sides.right_visible {
            self.toggle_right(window, cx);
        }
        let panel = self.panels.right.clone();
        cx.notify();
        // Das Panel steht erst im nächsten Frame im Baum, ein Fokus davor liefe ins Leere.
        cx.defer_in(window, move |_, window, cx| panel.read(cx).focus_handle(cx).focus(window, cx));
    }

    fn drag_tree(&mut self, ev: &DragMoveEvent<split::TreeDrag>, window: &Window, cx: &mut Context<Self>) {
        let x = (ev.event.position.x - ev.bounds.origin.x).as_f32();
        let change = self.sides.drag_tree(x, window.viewport_size().width.as_f32());
        self.remember(change, cx);
    }

    fn drag_right(&mut self, ev: &DragMoveEvent<split::RightDrag>, window: &Window, cx: &mut Context<Self>) {
        let from_right = (ev.bounds.right() - ev.event.position.x).as_f32();
        let change = self.sides.drag_right(from_right, window.viewport_size().width.as_f32());
        self.remember(change, cx);
    }

    fn drag_dock(&mut self, ev: &DragMoveEvent<split::DockDrag>, cx: &mut Context<Self>) {
        let from_bottom = (ev.bounds.bottom() - ev.event.position.y).as_f32();
        let change = self.sides.drag_dock(from_bottom, ev.bounds.size.height.as_f32());
        self.remember(change, cx);
    }

    fn remember(&mut self, change: Change, cx: &mut Context<Self>) {
        self.unsaved.retain(|(key, _)| *key != change.0);
        self.unsaved.push(change);
        cx.notify();
    }

    /// Maus losgelassen: die gezogenen Breiten und die Höhe in die Einstellungen (`sidebar.width.N` usw.).
    fn save_sides(&mut self, cx: &mut Context<Self>) {
        if self.unsaved.is_empty() {
            return;
        }
        let changes = std::mem::take(&mut self.unsaved).into_iter().map(|(key, value)| (key, Some(value))).collect();
        patch_settings(&self.shell.read(cx).state.profile.settings_path(), changes);
    }

    pub(super) fn cycle_layout(&mut self, cx: &mut Context<Self>) {
        let (index, next) = (self.index, self.layout.mode.next());
        let mode = kadrell_control::LayoutMode::from_raw(next.key()).unwrap_or_default();
        self.shell.update(cx, |sh, _| sh.state.view_set_mode(index, mode));
        cx.notify();
    }

}

impl MenuHost for ExtraWindow {
    fn host_menu(&mut self) -> Option<&mut ContextMenu> {
        self.context.as_mut()
    }

    /// Die Zeilen des Menüs hier: Aktionen der Extensions, Layouts und Konten.
    fn host_run(&mut self, cmd: Cmd, window: &mut Window, cx: &mut Context<Self>) {
        self.host_dismiss(window, cx);
        match cmd {
            Cmd::Ext { tab, id } => self.shell.read(cx).ext_action(&tab, &id),
            Cmd::Layout(mode) => self.set_mode(mode, cx),
            Cmd::Account(cmd) => self.run_account(cmd, cx),
            // Befehle des Baums (Session- und Gruppenmenü): im Hauptfenster, mit diesem Fenster als aktuellem.
            other => super::current::on_main(&self.shell, self.index, false, cx, move |sh, window, cx| sh.run_cmd(other, window, cx)),
        }
    }

    fn host_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if take_menu(&mut self.context, window, cx) {
            cx.notify();
        }
    }
}

impl Focusable for ExtraWindow {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.idle_focus.clone()
    }
}

impl Render for ExtraWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::appearance::window_color::ensure_native(window);
        crate::ui::overlay::sync_area(window, cx);
        self.shell.update(cx, |sh, _| sh.settle_host());
        let t = crate::ui::theme(cx);
        let a = crate::appearance::current(cx);
        self.layout.sync_columns(&self.shell.read(cx).state.settings);
        self.layout.gap = a.gap;
        self.layout.ui_scale = a.scale;
        let index = self.index;
        self.sync_menubar(cx);
        self.take_menu_request(cx);
        self.refresh_auto(window, cx);
        let view = self.view(cx);
        self.layout.mode = LayoutMode::from_name(view.mode.as_str()).unwrap_or(LayoutMode::Grid);
        let all = Self::visible_keys(&view, &self.shell.read(cx).groups);
        let visible = super::extra_keys::zoomed_keys(&all, view.focus.as_deref(), view.zoom);
        // Mit AUTO darf das Fenster leer sein, bis etwas wartet.
        if visible.is_empty() && !self.modes.auto.on {
            self.pick_default(cx);
        }
        // Dem Hauptfenster melden, was hier zu sehen ist: danach steht fest, wer welches Terminal hält.
        let shown = all;
        self.shell.update(cx, |sh, cx| {
            if sh.note_shown(index, shown) {
                cx.notify();
            }
        });
        let bar = self.bar(window, cx);
        let chrome_h = crate::appearance::metrics::bar_height(a.scale) + crate::ui::overlay::title_bar_height();
        let (vw, vh) = (window.viewport_size().width.as_f32(), window.viewport_size().height.as_f32() - chrome_h);
        let tree_w = self.sides.tree_width(vw);
        let tree = self.tree(tree_w, cx);
        let titlebar = crate::ui::bar::titlebar(&self.modes.menubar, window, cx);
        let sync = self.modes.sync;
        let sh = self.shell.read(cx);
        let groups: &[Group] = &sh.groups;
        let ids: Vec<(usize, usize)> = visible.iter().filter_map(|k| position(groups, k)).collect();
        let focus_id = view.focus.as_deref().and_then(|f| position(groups, f)).or_else(|| ids.first().copied());
        let active = focus_id.and_then(|f| ids.iter().position(|i| *i == f)).unwrap_or(0);
        let (frames, _) = self.layout.plan(ids.len(), active);
        let stacked = self.layout.mode == LayoutMode::Stack && ids.len() > 1;
        let tiles: Vec<gpui::AnyElement> = ids
            .iter()
            .enumerate()
            .map(|(i, &(g, s))| {
                let (group, session) = (&groups[g], &groups[g].sessions[s]);
                let focused = Some((g, s)) == focus_id;
                let elsewhere = sh.owners.elsewhere(index, &session.key);
                // Wie im Hauptfenster: bei SYNC tragen alle Kacheln den Ring (`Shell::tile_marks`).
                let marks = tile::Marks { active: focused, ring: focused || (sync && ids.len() > 1), ..Default::default() };
                let el = tile::cell(&tile::Cell { group, session, marks, geo: tile::Geometry::of(frames[i]), body: !stacked || focused, elsewhere, on_icon: None, on_head: None }, cx);
                placed(frames[i], &session.key, el, cx)
            })
            .collect();
        let left = extra_ext::left_column(&self.sides, &self.panels.left, tree, window, vh, &t, cx, cx.listener(|this, ev: &DragMoveEvent<split::DockDrag>, _, cx| this.drag_dock(ev, cx)));
        let right = extra_ext::right_panel(&self.sides, &self.panels.right, window, &t, cx);
        let right_w = self.sides.right_width(vw);
        let work = div()
            .id("extra-workspace")
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .bg(a.panel_fill(t.bg))
            .child(measure(self.layout.measured.clone(), cx.entity_id()))
            .children(tiles);
        let has_right = right.is_some();
        let root = div()
            .id("extra-window")
            .track_focus(&self.idle_focus)
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(crate::theme::FONT)
            .text_size(px(a.s(12.)))
            .line_height(gpui::relative(crate::ui::overlay::LINE_HEIGHT))
            .on_action(cx.listener(|_, _: &CloseWindow, window, _| window.remove_window()))
            .on_action(cx.listener(|this, _: &crate::menus::NextLayout, _, cx| this.cycle_layout(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::ZoomTile, _, cx| this.toggle_zoom(cx)))
            .on_action(cx.listener(|this, _: &crate::menus::NextSession, window, cx| this.step_focus(1, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::PrevSession, window, cx| this.step_focus(-1, window, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::MoreColumns, _, cx| this.step_columns(1, cx)))
            .on_action(cx.listener(|this, _: &crate::menus::FewerColumns, _, cx| this.step_columns(-1, cx)))
            .on_action(cx.listener(|_, _: &crate::menus::Minimize, window, _| window.minimize_window()))
            .on_action(cx.listener(|_, _: &crate::menus::Zoom, window, _| window.zoom_window()))
            .capture_key_down(cx.listener(|this, ev: &gpui::KeyDownEvent, window, cx| this.on_key(ev, window, cx)))
            .on_action(cx.listener(|this, _: &super::ToggleSidebar, _, cx| this.toggle_tree(cx)))
            .on_action(cx.listener(|this, _: &ToggleRightSidebar, window, cx| this.toggle_right(window, cx)))
            .on_action(cx.listener(|this, _: &FocusRightSidebar, window, cx| this.focus_right(window, cx)))
            .on_drag_move::<split::TreeDrag>(cx.listener(|this, ev: &DragMoveEvent<split::TreeDrag>, window, cx| this.drag_tree(ev, window, cx)))
            .on_drag_move::<split::RightDrag>(cx.listener(|this, ev: &DragMoveEvent<split::RightDrag>, window, cx| this.drag_right(ev, window, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.save_sides(cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.save_sides(cx)))
            .relative();
        let menu = menu_layer(self.context.as_mut(), window, cx);
        // Dialoge, die dieses Fenster aufgerufen hat (`ui::overlay_host`): die Maße kommen aus der Fläche dieses Fensters.
        let layers = self.shell.read(cx).overlay_layers(index);
        let inner = super::dialogs::attach_commands(root, cx)
            .children((!self.sides.tree_hidden).then_some(left))
            .child(work)
            .children(right)
            .children((!self.sides.tree_hidden).then(|| split::grip("extra-grip-tree", split::TreeDrag, true).left(px(tree_w + 1. - layout::GRAB / 2.))))
            .children(has_right.then(|| split::grip("extra-grip-right", split::RightDrag, true).left(px(vw - right_w - 1. - layout::GRAB / 2.))));
        // Dieselbe Hülle wie im Hauptfenster: Schrift und Größe gelten auch für die Leiste.
        let frame = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(crate::theme::FONT)
            .text_size(px(12.))
            .line_height(gpui::relative(crate::ui::overlay::LINE_HEIGHT));
        crate::ui::menubar::attach_hooks(frame, &self.modes.menubar).children(titlebar).child(bar).child(inner).children(layers).children(menu)
    }
}

impl ExtraWindow {
    /// Rechtsklick im Baum dieses Fensters: das Menü der Shell-Anfrage öffnet hier.
    fn take_menu_request(&mut self, cx: &mut Context<Self>) {
        let index = self.index;
        let request = self.shell.update(cx, |sh, _| match sh.win.menu_request.take() {
            Some(r) if r.index == index => Some(r),
            other => {
                sh.win.menu_request = other;
                None
            }
        });
        if let Some(r) = request {
            self.open_menu(r.at, r.items, cx);
        }
    }
}

/// Kachel an ihrem Platz; ein Klick macht sie zum Fokus.
fn placed(r: Rect, key: &str, tile: gpui::AnyElement, cx: &Context<ExtraWindow>) -> gpui::AnyElement {
    let key = key.to_string();
    div()
        .id(gpui::ElementId::Name(format!("extra-tile-{key}").into()))
        .absolute()
        .left(px(r.x))
        .top(px(r.y))
        .w(px(r.w))
        .h(px(r.h))
        .capture_any_mouse_down(cx.listener(move |this, _, window, cx| this.focus_tile(&key, window, cx)))
        .child(tile)
        .into_any_element()
}

/// Öffnet das Fenster mit dieser Nummer. Gibt das Handle zurück, `None` bei Fehlschlag.
pub fn open(shell: &Entity<Shell>, index: usize, title: String, cascade: Option<super::frame::Frame>, cx: &mut gpui::App) -> Option<AnyWindowHandle> {
    let saved = Settings::load(&shell.read(cx).state.profile.settings_path()).get(&super::frame::frame_key(index)).cloned();
    let bounds = super::frame::bounds_for(index, saved.as_ref(), cascade, cx)
        .unwrap_or_else(|| gpui::Bounds::centered(None, crate::ui::overlay::window_size(), cx));
    let options = WindowOptions {
        // Wie das Hauptfenster: unter Windows eigene Titelzeile mit Menü und Fensterknöpfen, auf dem Mac die des Systems.
        titlebar: Some(gpui::TitlebarOptions { title: Some(title.into()), appears_transparent: !cfg!(target_os = "macos"), traffic_light_position: None }),
        window_min_size: Some(size(px(super::frame::MIN_SIZE.0), px(super::frame::MIN_SIZE.1))),
        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
        ..Default::default()
    };
    let shell = shell.clone();
    cx.open_window(options, move |window, cx| cx.new(|cx| ExtraWindow::new(shell, index, window, cx))).ok().map(Into::into)
}
