//! Weiteres Fenster (⌘⇧T): derselbe Satz Sessions, aber eigene Auswahl, eigenes Layout und eigener Fokus. Sessions und
//! Terminals gehören der `Shell` des Hauptfensters; dieses Fenster liest ihren Baum, hält nur seine Auswahl (nach
//! Schlüsseln, damit Umsortieren sie nicht verschiebt) und zeigt die Kacheln. Ein Terminal hängt nur in einem Fenster
//! (`PaneOwners`): in dem anderen steht in der Kachel „IN ANDEREM FENSTER“, ein Klick holt es her. Schließen (⌘⇧W oder
//! Fensterknopf) beendet keine Session.

use gpui::{AppContext as _, DragMoveEvent, MouseButton, MouseUpEvent,
    AnyWindowHandle, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, Role,
    StatefulInteractiveElement, Styled, Window, WindowOptions, div, px, size,
};
use kadrell_core::Settings;
use serde_json::json;

use super::model::{key_for, keys_of};
use crate::Shell;
use super::layout::{self, Change, Sides};
use super::split;
use crate::menus::CloseWindow;
use crate::state::Run;
use crate::state::attention::{patch_setting, patch_settings};
use crate::ui::layout::{LayoutMode, LayoutState, Rect, measure};
use crate::ui::model::{Group, position};
use crate::ui::tile::{self, TileStyle};

pub struct ExtraWindow {
    shell: Entity<Shell>,
    index: usize,
    selected: Vec<String>,
    focus: Option<String>,
    layout: LayoutState,
    idle_focus: FocusHandle,
    /// Aufteilung dieses Fensters: Breite des Baums (`sidebar.width.N`), Baum ein/aus.
    sides: Sides,
    /// Noch nicht gespeicherte Breite, bis die Maus losgelassen wird.
    unsaved: Option<Change>,
}

impl ExtraWindow {
    /// Öffnet Fenster `index` (aus der Einstellung oder frisch). Auswahl und Layout kommen aus den Schlüsseln mit Suffix.
    pub fn new(shell: Entity<Shell>, index: usize, window: &mut Window, cx: &mut Context<Self>) -> ExtraWindow {
        cx.observe(&shell, |_, _, cx| cx.notify()).detach();
        cx.on_release(|this, cx| this.released(cx)).detach();
        let (selected, focus, mode) = this_state(&shell, index, cx);
        let sides = Sides::restore(&Settings::load(&shell.read(cx).state.profile.settings_path()), index);
        super::frame::watch(window, cx, shell.read(cx).state.profile.settings_path(), index);
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                let (shell, index) = (this.shell.clone(), this.index);
                shell.update(cx, |sh, cx| sh.note_current(index, cx));
            }
        })
        .detach();
        let me = ExtraWindow { shell, index, selected, focus, layout: LayoutState::new(mode), idle_focus: cx.focus_handle(), sides, unsaved: None };
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| this.start_missing(cx));
        })
        .detach();
        me
    }

    /// Schlüssel der sichtbaren Kacheln (die Auswahl, soweit es die Sessions noch gibt).
    fn visible_keys(&self, groups: &[Group]) -> Vec<String> {
        self.selected.iter().filter(|k| position(groups, k).is_some()).cloned().collect()
    }

    /// Fenster ist zu: Terminals werden frei, die Schlüssel des Fensters verschwinden, die Liste wird gespeichert. Beim
    /// Beenden der App bleibt alles, damit die Fenster nach dem Neustart wiederkommen.
    fn released(&mut self, cx: &mut gpui::App) {
        let index = self.index;
        self.shell.update(cx, |shell, cx| {
            shell.shown.remove(&index);
            if shell.down {
                return;
            }
            shell.windows.close(index);
            let path = shell.state.profile.settings_path();
            let mut changes: Vec<(String, Option<serde_json::Value>)> = keys_of(index).into_iter().map(|k| (k, None)).collect();
            changes.push((super::model::OPEN_KEY.to_string(), Some(shell.windows.to_value())));
            patch_settings(&path, changes);
            shell.extras.retain(|(i, _)| *i != index);
            cx.notify();
        });
    }

    /// Sessions der Auswahl ohne Prozess starten lassen: die Shell hält Terminals und Start (`reconcile`), ihr Fenster
    /// ist das Hauptfenster.
    fn start_missing(&self, cx: &mut Context<Self>) {
        let keys = self.selected.clone();
        let shell = self.shell.clone();
        let Some(main) = shell.read(cx).main_window else { return };
        let _ = cx.update_window(main, |_, window, cx| {
            shell.update(cx, |sh, cx| {
                for key in &keys {
                    if sh.state.run(key) == Run::NotStarted {
                        let _ = sh.state.request_start(key);
                    }
                }
                sh.sync(window, cx);
            });
        });
    }

    /// Ohne (noch vorhandene) Auswahl, etwa weil die Sessions erst nach dem Öffnen des Fensters entstehen: die erste
    /// Session, die das Hauptfenster nicht zeigt. Der Start läuft danach, nicht mitten im Zeichnen.
    fn pick_default(&mut self, cx: &mut Context<Self>) {
        let (selected, focus, _) = this_state(&self.shell, self.index, cx);
        if selected.is_empty() {
            return;
        }
        self.selected = selected;
        self.focus = focus;
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| {
                this.save(cx);
                this.start_missing(cx);
            });
        })
        .detach();
    }

    fn save(&self, cx: &mut Context<Self>) {
        let path = self.shell.read(cx).state.profile.settings_path();
        patch_setting(&path, &key_for("workspace.selected", self.index), json!(self.selected));
        patch_setting(&path, &key_for("workspace.selected.focus", self.index), json!(self.focus));
        patch_setting(&path, &key_for("workspace.mode", self.index), json!(self.layout.mode.key()));
    }

    fn apply_selection(&mut self, cx: &mut Context<Self>) {
        self.save(cx);
        self.start_missing(cx);
        cx.notify();
    }

    /// Klick im Baum: nur diese Session, Strg/Cmd nimmt sie dazu oder weg.
    fn click_session(&mut self, key: String, add: bool, cx: &mut Context<Self>) {
        if add && self.selected.contains(&key) && self.selected.len() > 1 {
            self.selected.retain(|k| *k != key);
            if self.focus.as_deref() == Some(key.as_str()) {
                self.focus = self.selected.last().cloned();
            }
        } else if add {
            if !self.selected.contains(&key) {
                self.selected.push(key.clone());
            }
            self.focus = Some(key);
        } else {
            self.selected = vec![key.clone()];
            self.focus = Some(key);
        }
        self.apply_selection(cx);
    }

    /// Klick oder Fokus in eine Kachel: sie wird Fokus und holt sich das Terminal, wenn es woanders hängt.
    fn focus_tile(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.focus = Some(key.to_string());
        let index = self.index;
        let owned = self.shell.update(cx, |sh, cx| {
            sh.owners.take(key, index);
            cx.notify();
            sh.panes.get(key).cloned()
        });
        if let Some(pane) = owned {
            pane.read(cx).focus.clone().focus(window, cx);
        }
        self.save(cx);
        cx.notify();
    }

    /// ⌘B: Baum dieses Fensters ein/aus.
    fn toggle_tree(&mut self, cx: &mut Context<Self>) {
        self.sides.toggle_tree();
        cx.notify();
    }

    fn drag_tree(&mut self, ev: &DragMoveEvent<split::TreeDrag>, window: &Window, cx: &mut Context<Self>) {
        let x = (ev.event.position.x - ev.bounds.origin.x).as_f32();
        self.unsaved = Some(self.sides.drag_tree(x, window.viewport_size().width.as_f32()));
        cx.notify();
    }

    /// Maus losgelassen: die gezogene Breite in die Einstellungen (`sidebar.width.N`).
    fn save_sides(&mut self, cx: &mut Context<Self>) {
        let Some((key, value)) = self.unsaved.take() else { return };
        let path = self.shell.read(cx).state.profile.settings_path();
        patch_setting(&path, &key, value);
    }

    fn cycle_layout(&mut self, cx: &mut Context<Self>) {
        self.layout.mode = self.layout.mode.next();
        self.save(cx);
        cx.notify();
    }
}

/// Gespeicherte Auswahl, Fokus und Layout dieses Fensters; ohne sie die erste Session, die das Hauptfenster nicht zeigt.
fn this_state(shell: &Entity<Shell>, index: usize, cx: &gpui::App) -> (Vec<String>, Option<String>, LayoutMode) {
    let sh = shell.read(cx);
    let s: &Settings = &sh.state.settings;
    let known = |k: &String| sh.state.session(k).is_some();
    let mut selected: Vec<String> = s.strings_or(&key_for("workspace.selected", index)).into_iter().filter(known).collect();
    if selected.is_empty() {
        let order = sh.state.ordered_keys();
        let pick = order.iter().find(|k| !sh.state.selected.contains(k)).or(order.first());
        selected = pick.cloned().into_iter().collect();
    }
    let focus = s.opt_string(&key_for("workspace.selected.focus", index)).filter(|f| selected.contains(f)).or_else(|| selected.first().cloned());
    let mode = LayoutMode::from_name(&s.string_or(&key_for("workspace.mode", index), "grid")).unwrap_or(LayoutMode::Grid);
    (selected, focus, mode)
}

impl Focusable for ExtraWindow {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.idle_focus.clone()
    }
}

impl Render for ExtraWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = crate::ui::theme(cx);
        let a = crate::appearance::current(cx);
        self.layout.gap = a.gap;
        self.layout.ui_scale = a.scale;
        let index = self.index;
        let mut visible = self.visible_keys(&self.shell.read(cx).groups);
        if visible.is_empty() {
            self.pick_default(cx);
            visible = self.visible_keys(&self.shell.read(cx).groups);
        }
        // Dem Hauptfenster melden, was hier zu sehen ist: danach steht fest, wer welches Terminal hält.
        let shown = visible.clone();
        self.shell.update(cx, |sh, cx| {
            if sh.note_shown(index, shown) {
                cx.notify();
            }
        });
        let sh = self.shell.read(cx);
        let groups: &[Group] = &sh.groups;
        let ids: Vec<(usize, usize)> = visible.iter().filter_map(|k| position(groups, k)).collect();
        let focus_id = self.focus.as_deref().and_then(|f| position(groups, f)).or_else(|| ids.first().copied());
        let active = focus_id.and_then(|f| ids.iter().position(|i| *i == f)).unwrap_or(0);
        let (frames, _) = self.layout.plan(ids.len(), active);
        let style = TileStyle::for_count(ids.len());
        let stacked = self.layout.mode == LayoutMode::Stack && ids.len() > 1;
        let tiles: Vec<gpui::AnyElement> = ids
            .iter()
            .enumerate()
            .map(|(i, &(g, s))| {
                let (group, session) = (&groups[g], &groups[g].sessions[s]);
                let focused = Some((g, s)) == focus_id;
                let elsewhere = sh.owners.elsewhere(index, &session.key);
                let marks = tile::Marks { active: focused, ring: focused, ..Default::default() };
                let el = tile::tile(group, session, style, marks, !stacked || focused, elsewhere, cx);
                placed(frames[i], &session.key, el, cx)
            })
            .collect();
        let vw = window.viewport_size().width.as_f32();
        let tree_w = self.sides.tree_width(vw);
        let mut tree = div().id("extra-tree").role(Role::Tree).flex().flex_col().flex_none().w(px(tree_w)).h_full().pt(px(a.s(6.))).overflow_y_scroll().bg(a.panel_fill(t.panel));
        for group in groups {
            tree = tree.child(
                div().px(px(a.s(10.))).pt(px(a.s(8.))).pb(px(a.s(2.))).text_size(px(a.s(10.5))).text_color(t.sub).child(format!("{} · {}", group.name.to_uppercase(), group.sessions.len())),
            );
            for session in &group.sessions {
                tree = tree.child(tree_row(session, self.selected.contains(&session.key), self.focus.as_deref() == Some(session.key.as_str()), &a, cx));
            }
        }
        let work = div()
            .id("extra-workspace")
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .bg(a.panel_fill(t.bg))
            .child(measure(self.layout.measured.clone(), cx.entity_id()))
            .children(tiles);
        div()
            .id("extra-window")
            .track_focus(&self.idle_focus)
            .size_full()
            .flex()
            .bg(t.bg)
            .text_color(t.fg)
            .font_family(crate::theme::FONT)
            .text_size(px(a.s(12.)))
            .on_action(cx.listener(|_, _: &CloseWindow, window, _| window.remove_window()))
            .on_action(cx.listener(|this, _: &crate::menus::NextLayout, _, cx| this.cycle_layout(cx)))
            .on_action(cx.listener(|this, _: &super::ToggleSidebar, _, cx| this.toggle_tree(cx)))
            .on_drag_move::<split::TreeDrag>(cx.listener(|this, ev: &DragMoveEvent<split::TreeDrag>, window, cx| this.drag_tree(ev, window, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.save_sides(cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.save_sides(cx)))
            .relative()
            .children((!self.sides.tree_hidden).then_some(tree))
            .child(work)
            .children((!self.sides.tree_hidden).then(|| split::grip("extra-grip-tree", split::TreeDrag, true).left(px(tree_w - layout::GRAB / 2.))))
    }
}

/// Zeile im Baum dieses Fensters: Statuspunkt und Titel, Klick wählt (mit Strg dazu oder weg).
fn tree_row(session: &crate::ui::model::Session, selected: bool, focused: bool, a: &crate::appearance::Appearance, cx: &Context<ExtraWindow>) -> impl IntoElement + use<> {
    let t = crate::ui::theme(cx);
    let key = session.key.clone();
    let edge = if focused { t.fg } else if selected { t.muted } else { gpui::transparent_black() };
    div()
        .id(gpui::ElementId::Name(format!("extra-row-{}", session.key).into()))
        .role(Role::TreeItem)
        .aria_label(crate::appearance::a11y::session_label(&session.title))
        .aria_selected(selected)
        .flex()
        .items_center()
        .gap(px(a.s(6.)))
        .mx(px(a.s(6.)))
        .my(px(a.s(1.)))
        .h(px(a.s(a.rows().session_row - 2.)))
        .px(px(a.s(6.)))
        .rounded(px(a.s(6.)))
        .border_1()
        .border_color(edge)
        .cursor_pointer()
        .when_selected(selected, t.bg)
        .child(crate::ui::sidebar::status_dot(session, &t, a))
        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(if selected { t.fg } else { t.sub }).child(session.title.clone()))
        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, _, cx| this.click_session(key.clone(), ev.modifiers().secondary(cx), cx)))
}

trait WhenSelected: Styled + Sized {
    fn when_selected(self, selected: bool, face: gpui::Hsla) -> Self {
        if selected { self.bg(face) } else { self }
    }
}
impl WhenSelected for gpui::Stateful<gpui::Div> {}

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
        titlebar: Some(gpui::TitlebarOptions { title: Some(title.into()), appears_transparent: false, traffic_light_position: None }),
        window_min_size: Some(size(px(super::frame::MIN_SIZE.0), px(super::frame::MIN_SIZE.1))),
        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
        ..Default::default()
    };
    let shell = shell.clone();
    cx.open_window(options, move |window, cx| cx.new(|cx| ExtraWindow::new(shell, index, window, cx))).ok().map(Into::into)
}
