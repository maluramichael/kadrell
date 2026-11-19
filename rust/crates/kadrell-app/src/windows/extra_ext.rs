//! Seitenleiste und Dock der Extensions, in jedem Fenster (`MainWindowController` hat beides je Fenster): die Panels
//! zeigen in allen Fenstern denselben Stand des Managers, jedes Fenster hat aber eigene Ansichten (Tab, Zeile, Tastatur)
//! und eigene Breiten (`rightSidebar.width.N`, `leftDock.height.N`). Die Zeichnung der Spalten liegt hier, die `Shell` und
//! das weitere Fenster rufen sie mit ihrer Aufteilung (`Sides`) auf.

use gpui::{AnyElement, App, AppContext as _, Context, DragMoveEvent, Entity, InteractiveElement, IntoElement, ParentElement, Styled, Window, div, px};
use kadrell_ext::manager::ExtensionManager;
use kadrell_ext::panel::Dock;

use super::{layout, split};
use crate::theme::Theme;
use crate::ui::chrome::{h_seam, v_seam};
use crate::ui::ext::model::PanelEntry;
use crate::ui::ext::panel::ExtPanel;

/// Die beiden Panels eines weiteren Fensters.
#[derive(Clone)]
pub struct ExtraPanels {
    pub right: Entity<ExtPanel>,
    pub left: Entity<ExtPanel>,
}

impl ExtraPanels {
    /// Neue Ansichten, gleich mit dem aktuellen Stand des Managers gefüllt (das Fenster kann nach dem letzten Ping
    /// aufgehen, `refresh_ext` käme dann nicht mehr).
    pub fn new<V: 'static>(manager: &ExtensionManager, cx: &mut Context<V>) -> ExtraPanels {
        let make = |id: &'static str, dock: Dock, cx: &mut Context<V>| {
            let panels = manager.panels_in(dock);
            cx.new(|cx| {
                let mut panel = ExtPanel::new(id, manager.clone(), cx);
                panel.set_panels(panels, cx);
                panel
            })
        };
        ExtraPanels { right: make("ext-right", Dock::Right, cx), left: make("ext-left", Dock::Left, cx) }
    }

    /// Neuer Stand des Managers; gezeichnet wird nur bei Änderung.
    pub fn set(&self, right: Vec<PanelEntry>, left: Vec<PanelEntry>, cx: &mut App) {
        self.right.update(cx, |p, cx| p.set_panels(right, cx));
        self.left.update(cx, |p, cx| p.set_panels(left, cx));
    }
}

/// Die linke Spalte: der Baum, darunter (mit Panels) das Dock der Extensions. Der Trenner dazwischen lässt sich ziehen,
/// der Baum behält mindestens 80, das Dock mindestens 60 pt. Beim ersten Erscheinen bekommt das Dock unten 220 pt
/// (höchstens die halbe Spalte), danach gilt die gezogene Höhe. `column_h` ist die Höhe der Spalte (Fenster ohne Titel
/// und Leiste), `on_drag` nimmt das Ziehen des Trenners entgegen.
#[allow(clippy::too_many_arguments)]
pub fn left_column(
    sides: &layout::Sides,
    panel: &Entity<ExtPanel>,
    sidebar: impl IntoElement,
    window: &Window,
    column_h: f32,
    t: &Theme,
    cx: &App,
    on_drag: impl Fn(&DragMoveEvent<split::DockDrag>, &mut Window, &mut App) + 'static,
) -> AnyElement {
    if !layout::show_dock(panel.read(cx).model().panels().len()) {
        return sidebar.into_any_element();
    }
    let width = sides.tree_width(window.viewport_size().width.as_f32());
    let dock_h = sides.dock_height(column_h);
    let dock = div()
        .flex()
        .flex_none()
        .h(px(dock_h))
        .min_h(px(layout::DOCK_MIN_H))
        .child(div().flex().flex_col().flex_none().w(px(width)).h_full().child(h_seam(t)).child(panel.clone()))
        .child(v_seam(t));
    let tree = div().flex().flex_1().min_h(px(layout::TREE_MIN_H)).child(sidebar);
    let grip = split::grip("grip-dock", split::DockDrag, false).bottom(px(dock_h - 1. - layout::GRAB / 2.));
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_none()
        .h_full()
        .on_drag_move::<split::DockDrag>(on_drag)
        .child(tree)
        .child(dock)
        .child(grip)
        .into_any_element()
}

/// Die rechte Seitenleiste, nur mit Panels und solange sie nicht ausgeblendet ist.
pub fn right_panel(sides: &layout::Sides, panel: &Entity<ExtPanel>, window: &Window, t: &Theme, cx: &App) -> Option<AnyElement> {
    if !layout::show_right(sides.right_visible, panel.read(cx).model().panels().len()) {
        return None;
    }
    let width = sides.right_width(window.viewport_size().width.as_f32());
    let column = div().flex().flex_col().flex_none().w(px(width)).h_full().child(panel.clone());
    Some(div().flex().flex_none().h_full().child(v_seam(t)).child(column).into_any_element())
}
