//! Das Hauptfenster als Aufteilung (`MainWindowController.split`): Baum links (mit Dock der Extensions darunter), die
//! Arbeitsfläche, rechts die Seitenleiste der Extensions. Die Trenner lassen sich ziehen (Griffzone 8 px), Baum und rechte
//! Seitenleiste sind höchstens halb so breit wie das Fenster, ⌘B nimmt den Baum samt Dock ganz heraus. Breiten und
//! Dock-Höhe gehen verzögert in die Einstellungen (`layout`). Auch das Ein- und Ausklappen aller Gruppen (⇧⌘B) und die
//! Verbindung zum aktiven Fenster (`windowDidBecomeKey`) stehen hier.

use std::time::Duration;

use gpui::{
    AppContext as _,
    AnyElement, App, Context, Div, DragMoveEvent, Global, InteractiveElement, IntoElement, MouseButton, MouseUpEvent, ParentElement, Render,
    Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::layout::{self, Change, GRAB};
use crate::Shell;
use crate::state::attention::patch_settings;
use crate::ui::theme;

/// So lange muss ein Ziehen ruhen, bevor die Breite in `settings.json` geschrieben wird.
const SAVE_DELAY: Duration = Duration::from_millis(300);

/// Breite des Baums, wie sie `ui::sidebar` zeichnet (setzt die `Shell` vor jedem Zeichnen).
pub struct TreeWidth(pub f32);
impl Global for TreeWidth {}

pub fn tree_width(cx: &App) -> f32 {
    cx.try_global::<TreeWidth>().map_or(layout::DEFAULT_SIDE, |w| w.0)
}

/// Welcher Trenner gerade gezogen wird (Nutzlast des Ziehens).
#[derive(Clone, Copy)]
pub struct TreeDrag;
#[derive(Clone, Copy)]
pub struct RightDrag;
#[derive(Clone, Copy)]
pub struct DockDrag;

/// Leere Ansicht während des Ziehens: gezeichnet wird nichts, der Trenner folgt der Maus selbst.
struct Ghost;

impl Render for Ghost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Unsichtbare Griffzone auf einem Trenner; `vertical` ist ein senkrechter Trenner (Zeiger ↔), sonst waagerecht (Zeiger ↕).
/// Der Aufrufer setzt die Lage (`left`/`bottom`).
pub fn grip<D: Clone + 'static>(id: &'static str, drag: D, vertical: bool) -> Stateful<Div> {
    let zone = div().id(id).absolute();
    let zone = if vertical { zone.top_0().h_full().w(px(GRAB)).cursor_col_resize() } else { zone.left_0().w_full().h(px(GRAB)).cursor_row_resize() };
    zone.on_drag(drag, |_, _, _, cx| cx.new(|_| Ghost))
}

fn viewport_width(window: &Window) -> f32 {
    window.viewport_size().width.as_f32()
}

impl Shell {
    // MARK: Zeichnen

    /// Die Breite des Baums für `ui::sidebar`, vor dem Bau des Baums aufrufen.
    pub(crate) fn publish_tree_width(&self, window: &Window, cx: &mut Context<Self>) {
        let width = self.win.sides.tree_width(viewport_width(window));
        if (tree_width(cx) - width).abs() > 0.01 {
            cx.set_global(TreeWidth(width));
        }
    }

    /// Baum (samt Dock), Arbeitsfläche und rechte Seitenleiste nebeneinander, mit den Griffzonen der Trenner.
    pub(crate) fn split_row(&self, sidebar: AnyElement, workspace: impl IntoElement, right: Option<AnyElement>, window: &Window, cx: &mut Context<Self>) -> Div {
        let t = theme(cx);
        let (vw, sides) = (viewport_width(window), &self.win.sides);
        let (tree_w, right_w) = (sides.tree_width(vw), sides.right_width(vw));
        let mut row = div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .on_drag_move::<TreeDrag>(cx.listener(|this, ev: &DragMoveEvent<TreeDrag>, window, cx| this.drag_tree(ev, window, cx)))
            .on_drag_move::<RightDrag>(cx.listener(|this, ev: &DragMoveEvent<RightDrag>, window, cx| this.drag_right(ev, window, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.flush_sides()))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.flush_sides()))
            .children(crate::appearance::wallpaper::layer(&crate::appearance::current(cx).wallpaper, t.bg));
        if !sides.tree_hidden {
            row = row.child(sidebar);
        }
        let has_right = right.is_some();
        row = row.child(workspace).children(right);
        if !sides.tree_hidden {
            row = row.child(grip("grip-tree", TreeDrag, true).left(px(tree_w + 1. - GRAB / 2.)));
        }
        if has_right {
            row = row.child(grip("grip-right", RightDrag, true).left(px(vw - right_w - 1. - GRAB / 2.)));
        }
        row
    }

    // MARK: Ziehen und Speichern

    fn drag_tree(&mut self, ev: &DragMoveEvent<TreeDrag>, window: &Window, cx: &mut Context<Self>) {
        let x = (ev.event.position.x - ev.bounds.origin.x).as_f32();
        let change = self.win.sides.drag_tree(x, viewport_width(window));
        self.queue_save(change, cx);
    }

    fn drag_right(&mut self, ev: &DragMoveEvent<RightDrag>, window: &Window, cx: &mut Context<Self>) {
        let from_right = (ev.bounds.right() - ev.event.position.x).as_f32();
        let change = self.win.sides.drag_right(from_right, viewport_width(window));
        self.queue_save(change, cx);
    }

    /// Trenner zwischen Baum und Dock: die Spalte, in der er liegt, hängt den Listener an.
    pub(crate) fn drag_dock(&mut self, ev: &DragMoveEvent<DockDrag>, cx: &mut Context<Self>) {
        let from_bottom = (ev.bounds.bottom() - ev.event.position.y).as_f32();
        let change = self.win.sides.drag_dock(from_bottom, ev.bounds.size.height.as_f32());
        self.queue_save(change, cx);
    }

    /// Merkt die Änderung und schreibt sie, sobald das Ziehen 300 ms ruht (oder die Maus losgelassen wird).
    pub(crate) fn queue_save(&mut self, change: Change, cx: &mut Context<Self>) {
        let win = &mut self.win;
        win.pending.retain(|(key, _)| *key != change.0);
        win.pending.push(change);
        win.generation += 1;
        let generation = win.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let _ = this.update(cx, |shell, _| {
                if shell.win.generation == generation {
                    shell.flush_sides();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn flush_sides(&mut self) {
        if self.win.pending.is_empty() {
            return;
        }
        let changes = std::mem::take(&mut self.win.pending).into_iter().map(|(key, value)| (key, Some(value))).collect();
        patch_settings(&self.state.profile.settings_path(), changes);
    }

    // MARK: Aktionen

    /// ⌘B: Baum samt Dock ganz aus der Aufteilung nehmen und wieder einsetzen.
    pub(crate) fn toggle_tree(&mut self, cx: &mut Context<Self>) {
        self.win.sides.toggle_tree();
        cx.notify();
    }

    // MARK: Fenster

    /// Beim Start: Lage des Hauptfensters speichern und das aktive Fenster verfolgen (`windowDidBecomeKey`).
    pub(crate) fn start_windows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        super::frame::watch(window, cx, self.state.profile.settings_path(), 0);
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.note_current(0, cx);
            }
        })
        .detach();
    }

    /// Das Fenster `index` ist jetzt vorn: Menü, Kürzel und Leiste richten sich nach ihm.
    pub(crate) fn note_current(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.win.current != index {
            self.win.current = index;
            cx.notify();
        }
    }

    /// Nummer des Fensters, das zuletzt aktiv war.
    pub(crate) fn current_window(&self) -> usize {
        self.win.current
    }
}

