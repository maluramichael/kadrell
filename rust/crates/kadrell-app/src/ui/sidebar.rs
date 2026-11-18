//! Linke Seitenleiste: der Baum der Gruppen und Sessions in einem der beiden Designs der Mac-App (Klassisch mit Pfad
//! unter dem Gruppennamen, Kompakt ohne Pfad mit Status-Punkten im eingeklappten Kopf und Fuge zwischen den Gruppen).
//! Maße kommen aus `appearance::metrics`, alles wird mit der UI-Größe multipliziert und folgt Design, Laufzeit-Anzeige
//! und Deckkraft der Einstellungen sofort. Der Baum ist für Bildschirmleser eine Gliederung (`appearance::a11y`).
//! Dazu die Bedienung der Zeilen: schwebende Leiste (Herz, Plus, Stift, Kreuz), Mittelklick schließt, Ziehen sortiert
//! um oder wirft eine Session in eine andere Gruppe, Ordner aus dem Explorer starten eine Session (Regeln in
//! `groups::drag`). Hat der Baum die Tastatur (⌘1), trägt die Fokus-Zeile die Akzentfarbe.

use gpui::{
    AnyElement, AppContext, ClickEvent, Context, Div, ExternalPaths, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseUpEvent, ParentElement, Role, ScrollHandle, Stateful, StatefulInteractiveElement, Styled, div, px,
};
use kadrell_i18n::trd;

use super::chrome::{h_seam, v_seam};
use super::groups::drag::{DragRow, Dragged, Over, TreeView, can_drag, plan};
use super::groups::rows::{DragGhost, Mark, RowKey, block_mark, middle_released, row_mark};
use super::groups::{files, tool_icons};
use super::icons;
use super::layout::Selection;
use super::model::{Group, Session, Status};
use super::overlay::raised;
use super::theme;
use crate::Shell;
use crate::appearance::metrics::{self, ROW_INSET};
use crate::appearance::{Appearance, a11y, breathing};
use crate::theme::Theme;

/// Breite einer Taste der schwebenden Leiste und Höhe der Leiste (unskaliert).
const TOOL_W: f32 = 20.;
const TOOL_H: f32 = 22.;

/// Was der Baum von der Shell braucht, ohne sie lesen zu können (sie wird gerade gezeichnet).
pub struct SidebarCtx {
    pub view: TreeView,
    pub scroll: ScrollHandle,
    /// Die Tastatur gehört dem Baum (⌘1).
    pub keys: bool,
}

const OUTLINE: f32 = 1.5;

/// Zeilenrahmen mit optionalem Auswahlbalken (3 px links, volle Zeilenhöhe) und der Fläche darin.
fn row_frame(id: impl Into<gpui::ElementId>, height: f32, bar: Option<Hsla>, a: &Appearance) -> Stateful<Div> {
    let mut row = div()
        .id(id)
        .relative()
        .flex_none()
        .w_full()
        .h(px(a.s(height)))
        .px(px(a.s(ROW_INSET.0)))
        .py(px(a.s(ROW_INSET.1)))
        .cursor_pointer();
    if let Some(color) = bar {
        row = row.child(div().absolute().left_0().top_0().w(px(a.s(3.))).h(px(a.s(height))).bg(color));
    }
    row
}

fn outline(el: Div, color: Hsla, a: &Appearance) -> Div {
    let w = px(a.s(OUTLINE));
    el.border_t(w).border_b(w).border_l(w).border_r(w).border_color(color)
}

/// Gesperrtes Versal-Label mit Zahl: `KADRELL · 2`.
fn group_label(group: &Group, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .text_size(px(a.s(10.5)))
        .font_weight(FontWeight::BOLD)
        .text_color(t.sub)
        .whitespace_nowrap()
        .text_ellipsis()
        .child(format!("{} · {}", group.name.to_uppercase(), group.sessions.len()))
}

/// Chevron als eigene Klickfläche: klappt die Gruppe auf oder zu, ohne sie zu markieren.
fn chevron_button(g: usize, group: &Group, t: &Theme, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    div()
        .id(("chevron", g))
        .flex_none()
        .child(icons::chevron_scaled(!group.collapsed, t.muted, a.scale))
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.toggle_group(g, cx);
        }))
}

/// Sessions der Gruppe, die auf dich warten, auch bei eingeklappter Gruppe.
fn waiting(group: &Group) -> usize {
    group.sessions.iter().filter(|s| s.status == Status::Waiting).count()
}

/// „2 ⏳“ links vom Favoriten, nur wenn etwas wartet.
fn waiting_badge(group: &Group, t: &Theme, a: &Appearance) -> Option<Div> {
    let n = waiting(group);
    (n > 0).then(|| div().flex_none().ml(px(a.s(6.))).text_size(px(a.s(10.5))).font_weight(FontWeight::BOLD).text_color(t.waiting).child(format!("{n} ⏳")))
}

/// Favoritenherz ganz rechts in der Kopfzeile.
fn favorite(group: &Group, t: &Theme, a: &Appearance) -> Option<Div> {
    group.favorite.then(|| div().flex_none().ml(px(a.s(6.))).text_size(px(a.s(11.))).text_color(t.group(group.color)).child("♥"))
}

/// Kompakt, eingeklappt: ein Punkt je Session, so bleibt der Status sichtbar. Aufgeklappt zeigen ihn die Zeilen selbst.
fn status_dots(group: &Group, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    let dots = group.sessions.iter().map(|s| div().flex_none().size(px(a.s(5.))).rounded_full().bg(s.status.color(t)));
    div().flex().flex_none().items_center().gap(px(a.s(3.))).mr(px(a.s(5.))).children(dots)
}

fn group_head(group: &Group, t: &Theme, a: &Appearance, chevron: gpui::AnyElement) -> impl IntoElement + use<> {
    let m = a.rows();
    let compact_dots = !m.path && group.collapsed;
    div()
        .flex()
        .items_center()
        .h(px(a.s(m.group_head() - 2. * ROW_INSET.1)))
        .child(chevron)
        .child(div().ml(px(a.s(4.))).mr(px(a.s(5.))).size(px(a.s(6.))).flex_none().rounded(px(a.s(1.5))).bg(t.group(group.color)))
        .child(group_label(group, t, a))
        .children(compact_dots.then(|| status_dots(group, t, a)))
        .children(waiting_badge(group, t, a))
        .children(favorite(group, t, a))
}

fn group_path(group: &Group, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    div()
        .ml(px(a.s(metrics::GROUP_PATH_X - metrics::ROW_INSET.0)))
        .h(px(a.s(14.)))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(a.s(10.5)))
        .text_color(t.muted)
        .child(group.path.clone())
}

// MARK: Schwebende Leiste

/// Eine Taste der Leiste: Klick löst aus, ohne die Zeile darunter zu wählen.
fn tool_button(id: (&'static str, usize), icon: AnyElement, t: &Theme, a: &Appearance, on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static) -> Stateful<Div> {
    let hot = t.bg;
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(a.s(TOOL_W)))
        .rounded(px(a.s(4.)))
        .cursor_pointer()
        .hover(move |s| s.bg(hot))
        .on_click(move |ev, window, cx| {
            cx.stop_propagation();
            on_click(ev, window, cx);
        })
        .child(icon)
}

/// Erhabene Leiste über dem rechten Ende der Zeile, nur sichtbar, solange die Zeile überfahren wird.
fn toolbar(host: String, buttons: Vec<Stateful<Div>>, t: &Theme, a: &Appearance) -> Div {
    let m = a.rows();
    let width = a.s(buttons.len() as f32 * TOOL_W + 4.);
    div()
        .absolute()
        .right(px(a.s(ROW_INSET.0)))
        .top(px(a.s((m.group_head().min(m.session_row) - TOOL_H) / 2.)))
        .w(px(width))
        .h(px(a.s(TOOL_H)))
        .px(px(a.s(2.)))
        .py(px(a.s(1.)))
        .flex()
        .items_center()
        .rounded(px(a.s(5.)))
        .bg(raised(t))
        .border_1()
        .border_color(t.line)
        .opacity(if always_visible() { 1. } else { 0. })
        .group_hover(host, |s| s.opacity(1.))
        .children(buttons)
}

/// Nur für Screenshots: `KADRELL_DEV_TOOLBAR=1` zeigt die Leisten an allen Zeilen, ohne dass die Maus darüber steht.
fn always_visible() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("KADRELL_DEV_TOOLBAR").is_some())
}

fn group_tools(g: usize, group: &Group, t: &Theme, a: &Appearance, cx: &mut Context<Shell>) -> Vec<Stateful<Div>> {
    let id = group.id.clone();
    let heart_color = if group.favorite { t.group(group.color) } else { t.muted };
    let fav = id.clone();
    let mut tools = vec![tool_button(("tool-fav", g), tool_icons::heart(heart_color, group.favorite).into_any_element(), t, a, cx.listener(move |this, _, window, cx| this.run_cmd(crate::ui::context_menu::Cmd::ToggleFavorite(fav.clone()), window, cx)))];
    if group.cwd.is_empty() {
        return tools;
    }
    let new = id.clone();
    tools.push(tool_button(("tool-new", g), tool_icons::plus(t.muted).into_any_element(), t, a, cx.listener(move |this, ev: &ClickEvent, window, cx| {
        let cmd = if ev.modifiers().secondary(cx) { crate::ui::context_menu::Cmd::NewTerminalIn(new.clone()) } else { crate::ui::context_menu::Cmd::NewSessionIn(new.clone()) };
        this.run_cmd(cmd, window, cx);
    })));
    let edit = id.clone();
    tools.push(tool_button(("tool-edit", g), tool_icons::pen(t.muted).into_any_element(), t, a, cx.listener(move |this, _, window, cx| this.open_edit_group(&edit, window, cx))));
    tools.push(tool_button(("tool-close", g), tool_icons::x(t.muted).into_any_element(), t, a, cx.listener(move |this, ev: &ClickEvent, window, cx| this.request_close_group(&id, ev.modifiers().alt, window, cx))));
    tools
}

fn session_tools(n: usize, key: &str, t: &Theme, a: &Appearance, cx: &mut Context<Shell>) -> Vec<Stateful<Div>> {
    let (rename, close) = (key.to_string(), key.to_string());
    vec![
        tool_button(("tool-rename", n), tool_icons::pen(t.muted).into_any_element(), t, a, cx.listener(move |this, _, window, cx| this.open_rename(super::rename::Target::Session(rename.clone()), window, cx))),
        tool_button(("tool-remove", n), tool_icons::x(t.muted).into_any_element(), t, a, cx.listener(move |this, ev: &ClickEvent, window, cx| this.request_close_session(&close, ev.modifiers().alt, window, cx))),
    ]
}

// MARK: Ziehen und Ablegen

/// Einfügelinie oder Rahmen als Fläche, die nur beim Ziehen über `host` sichtbar wird (und nur, wo `show` zutrifft).
fn mark_overlay(host: String, name: String, mark: Mark, show: impl Fn(&DragRow) -> bool + 'static, t: &Theme, a: &Appearance) -> Div {
    let base = div().group(name).absolute().can_drop(move |v, _, _| v.downcast_ref::<DragRow>().is_some_and(&show));
    let (line, tint) = (t.fg, t.accent.opacity(0.14));
    let ring = t.accent;
    match mark {
        Mark::Above => base.top_0().left(px(a.s(ROW_INSET.0))).right(px(a.s(ROW_INSET.0))).h(px(a.s(2.))).group_drag_over::<DragRow>(host, move |s| s.bg(line)),
        Mark::Below => base.bottom_0().left(px(a.s(ROW_INSET.0))).right(px(a.s(ROW_INSET.0))).h(px(a.s(2.))).group_drag_over::<DragRow>(host, move |s| s.bg(line)),
        Mark::Ring => base
            .top_0()
            .left(px(a.s(3.)))
            .right(px(a.s(3.)))
            .h_full()
            .rounded(px(a.s(6.)))
            .border_2()
            .border_color(gpui::transparent_black())
            .group_drag_over::<DragRow>(host, move |s| s.border_color(ring).bg(tint)),
    }
}

/// Das Greifen einer Zeile: Nutzlast samt Ghost.
fn draggable(row: Stateful<Div>, drag: DragRow, view: TreeView) -> Stateful<Div> {
    if !can_drag(&drag.item, view) {
        return row;
    }
    row.on_drag(drag, |d, _, _, cx| cx.new(|_| DragGhost { label: d.label.clone() }))
}

/// Ablage auf einer Zeile: Wirkung nach `groups::drag::plan`, nur wo es eine gibt.
fn droppable(row: Stateful<Div>, key: Option<String>, group: String, view: TreeView, cx: &mut Context<Shell>) -> Stateful<Div> {
    let (k, g) = (key.clone(), group.clone());
    row.can_drop(move |v, _, _| v.downcast_ref::<DragRow>().is_some_and(|d| plan(d, over(&k, &g), view).is_some()))
        .on_drop(cx.listener(move |this, d: &DragRow, window, cx| this.drop_on_row(d, key.as_deref(), &group, window, cx)))
}

fn over<'a>(key: &'a Option<String>, group: &'a str) -> Over<'a> {
    match key {
        Some(key) => Over::Session { key, group },
        None => Over::Head { group },
    }
}

// MARK: Zeilen

fn group_header(place: Place, group: &Group, holds_selection: bool, ctx: &SidebarCtx, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let Place { g, head_order: order, .. } = place;
    let first = g == 0;
    let t = theme(cx);
    let m = a.rows();
    let bar = holds_selection.then(|| t.group(group.color));
    let face = div()
        .size_full()
        .rounded(px(a.s(6.)))
        .flex()
        .flex_col()
        .hover(move |s| s.bg(t.bg))
        .child(group_head(group, &t, a, chevron_button(g, group, &t, a, cx).into_any_element()));
    let face = if m.path { face.child(group_path(group, &t, a)) } else { face };
    let label = a11y::group_label(&group.name, group.sessions.len());
    let host = format!("row-{order}");
    let id = group.id.clone();
    let real = !id.is_empty();
    let drag = DragRow { item: Dragged::Group(id.clone()), group: id.clone(), order, label: group.name.clone() };
    let mut row = row_frame(("group", g), m.group_row, bar, a)
        .group(host.clone())
        .role(Role::TreeItem)
        .aria_label(label)
        .aria_level(a11y::level(true, true))
        .aria_expanded(!group.collapsed)
        .on_click(cx.listener(move |this, _, window, cx| this.select_group(g, window, cx)))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            let id = this.groups[g].id.clone();
            if !id.is_empty() {
                let items = this.group_menu_items(&id);
                this.open_context(ev.position, items, cx);
            }
        }))
        .child(face);
    if real {
        row = row.child(toolbar(host, group_tools(g, group, &t, a, cx), &t, a));
        let key = RowKey::Group(id.clone());
        let pressed = key.clone();
        row = row
            .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, _, _| this.groups_ui.middle = Some(pressed.clone())))
            .on_mouse_up(MouseButton::Middle, cx.listener(move |this, ev: &MouseUpEvent, window, cx| {
                if middle_released(&mut this.groups_ui.middle, &key) {
                    this.request_close_group(&id, ev.modifiers.alt, window, cx);
                }
            }));
    }
    let group_id = group.id.clone();
    let row = droppable(draggable(row, drag, ctx.view), None, group_id, ctx.view, cx);
    // Kompakt trennt eine Fuge die Gruppen, nur die erste hat keine.
    div().flex().flex_col().flex_none().children((!m.path && !first).then(|| h_seam(&t))).child(row)
}

fn new_badge(status: Status, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    let face = if status == Status::Waiting { t.waiting } else { t.running };
    div()
        .flex()
        .flex_none()
        .items_center()
        .h(px(a.s(16.)))
        .px(px(a.s(5.)))
        .rounded(px(a.s(3.)))
        .bg(face)
        .text_color(t.pill_text_on(face))
        .text_size(px(a.s(10.)))
        .font_weight(FontWeight::BOLD)
        .child(trd!("neu"))
}

/// Statuspunkt; arbeitende Sessions bekommen den atmenden Punkt (Warten und Fehler behalten den Punkt).
pub(crate) fn status_dot(session: &Session, t: &Theme, a: &Appearance) -> gpui::AnyElement {
    let color = session.status.color(t);
    if session.status == Status::Running {
        return div().flex_none().child(breathing::breathing_dot(color, a.s(metrics::DOT_SIZE), a.s(breathing::GLOW_RADIUS))).into_any_element();
    }
    div().size(px(a.s(metrics::DOT_SIZE))).flex_none().rounded_full().bg(color).into_any_element()
}

fn session_face(session: &Session, selected: bool, t: &Theme, a: &Appearance) -> Div {
    let title_color = if selected || session.is_new { t.fg } else { t.sub };
    let title = div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(title_color).child(session.title.clone());
    let title = if session.is_new { title.font_weight(FontWeight::BOLD) } else { title };
    let mut face = div()
        .size_full()
        .rounded(px(a.s(6.)))
        .flex()
        .items_center()
        .gap(px(a.s(6.)))
        .pl(px(a.s(4.5)))
        .pr(px(a.s(2.5)))
        .child(status_dot(session, t, a))
        .child(title);
    if session.is_new {
        face = face.child(new_badge(session.status, t, a));
    }
    if a.show_age {
        face = face.child(div().flex_none().text_size(px(a.s(10.5))).text_color(t.muted).child(session.age.clone()));
    }
    face
}

/// Alles, was eine Session-Zeile außer ihrer Gestalt braucht.
struct SessionRow {
    g: usize,
    s: usize,
    selected: bool,
    focused: bool,
    order: usize,
    /// Der Baum ist gruppiert (Ebene der Gliederung).
    grouped: bool,
    /// Der Baum hat die Tastatur (⌘1): die Fokus-Session trägt die Akzentfarbe.
    keys: bool,
}

/// Rand der Zeile: Fokus-Session in `fg` (mit Tastatur im Baum in der Akzentfarbe), die übrigen markierten in `muted`.
fn session_edge(focused: bool, selected: bool, keys: bool, t: &Theme) -> Hsla {
    match (focused, selected) {
        (true, _) if keys => t.accent,
        (true, _) => t.fg,
        (false, true) => t.muted,
        _ => gpui::transparent_black(),
    }
}

fn session_row(row: SessionRow, group: &Group, ctx: &SidebarCtx, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let SessionRow { g, s, selected, focused, order, grouped, keys } = row;
    let t = theme(cx);
    let session = &group.sessions[s];
    let mut face = session_face(session, selected, &t, a).hover(move |st| st.bg(t.bg).text_color(t.fg));
    // Der Rand steht immer, nur seine Farbe wechselt: Inhalt springt bei der Auswahl nicht.
    if selected {
        face = face.bg(t.bg);
    }
    face = outline(face, session_edge(focused, selected, keys, &t), a);
    let bar = selected.then(|| t.group(group.color));
    let spoken = a11y::spoken(session.status, session.ended, false);
    let host = format!("row-{order}");
    let key = session.key.clone();
    let (pressed, released) = (RowKey::Session(key.clone()), RowKey::Session(key.clone()));
    let (menu_key, close_key) = (key.clone(), key.clone());
    let drag = DragRow { item: Dragged::Session(key.clone()), group: group.id.clone(), order, label: session.title.clone() };
    let mut row = row_frame(("session", g * 100 + s), a.rows().session_row, bar, a)
        .group(host.clone())
        .role(Role::TreeItem)
        .aria_label(a11y::session_label(&session.title))
        .aria_value(a11y::session_value(&spoken, session.is_new, selected))
        .aria_level(a11y::level(false, grouped))
        .aria_selected(selected)
        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
            this.click_session(g, s, ev.modifiers(), window, cx)
        }))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            let items = this.session_menu_items(&menu_key);
            this.open_context(ev.position, items, cx);
        }))
        .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, _, _| this.groups_ui.middle = Some(pressed.clone())))
        .on_mouse_up(MouseButton::Middle, cx.listener(move |this, ev: &MouseUpEvent, window, cx| {
            if middle_released(&mut this.groups_ui.middle, &released) {
                this.request_close_session(&close_key, ev.modifiers.alt, window, cx);
            }
        }))
        .child(face)
        .child(toolbar(host.clone(), session_tools(g * 100 + s, &key, &t, a, cx), &t, a));
    // Einfügelinie beim Umsortieren in der eigenen Gruppe (oder in der flachen Liste).
    for mark in [Mark::Above, Mark::Below] {
        let (k, gid, view) = (key.clone(), group.id.clone(), ctx.view);
        let show = move |d: &DragRow| row_mark(&plan(d, Over::Session { key: &k, group: &gid }, view), d, order) == Some(mark);
        row = row.child(mark_overlay(host.clone(), format!("{host}-{mark:?}"), mark, show, &t, a));
    }
    droppable(draggable(row, drag, ctx.view), Some(key), group.id.clone(), ctx.view, cx)
}

/// Rahmen und Linien am ganzen Gruppenblock: eine fallende Session (Rahmen) und eine ziehende Gruppe (Linie).
fn block_overlays(block: &str, group: &Group, head_order: usize, ctx: &SidebarCtx, t: &Theme, a: &Appearance) -> Vec<Div> {
    [Mark::Ring, Mark::Above, Mark::Below]
        .into_iter()
        .map(|mark| {
            let (gid, view) = (group.id.clone(), ctx.view);
            let show = move |d: &DragRow| block_mark(&plan(d, Over::Head { group: &gid }, view), d, head_order) == Some(mark);
            mark_overlay(block.to_string(), format!("{block}-{mark:?}"), mark, show, t, a)
        })
        .collect()
}

/// Wo eine Gruppe im Baum steht: Nummer, Nummer ihrer Kopfzeile in der Anzeigereihenfolge und ob der Baum gruppiert ist.
#[derive(Clone, Copy)]
struct Place {
    g: usize,
    head_order: usize,
    grouped: bool,
}

fn group_block(place: Place, group: &Group, sel: &Selection, ctx: &SidebarCtx, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let Place { g, head_order, grouped } = place;
    let t = theme(cx);
    // Klassisch: Luft vor jeder Gruppe außer der ersten.
    let gap = if g > 0 { a.s(a.rows().group_gap) } else { 0. };
    let block_name = format!("blk-{g}");
    let head = group_header(place, group, sel.focus.0 == g, ctx, a, cx);
    let mut block = div().id(("block", g)).group(block_name.clone()).relative().flex().flex_col().flex_none().mt(px(gap)).child(head);
    if !group.collapsed {
        for s in 0..group.sessions.len() {
            let row = SessionRow { g, s, selected: sel.contains((g, s)), focused: sel.focus == (g, s), order: head_order + 1 + s, grouped, keys: ctx.keys };
            block = block.child(session_row(row, group, ctx, a, cx));
        }
    }
    if group.id.is_empty() {
        return block;
    }
    block.children(block_overlays(&block_name, group, head_order, ctx, &t, a))
}

pub fn sidebar(groups: &[Group], sel: &Selection, ctx: &SidebarCtx, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let t = theme(cx);
    let a = crate::appearance::current(cx);
    let accent = t.accent.opacity(0.14);
    let grouped = groups.len() != 1 || !groups[0].id.is_empty();
    let mut col = div()
        .id("sidebar")
        .role(Role::Tree)
        .aria_label(trd!("Sessions"))
        .flex()
        .flex_col()
        .flex_none()
        .w(px(crate::windows::split::tree_width(cx)))
        .h_full()
        .pt(px(a.s(a.rows().top_inset)))
        .pb(px(a.s(8.)))
        .overflow_y_scroll()
        .track_scroll(&ctx.scroll)
        .bg(a.panel_fill(t.panel))
        .text_size(px(a.s(12.)))
        .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
            if let Some(dir) = files::first_folder(paths) {
                this.drop_folder(&dir, window, cx);
            }
        }))
        .drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(accent));
    let mut order = 0;
    for (g, group) in groups.iter().enumerate() {
        col = col.child(group_block(Place { g, head_order: order, grouped }, group, sel, ctx, &a, cx));
        order += 1 + if group.collapsed { 0 } else { group.sessions.len() };
    }
    div().flex().flex_none().h_full().child(col).child(v_seam(&t))
}
