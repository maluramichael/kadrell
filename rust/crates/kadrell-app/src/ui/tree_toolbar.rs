//! Schwebende Leiste an den Baumzeilen (`SidebarView.toolbar`): Gruppe Favorit, Neue Session (⌘/Strg: Terminal, Remote:
//! Server-Symbol), Bearbeiten, Schließen; Session Umbenennen, Schließen. Erhabene Fläche (Radius 5, fester Schatten 2,
//! Fugenring), Symbole gedämpft, beim Überfahren `fg`, das Schließen-Kreuz `error`, Hover-Fläche `hoverOnRaised`.
//! Jede Taste hat Beschriftung, Tooltip und Rolle `Button` für Bildschirmleser.

use gpui::{
    AnyElement, BoxShadow, ClickEvent, Context, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Role, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, hsla, point, px,
};
use kadrell_i18n::trd;

use super::context_menu::Cmd;
use super::groups::tool_icons;
use super::model::Group;
use super::{chrome, overlay::raised, tooltip};
use crate::Shell;
use crate::appearance::Appearance;
use crate::appearance::metrics::ROW_INSET;
use crate::theme::Theme;

/// Breite einer Taste und Höhe der Leiste (unskaliert), Höhe der Kopfzeile, an der die Leiste mittig sitzt.
const TOOL_W: f32 = 20.;
const TOOL_H: f32 = 22.;
const HEAD_MAX: f32 = 24.;

/// Zeichnet ein Symbol in der gegebenen Farbe.
type IconFn = Box<dyn Fn(Hsla) -> AnyElement>;

/// Eine Taste: Beschriftung, Symbol und ob sie die letzte ist (Schließen, beim Überfahren in `error`).
struct Tool {
    id: (&'static str, usize),
    label: SharedString,
    icon: IconFn,
    last: bool,
}

fn icon(f: impl Fn(Hsla) -> AnyElement + 'static) -> IconFn {
    Box::new(f)
}

/// Eine Taste der Leiste: Klick löst aus, ohne die Zeile darunter zu wählen. Zwei Symbole übereinander, das helle
/// blendet beim Überfahren ein (kein Zustand nötig).
fn tool_button(tool: Tool, t: &Theme, a: &Appearance, on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static) -> Stateful<Div> {
    let group = format!("tool-{}-{}", tool.id.0, tool.id.1);
    let hot = if tool.last { t.error } else { t.fg };
    let overlay = div().absolute().top_0().left_0().size_full().flex().items_center().justify_center().opacity(0.).group_hover(group.clone(), |s| s.opacity(1.)).child((tool.icon)(hot));
    let hover_face = t.hover_on_raised();
    div()
        .id(tool.id)
        .group(group)
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(a.s(TOOL_W)))
        .rounded(px(a.s(6.)))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_face))
        .role(Role::Button)
        .aria_label(tool.label.clone())
        .tooltip(tooltip::text(tool.label))
        .on_click(move |ev, window, cx| {
            cx.stop_propagation();
            on_click(ev, window, cx);
        })
        .child((tool.icon)(t.muted))
        .child(overlay)
}

/// Erhabene Leiste über dem rechten Ende der Zeile, nur sichtbar, solange die Zeile überfahren wird (und nicht
/// gezogen wird: GPUI meldet während des Ziehens kein Hover). `row_height` ist die Höhe der ganzen Zeile.
fn toolbar(host: String, row_height: f32, buttons: Vec<Stateful<Div>>, t: &Theme, a: &Appearance) -> Div {
    let width = a.s(buttons.len() as f32 * TOOL_W + 4.);
    let head = row_height.min(HEAD_MAX);
    let shadow = BoxShadow {
        color: hsla(0., 0., 0., if t.dark { 0.5 } else { 0.16 }),
        offset: point(px(0.), px(a.s(2.))),
        blur_radius: px(0.),
        spread_radius: px(0.),
        inset: false,
    };
    let ring = div().absolute().top_0().left_0().size_full().rounded(px(a.s(4.))).border_1().border_color(chrome::seam_light(t));
    div()
        .absolute()
        .right(px(a.s(ROW_INSET.0)))
        .top(px(a.s((head - TOOL_H) / 2.)))
        .w(px(width))
        .h(px(a.s(TOOL_H)))
        .px(px(1.))
        .flex()
        .items_center()
        .rounded(px(a.s(5.)))
        .bg(raised(t))
        .border_1()
        .border_color(chrome::seam_dark(t))
        .shadow(vec![shadow])
        .opacity(if always_visible() { 1. } else { 0. })
        .group_hover(host, |s| s.opacity(1.))
        .child(ring)
        .children(buttons)
}

/// Nur für Screenshots: `KADRELL_DEV_TOOLBAR=1` zeigt die Leisten an allen Zeilen, ohne dass die Maus darüber steht.
fn always_visible() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("KADRELL_DEV_TOOLBAR").is_some())
}

fn favorite_label(group: &Group) -> SharedString {
    if group.favorite { trd!("Kein Favorit") } else { trd!("Favorit") }.into()
}

/// Symbol von „Neue Session“: Remote-Gruppen zeigen den Server, mit ⌘/Strg gehalten der Computer, sonst Plus.
pub fn new_session_icon(remote: bool, cmd: bool) -> NewIcon {
    if remote {
        NewIcon::Server
    } else if cmd {
        NewIcon::Computer
    } else {
        NewIcon::Plus
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NewIcon {
    Plus,
    Server,
    Computer,
}

fn new_icon_element(kind: NewIcon, color: Hsla) -> AnyElement {
    match kind {
        NewIcon::Plus => tool_icons::plus(color).into_any_element(),
        NewIcon::Server => tool_icons::server(color).into_any_element(),
        NewIcon::Computer => tool_icons::computer(color).into_any_element(),
    }
}

/// Wo die Leiste sitzt: Name der Zeilengruppe für das Hover, Höhe der Zeile und ob ⌘/Strg gehalten wird.
pub struct Spot {
    pub host: String,
    pub row_height: f32,
    pub cmd: bool,
}

/// Leiste einer Gruppenzeile.
pub fn group_toolbar(g: usize, group: &Group, spot: Spot, t: &Theme, a: &Appearance, cx: &mut Context<Shell>) -> Div {
    let Spot { host, row_height, cmd } = spot;
    let id = group.id.clone();
    let (fav_color, filled) = (t.group(group.color), group.favorite);
    let fav = id.clone();
    let favorite = Tool {
        id: ("tool-fav", g),
        label: favorite_label(group),
        icon: icon(move |c| tool_icons::heart(if filled { fav_color } else { c }, filled).into_any_element()),
        last: false,
    };
    let mut tools = vec![tool_button(favorite, t, a, cx.listener(move |this, _, window, cx| this.run_cmd(Cmd::ToggleFavorite(fav.clone()), window, cx)))];
    if group.cwd.is_empty() {
        return toolbar(host, row_height, tools, t, a);
    }
    let kind = new_icon_element_kind(group, cmd);
    let new = id.clone();
    let plus = Tool { id: ("tool-new", g), label: trd!("Neue Session").into(), icon: icon(move |c| new_icon_element(kind, c)), last: false };
    tools.push(tool_button(plus, t, a, cx.listener(move |this, ev: &ClickEvent, window, cx| {
        let cmd = if ev.modifiers().secondary(cx) { Cmd::NewTerminalIn(new.clone()) } else { Cmd::NewSessionIn(new.clone()) };
        this.run_cmd(cmd, window, cx);
    })));
    let edit = id.clone();
    let pen = Tool { id: ("tool-edit", g), label: trd!("Bearbeiten").into(), icon: icon(|c| tool_icons::pen(c).into_any_element()), last: false };
    tools.push(tool_button(pen, t, a, cx.listener(move |this, _, window, cx| this.open_edit_group(&edit, window, cx))));
    let cross = Tool { id: ("tool-close", g), label: trd!("Schließen").into(), icon: icon(|c| tool_icons::x(c).into_any_element()), last: true };
    tools.push(tool_button(cross, t, a, cx.listener(move |this, ev: &ClickEvent, window, cx| this.request_close_group(&id, ev.modifiers().alt, window, cx))));
    toolbar(host, row_height, tools, t, a)
}

fn new_icon_element_kind(group: &Group, cmd: bool) -> NewIcon {
    new_session_icon(group.host.is_some(), cmd)
}

/// Leiste einer Session-Zeile: Umbenennen und Schließen.
pub fn session_toolbar(n: usize, key: &str, host: String, row_height: f32, t: &Theme, a: &Appearance, cx: &mut Context<Shell>) -> Div {
    let (rename, close) = (key.to_string(), key.to_string());
    let pen = Tool { id: ("tool-rename", n), label: trd!("Umbenennen").into(), icon: icon(|c| tool_icons::pen(c).into_any_element()), last: false };
    let cross = Tool { id: ("tool-remove", n), label: trd!("Schließen").into(), icon: icon(|c| tool_icons::x(c).into_any_element()), last: true };
    let buttons = vec![
        tool_button(pen, t, a, cx.listener(move |this, _, window, cx| this.open_rename(super::rename::Target::Session(rename.clone()), window, cx))),
        tool_button(cross, t, a, cx.listener(move |this, ev: &ClickEvent, window, cx| this.request_close_session(&close, ev.modifiers().alt, window, cx))),
    ];
    toolbar(host, row_height, buttons, t, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Neue Session`: Remote zeigt den Server (auch mit ⌘), sonst Plus, mit ⌘ der Computer.
    #[test]
    fn new_session_icon_follows_remote_and_command() {
        assert_eq!(new_session_icon(false, false), NewIcon::Plus);
        assert_eq!(new_session_icon(false, true), NewIcon::Computer);
        assert_eq!(new_session_icon(true, false), NewIcon::Server);
        assert_eq!(new_session_icon(true, true), NewIcon::Server);
    }
}
