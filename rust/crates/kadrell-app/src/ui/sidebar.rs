//! Linke Seitenleiste: der Baum der Gruppen und Sessions in einem der beiden Designs der Mac-App (Klassisch mit Pfad
//! unter dem Gruppennamen, Kompakt ohne Pfad mit Status-Punkten im eingeklappten Kopf und Fuge zwischen den Gruppen).
//! Maße kommen aus `appearance::metrics`, alles wird mit der UI-Größe multipliziert und folgt Design, Laufzeit-Anzeige
//! und Deckkraft der Einstellungen sofort. Der Baum ist für Bildschirmleser eine Gliederung (`appearance::a11y`).
//! Dazu die Bedienung der Zeilen: schwebende Leiste (`tree_toolbar`), Mittelklick schließt, Ziehen sortiert um oder
//! wirft eine Session in eine andere Gruppe, Ordner aus dem Explorer starten eine Session (Regeln in `groups::drag`).
//! Hat der Baum die Tastatur (⌘1), trägt die Fokus-Zeile den Tastaturring in der Akzentfarbe (`metrics::key_focus_ring`).
//! Regeln ohne GPUI (Farben, Pfad kürzen, Bewegung) stehen in `tree_rules`, die Bewegung in `tree_anim`.

use gpui::{
    AppContext, Context, Div, ExternalPaths, FontWeight, HighlightStyle, Hsla, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    MouseUpEvent, ParentElement, Role, ScrollHandle, Stateful, StatefulInteractiveElement, Styled, StyledText, div, px,
};
use kadrell_i18n::trd;

use super::chrome::{h_seam, v_seam};
use super::groups::drag::{DragRow, Dragged, Over, TreeView, can_drag, plan};
use super::groups::files;
use super::groups::rows::{DragGhost, Mark, RowKey, block_mark, middle_released, row_mark};
use super::icons;
use super::layout::Selection;
use super::model::{Group, Session, Status};
use super::theme;
use super::tree_anim::{AnimSnapshot, RowAnim};
use super::tree_icons::{self, TypeIcon, heart_filled, type_icon, type_icon_element};
use super::tree_rules::{self, MONO_ADVANCE};
use super::tree_toolbar::{Spot, group_toolbar, session_toolbar};
use crate::Shell;
use crate::appearance::metrics::{self, ROW_INSET};
use crate::appearance::{Appearance, a11y, breathing};
use crate::theme::Theme;

/// Was der Baum von der Shell braucht, ohne sie lesen zu können (sie wird gerade gezeichnet).
pub struct SidebarCtx {
    pub view: TreeView,
    pub scroll: ScrollHandle,
    /// Die Tastatur gehört dem Baum (⌘1).
    pub keys: bool,
    /// Bewegung der Zeilen in diesem Bild: Aufblitzen und Einfahren.
    pub anim: AnimSnapshot,
    /// ⌘ (Mac) bzw. Strg (Windows) gehalten: „Neue Session“ der Gruppen zeigt das Terminal-Symbol.
    pub cmd: bool,
}

const OUTLINE: f32 = 1.5;
/// Rand der Zeile oben und unten mit Tastaturring (`sessionShape`: Ring und Luft 2,5 pt, minus 0,5 pt Luft zur Kante).
const RING_ROW_INSET_Y: f32 = 3.;
const RING_WIDTH: f32 = 2.;
const RING_RADIUS: f32 = 8.;
/// Breite der Chevron-Fläche: ein Klick auf x < 26 klappt die Gruppe auf der Maustaste (`mouseDown`).
const CHEVRON_W: f32 = 20.;

/// Zeilenrahmen mit optionalem Auswahlbalken (3 px links, volle Zeilenhöhe) und der Fläche darin.
fn row_frame(id: impl Into<gpui::ElementId>, height: f32, inset_y: f32, bar: Option<Hsla>, a: &Appearance) -> Stateful<Div> {
    let mut row = div()
        .id(id)
        .relative()
        .flex_none()
        .w_full()
        .h(px(a.s(height)))
        .px(px(a.s(ROW_INSET.0)))
        .py(px(a.s(inset_y)))
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

/// Breite des Baums in Pixeln für das laufende Zeichnen (setzt `sidebar` aus `windows::split::tree_width`).
static WIDTH_PX: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Breite der Seitenleiste in unskalierten Punkten, wie der Baum sie zeichnet (für das Kürzen von Pfad und Nachricht).
fn width_pt(a: &Appearance) -> f32 {
    let w = f32::from_bits(WIDTH_PX.load(std::sync::atomic::Ordering::Relaxed));
    let w = if w > 0. { w } else { crate::windows::layout::DEFAULT_SIDE };
    w / a.scale
}

/// Anzahl Zeichen einer Textzeile der Größe `size` (unskaliert), die in `avail` Punkte (unskaliert) passen.
fn chars_in(avail: f32, size: f32) -> usize {
    tree_rules::fits(avail, size * MONO_ADVANCE)
}

/// Gesperrtes Versal-Label mit Zahl: `KADRELL · 2`. Nur der Name wird gekürzt, die Zahl bleibt ganz sichtbar.
/// Die Sperrung (0,8 pt) fehlt: GPUI kennt keinen Buchstabenabstand.
fn group_label(group: &Group, t: &Theme, a: &Appearance, gap_right: f32) -> impl IntoElement + use<> {
    let (name, count) = tree_rules::group_label_parts(&group.name, group.sessions.len());
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .mr(px(a.s(gap_right)))
        .text_size(px(a.s(10.5)))
        .font_weight(FontWeight::BOLD)
        .text_color(t.sub)
        .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(name))
        .child(div().flex_none().whitespace_nowrap().child(count))
}

/// Chevron als eigene Klickfläche: klappt die Gruppe beim Drücken der Maustaste auf oder zu (`mouseDown`, nicht erst
/// beim Loslassen), ohne sie zu markieren oder zu ziehen.
fn chevron_button(g: usize, group: &Group, t: &Theme, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    div()
        .id(("chevron", g))
        .flex_none()
        .w(px(a.s(CHEVRON_W)))
        .child(icons::chevron_scaled(!group.collapsed, t.muted, a.scale))
        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            this.toggle_group(g, cx);
        }))
}

/// Sessions der Gruppe, die auf dich warten, auch bei eingeklappter Gruppe.
fn waiting(group: &Group) -> usize {
    group.sessions.iter().filter(|s| s.status == Status::Waiting).count()
}

/// „2 ⏳“ links vom Favoriten, nur wenn etwas wartet.
fn waiting_badge(group: &Group, t: &Theme, a: &Appearance, gap_right: f32) -> Option<Div> {
    let n = waiting(group);
    (n > 0).then(|| div().flex_none().mr(px(a.s(gap_right))).text_size(px(a.s(10.5))).font_weight(FontWeight::BOLD).text_color(t.waiting_text).child(format!("{n} ⏳")))
}

/// Favoritenherz ganz rechts in der Kopfzeile: gefüllt, 12 x 12, Gruppenfarbe.
fn favorite(group: &Group, t: &Theme, a: &Appearance) -> Option<Div> {
    group.favorite.then(|| div().flex_none().child(heart_filled(t.group(group.color), a.s(12.))))
}

/// Kompakt, eingeklappt: ein Punkt je Session, so bleibt der Status sichtbar. Aufgeklappt zeigen ihn die Zeilen selbst.
fn status_dots(group: &Group, t: &Theme, a: &Appearance, gap_right: f32) -> impl IntoElement + use<> {
    let dots = group.sessions.iter().map(|s| div().flex_none().size(px(a.s(5.))).rounded_full().bg(s.dot_color(t)));
    div().flex().flex_none().items_center().gap(px(a.s(3.))).mr(px(a.s(gap_right))).children(dots)
}

fn group_head(group: &Group, t: &Theme, a: &Appearance, chevron: gpui::AnyElement) -> impl IntoElement + use<> {
    let m = a.rows();
    let compact_dots = !m.path && group.collapsed;
    let n_waiting = waiting_badge(group, t, a, 0.).is_some();
    let gaps = tree_rules::head_gaps(group.favorite, n_waiting, compact_dots, !m.path);
    div()
        .flex()
        .items_center()
        .pr(px(a.s(4.)))
        .h(px(a.s(m.group_head() - 2. * ROW_INSET.1)))
        .child(chevron)
        .child(div().mr(px(a.s(5.))).size(px(a.s(6.))).flex_none().rounded(px(a.s(1.5))).bg(t.group(group.color)))
        .child(group_label(group, t, a, gaps.label))
        .children(compact_dots.then(|| status_dots(group, t, a, gaps.dots)))
        .children(waiting_badge(group, t, a, gaps.badge))
        .children(favorite(group, t, a))
}

/// Pfadzeile der Klassisch-Zeile: Remote-Gruppen zeigen „ssh · tmux“ (steht in Swift ohne Übersetzung), sonst den Pfad,
/// bei Platzmangel von vorn auf Anfangsbuchstaben gekürzt (`Theme.fitPath`).
fn path_text(group: &Group, a: &Appearance) -> String {
    if group.host.is_some() {
        return "ssh · tmux".to_string();
    }
    let avail = width_pt(a) - metrics::RIGHT_INSET - metrics::GROUP_PATH_X;
    tree_rules::fit_path(&group.path, avail, |s| s.chars().count() as f32 * 10.5 * MONO_ADVANCE)
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
        .child(path_text(group, a))
}

// MARK: Ziehen und Ablegen

/// Einfügelinie oder Rahmen als Fläche, die nur beim Ziehen über `host` sichtbar wird (und nur, wo `show` zutrifft).
/// Die Linie läuft über die ganze Breite (`dropLineY`), der Rahmen sitzt wie `dropGroupShape` 3 pt innen.
fn mark_overlay(host: String, name: String, mark: Mark, show: impl Fn(&DragRow) -> bool + 'static, t: &Theme, a: &Appearance) -> Div {
    let base = div().group(name).absolute().can_drop(move |v, _, _| v.downcast_ref::<DragRow>().is_some_and(&show));
    let (line, tint) = (t.fg, t.accent.opacity(0.14));
    let ring = t.accent;
    match mark {
        Mark::Above => base.top_0().left_0().right_0().h(px(a.s(2.))).group_drag_over::<DragRow>(host, move |s| s.bg(line)),
        Mark::Below => base.bottom_0().left_0().right_0().h(px(a.s(2.))).group_drag_over::<DragRow>(host, move |s| s.bg(line)),
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

// MARK: Gruppenzeile

/// Eine Gruppenzeile braucht: Platz im Baum, ob irgendeine ihrer Sessions markiert ist und den Zustand von ⌘.
struct HeadRow {
    place: Place,
    holds_selection: bool,
}

fn group_header(h: HeadRow, group: &Group, ctx: &SidebarCtx, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let HeadRow { place: Place { g, head_order: order, .. }, holds_selection } = h;
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
    let mut row = row_frame(("group", g), m.group_row, ROW_INSET.1, bar, a)
        .group(host.clone())
        .role(Role::TreeItem)
        .aria_label(label)
        .aria_level(a11y::level(true, true))
        .aria_expanded(!group.collapsed)
        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| this.click_group(g, ev.modifiers(), window, cx)))
        .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            let id = this.groups[g].id.clone();
            if !id.is_empty() {
                let items = this.group_menu_items(&id);
                this.open_context(ev.position, items, cx);
            }
        }))
        .child(face);
    if real {
        row = row.child(group_toolbar(g, group, Spot { host, row_height: m.group_row, cmd: ctx.cmd }, &t, a, cx));
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

// MARK: Sessionzeile

fn new_badge(status: Status, height: f32, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    let face = if status == Status::Waiting { t.waiting } else { t.running };
    div()
        .flex()
        .flex_none()
        .items_center()
        .ml(px(a.s(8.)))
        .h(px(a.s(height)))
        .px(px(a.s(5.)))
        .rounded(px(a.s(3.)))
        .bg(face)
        .text_color(t.pill_text_on(face))
        .text_size(px(a.s(10.)))
        .font_weight(FontWeight::BOLD)
        .child(trd!("neu"))
}

/// Statuspunkt; arbeitende Sessions bekommen den atmenden Punkt (Warten und Fehler behalten den Punkt), auch fertige
/// mit noch laufendem Kommando. Mit „Unterscheiden ohne Farbe“ trägt die Form den Status.
/// TODO(paket-tiles): die Kachel (`tile.rs`) soll `Session::activity_color`/`dot_color` und diesen Punkt nutzen.
pub(crate) fn status_dot(session: &Session, t: &Theme, a: &Appearance) -> gpui::AnyElement {
    status_dot_with(session, None, t, a)
}

/// Wie `status_dot`, mit `flash` (Stärke des Weiß beim Statuswechsel, `dotColor`).
fn status_dot_with(session: &Session, flash: Option<f32>, t: &Theme, a: &Appearance) -> gpui::AnyElement {
    if let Some(color) = session.activity_color(t) {
        return div().flex_none().child(breathing::breathing_dot(color, a.s(metrics::DOT_SIZE), a.s(breathing::GLOW_RADIUS))).into_any_element();
    }
    let base = session.dot_color(t);
    let color = flash.map_or(base, |amount| tree_rules::flash_color(base, amount));
    let shape = tree_rules::dot_shape(session.status, a11y::differentiate_without_color());
    tree_icons::status_dot(shape, color, t.pill_text_on(color), a.s(metrics::DOT_SIZE), a.scale).into_any_element()
}

/// Typ-Symbol hinter dem Punkt: gedämpft, bei Hover oder Auswahl `fg`.
fn type_symbol(kind: TypeIcon, selected: bool, host: &str, n: usize, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    let size = a.s(12.);
    let hot = type_icon_element(kind, t.fg, size);
    if selected {
        return div().flex_none().mr(px(a.s(5.))).child(hot).into_any_element();
    }
    let group = host.to_string();
    let over = div().absolute().top_0().left_0().opacity(0.).group_hover(group, |s| s.opacity(1.)).child(hot);
    div().id(("type", n)).relative().flex_none().mr(px(a.s(5.))).child(type_icon_element(kind, t.muted, size)).child(over).into_any_element()
}

/// Titel der Session, ohne Gruppierung mit „projekt › “ in Gruppenfarbe davor.
fn title_text(session: &Session, t: &Theme) -> StyledText {
    let Some((project, hue)) = &session.project else { return StyledText::new(session.title.clone()) };
    let prefix = format!("{project} › ");
    let split = prefix.len();
    let bold = HighlightStyle { color: Some(t.group(*hue)), font_weight: Some(FontWeight::BOLD), ..Default::default() };
    StyledText::new(format!("{prefix}{}", session.title)).with_highlights([(0..split, bold)])
}

fn session_title(session: &Session, color: Hsla, host: &str, n: usize, t: &Theme) -> impl IntoElement + use<> {
    let hot = t.fg;
    let title = div()
        .id(("title", n))
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_color(color)
        .group_hover(host.to_string(), move |s| s.text_color(hot))
        .child(title_text(session, t));
    if session.is_new { title.font_weight(FontWeight::BOLD) } else { title }
}

/// Zeile mit der letzten Antwort (Einstellung „letzte Nachricht zeigen“): die letzten 300 Zeichen, 10,5 gedämpft,
/// vorn abgeschnitten. `top` ist der Versatz zur Kopfzeile, damit die Zeile bei y = Kopf − 4 beginnt.
fn message_line(message: &str, top: f32, t: &Theme, a: &Appearance) -> impl IntoElement + use<> {
    // 26 pt links, 10 pt rechts; die Breite des Baums ist fest.
    let chars = chars_in(width_pt(a) - metrics::SESSION_TEXT_X - metrics::RIGHT_INSET, 10.5);
    let text = tree_rules::truncate_head(&tree_rules::message_tail(message), chars);
    div()
        .flex_none()
        .h(px(a.s(14.)))
        .ml(px(a.s(metrics::SESSION_TEXT_X - metrics::DOT_X)))
        .mt(px(a.s(top)))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_size(px(a.s(10.5)))
        .text_color(t.muted)
        .child(text)
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
    /// Der Baum hat die Tastatur (⌘1): die Fokus-Session trägt den Tastaturring.
    keys: bool,
    anim: RowAnim,
}

impl SessionRow {
    /// Tastaturring: nur an der Fokus-Zeile, solange der Baum die Tastatur hat.
    fn ring(&self) -> bool {
        self.focused && self.keys
    }

    /// Rand der Zeilenfläche oben und unten (mit Ring rückt sie ein, siehe `metrics::session_shape`).
    fn inset_y(&self) -> f32 {
        if self.ring() { RING_ROW_INSET_Y } else { ROW_INSET.1 }
    }
}

/// Rand der Zeile: nur die Fokus-Session trägt einen Umriss in `fg`, markierte Zeilen nur Fläche und Balken.
fn session_edge(focused: bool, t: &Theme) -> Hsla {
    if focused { t.fg } else { gpui::transparent_black() }
}

/// Der 2 pt Tastaturring in der Akzentfarbe, außen um die Zeilenfläche (`keyFocusRing`), innerhalb der Zeile.
fn key_ring(t: &Theme, a: &Appearance) -> Div {
    let w = px(a.s(RING_WIDTH));
    div()
        .absolute()
        .top(px(a.s(0.5)))
        .bottom(px(a.s(0.5)))
        .left(px(a.s(3.5)))
        .right(px(a.s(3.5)))
        .rounded(px(a.s(RING_RADIUS)))
        .border_t(w)
        .border_b(w)
        .border_l(w)
        .border_r(w)
        .border_color(t.accent)
}

/// Höhe der Marke „neu“: 16, mit Tastaturring flacher, damit sie im Umriss bleibt (`drawUnreadPill`).
fn pill_height(inset_y: f32, a: &Appearance) -> f32 {
    16f32.min(a.rows().session_row - 2. * inset_y - 4.)
}

fn session_face(session: &Session, row: &SessionRow, host: &str, n: usize, t: &Theme, a: &Appearance) -> Div {
    let SessionRow { selected, focused, .. } = *row;
    let inset_y = row.inset_y();
    let session_row = a.rows().session_row;
    let bright = selected || focused || session.is_new;
    let title_color = if bright { t.fg } else { t.sub };
    let kind = type_icon(session.is_remote, session.is_shell, session.is_pi);
    let head_h = session_row - 2. * (inset_y + OUTLINE);
    let mut head = div()
        .flex()
        .items_center()
        .w_full()
        .h(px(a.s(head_h)))
        .child(div().flex_none().mr(px(a.s(6.))).child(status_dot_with(session, row.anim.flash, t, a)))
        .children(kind.map(|k| type_symbol(k, selected, host, n, t, a)))
        .child(session_title(session, title_color, host, n, t));
    if session.is_new {
        head = head.child(new_badge(session.status, pill_height(inset_y, a), t, a));
    }
    if a.show_age {
        head = head.child(div().flex_none().ml(px(a.s(8.))).text_size(px(a.s(10.5))).text_color(t.muted).child(session.age.clone()));
    }
    let face = div().size_full().rounded(px(a.s(6.))).flex().flex_col().pl(px(a.s(4.5))).pr(px(a.s(2.5))).child(head);
    if !a.show_message {
        return face;
    }
    let top = inset_y + OUTLINE - 4.;
    face.child(message_line(session.message.as_deref().unwrap_or(""), top, t, a))
}

fn session_row(row: SessionRow, group: &Group, ctx: &SidebarCtx, a: &Appearance, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    let t = theme(cx);
    let (g, s, selected, focused, order, grouped) = (row.g, row.s, row.selected, row.focused, row.order, row.grouped);
    let session = &group.sessions[s];
    let host = format!("row-{order}");
    let n = g * 100 + s;
    let mut face = session_face(session, &row, &host, n, &t, a).hover(move |st| st.bg(t.bg));
    // Der Rand steht immer, nur seine Farbe wechselt: Inhalt springt bei der Auswahl nicht.
    if selected || focused {
        face = face.bg(t.bg);
    }
    face = outline(face, session_edge(focused, &t), a);
    let bar = selected.then(|| t.group(group.color));
    let spoken = a11y::spoken(session.status, session.ended, session.shell_running);
    let key = session.key.clone();
    let (pressed, released) = (RowKey::Session(key.clone()), RowKey::Session(key.clone()));
    let (menu_key, close_key) = (key.clone(), key.clone());
    let drag = DragRow { item: Dragged::Session(key.clone()), group: group.id.clone(), order, label: session.title.clone() };
    let height = a.rows().session_height(a.show_message);
    let mut frame = row_frame(("session", n), height, row.inset_y(), bar, a);
    if let Some((dx, alpha)) = row.anim.slide {
        frame = frame.left(px(a.s(dx))).opacity(alpha);
    }
    let mut row_el = frame
        .group(host.clone())
        .role(Role::TreeItem)
        .aria_label(a11y::session_label(&session.title))
        .aria_value(a11y::session_value(&spoken, session.is_new, selected))
        .aria_level(a11y::level(false, grouped))
        .aria_selected(selected)
        .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| this.click_session(g, s, ev.modifiers(), window, cx)))
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
        .children(row.ring().then(|| key_ring(&t, a)))
        .child(face)
        .child(session_toolbar(n, &key, host.clone(), height, &t, a, cx));
    // Einfügelinie beim Umsortieren in der eigenen Gruppe (oder in der flachen Liste).
    for mark in [Mark::Above, Mark::Below] {
        let (k, gid, view) = (key.clone(), group.id.clone(), ctx.view);
        let show = move |d: &DragRow| row_mark(&plan(d, Over::Session { key: &k, group: &gid }, view), d, order) == Some(mark);
        row_el = row_el.child(mark_overlay(host.clone(), format!("{host}-{mark:?}"), mark, show, &t, a));
    }
    droppable(draggable(row_el, drag, ctx.view), Some(key), group.id.clone(), ctx.view, cx)
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
    let mut block = div().id(("block", g)).group(block_name.clone()).relative().flex().flex_col().flex_none().mt(px(gap));
    // Ohne Gruppierung gibt es keine Gruppenzeile, die Sessions stehen in einer flachen Liste (die Zeilennummern
    // der Anzeigereihenfolge zählen die Kopfzeile weiter mit).
    if grouped {
        let holds_selection = sel.items.iter().any(|&(gi, _)| gi == g);
        block = block.child(group_header(HeadRow { place, holds_selection }, group, ctx, a, cx));
    }
    if !group.collapsed {
        for s in 0..group.sessions.len() {
            let key = &group.sessions[s].key;
            let row = SessionRow {
                g,
                s,
                selected: sel.contains((g, s)),
                focused: sel.focus == (g, s),
                order: head_order + 1 + s,
                grouped,
                keys: ctx.keys,
                anim: ctx.anim.of(key),
            };
            block = block.child(session_row(row, group, ctx, a, cx));
        }
    }
    if group.id.is_empty() {
        return block;
    }
    block.children(block_overlays(&block_name, group, head_order, ctx, &t, a))
}

pub fn sidebar(groups: &[Group], sel: &Selection, ctx: &SidebarCtx, cx: &mut Context<Shell>) -> impl IntoElement + use<> {
    WIDTH_PX.store(crate::windows::split::tree_width(cx).to_bits(), std::sync::atomic::Ordering::Relaxed);
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
        .pb(px(a.s(6.)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::metrics::RowMetrics;

    fn appearance(show_message: bool) -> Appearance {
        Appearance { show_message, ..Appearance::default() }
    }

    /// `testUnreadPillStaysInsideKeyFocusOutline`: ohne Ring 16, mit Ring flacher, kompakt knapp.
    #[test]
    fn pill_gets_flatter_with_the_key_ring() {
        let a = appearance(false);
        assert_eq!(pill_height(ROW_INSET.1, &a), 16.);
        assert_eq!(pill_height(RING_ROW_INSET_Y, &a), 14., "klassisch: 24 - 6 - 4");
        let mut compact = a.clone();
        compact.sidebar = kadrell_core::settings::SidebarStyle::Compact;
        assert_eq!(compact.rows(), RowMetrics::COMPACT);
        assert_eq!(pill_height(RING_ROW_INSET_Y, &compact), 12.);
    }

    /// Ring und Umriss: der Rand oben und unten stimmt mit `metrics::session_shape` überein.
    #[test]
    fn ring_inset_matches_the_session_shape() {
        use crate::ui::layout::Rect;
        let row = Rect::new(0., 0., 200., 24.);
        let shape = metrics::session_shape(true, true, row);
        assert_eq!(shape.y, RING_ROW_INSET_Y);
        assert_eq!(metrics::session_shape(false, false, row).y, ROW_INSET.1);
        assert_eq!(metrics::key_focus_ring(row).x, 3.5, "Ring sitzt 3,5 pt von links");
    }

    /// Nur die Fokus-Zeile hat einen Umriss, eine bloß markierte nicht (Fläche und Balken genügen).
    #[test]
    fn only_the_focused_row_has_an_outline() {
        let t = Theme::mocha();
        assert_eq!(session_edge(true, &t), t.fg);
        assert_eq!(session_edge(false, &t), gpui::transparent_black());
    }

    #[test]
    fn row_height_grows_with_the_message_line() {
        assert_eq!(appearance(true).rows().session_height(true), 24. + 16.);
        assert_eq!(appearance(false).rows().session_height(false), 24.);
    }

    #[test]
    fn remote_groups_show_ssh_tmux_instead_of_the_path() {
        let mut g = crate::ui::model::test_group("a", &[]);
        g.path = "~/projekt".into();
        let a = appearance(false);
        assert_eq!(path_text(&g, &a), "~/projekt");
        g.host = Some("server".into());
        assert_eq!(path_text(&g, &a), "ssh · tmux");
    }

    #[test]
    fn long_paths_shrink_leading_folders() {
        let mut g = crate::ui::model::test_group("a", &[]);
        g.path = format!("~/{}/{}/projekt", "d".repeat(30), "p".repeat(30));
        let a = appearance(false);
        let text = path_text(&g, &a);
        assert!(text.ends_with("/projekt") && text.chars().count() < g.path.chars().count(), "{text}");
        assert!(text.starts_with("~/d/"), "{text}");
    }
}
