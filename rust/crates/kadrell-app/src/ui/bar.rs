//! Obere Leiste (Tasten links, Nutzung und Status rechts) und unter Windows die eigene Titelzeile darüber
//! (Menüs links, Titel in der Mitte, Fensterknöpfe rechts). Auf macOS liefert das System die Titelleiste.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, Entity, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, Window, WindowControlArea, div,
    px,
};

use super::icons;
use super::layout::LayoutMode;
use super::chrome::{BAR_HEIGHT, h_seam, window_controls};
use super::menubar::{MenuBar, TITLE_HEIGHT};
use super::pill::{Pill, PillKind};
use super::usage_bar::{self, BarCtl};
use super::theme;
use kadrell_core::SidebarSort;
use kadrell_i18n::trd;

const LEFT_INSET: f32 = 8.;

/// Eine Taste der linken Seite. Welche da sind, hängt vom Layout ab (Spalten im Grid und Scrollen, TEILT in Frei)
/// und vom Zustand (ZOOM nur bei Zoom).
#[derive(Clone, Copy, PartialEq, Debug)]
enum Key {
    Layout,
    Less,
    Cols,
    More,
    Split,
    Auto,
    Sync,
    Group,
    Alpha,
    Status,
    Zoom,
}

fn left_keys(ctl: &LayoutCtl) -> Vec<Key> {
    let mut keys = vec![Key::Layout];
    if ctl.cols_label.is_some() {
        keys.extend([Key::Less, Key::Cols, Key::More]);
    }
    if ctl.split.is_some() {
        keys.push(Key::Split);
    }
    keys.extend([Key::Auto, Key::Sync, Key::Group, Key::Alpha, Key::Status]);
    if ctl.zoom {
        keys.push(Key::Zoom);
    }
    keys
}

/// Text der Taste (die Pfeile und das Layout-Symbol sind gezeichnet).
fn key_label(key: Key, ctl: &LayoutCtl) -> String {
    match key {
        Key::Layout => "▦".into(),
        Key::Less => "‹".into(),
        Key::More => "›".into(),
        Key::Cols => ctl.cols_label.clone().unwrap_or_default(),
        Key::Split => split_label(ctl.split.unwrap_or('a')),
        Key::Auto => "AUTO".into(),
        Key::Sync => "SYNC".into(),
        Key::Group => "GRP".into(),
        Key::Alpha => "A-Z".into(),
        Key::Status => "STATUS".into(),
        Key::Zoom => "ZOOM".into(),
    }
}

/// TEILT: wohin die nächste Kachel im Layout Frei entsteht (rechts, unten, entlang der längeren Seite).
fn split_label(dir: char) -> String {
    match dir {
        'r' => trd!("TEILT →"),
        'd' => trd!("TEILT ↓"),
        _ => trd!("TEILT AUTO"),
    }
}

pub type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Zustand und Aktionen der Layout-Tasten: Symbol (reihum), ‹ und › (Spaltenzahl im Grid).
#[derive(Clone)]
pub struct LayoutCtl {
    pub mode: LayoutMode,
    /// Beschriftung der Spaltentaste im Grid und beim Scrollen, sonst keine Spaltentasten.
    pub cols_label: Option<String>,
    pub cycle: Handler,
    pub dec: Handler,
    pub inc: Handler,
    /// Frei: Teilrichtung an der Fokus-Kachel (`r`, `d`, `a`), sonst keine TEILT-Taste.
    pub split: Option<char>,
    pub cycle_split: Handler,
    /// AUTO (gelb), SYNC (rot) und ZOOM (gelb) sind an, wenn sie leuchten; ein Klick schaltet um.
    pub auto: bool,
    pub sync: bool,
    pub zoom: bool,
    pub toggle_auto: Handler,
    pub toggle_sync: Handler,
    pub toggle_zoom: Handler,
    /// Gruppierung des Baums (GRP) und seine Sortierung (A-Z, STATUS).
    pub grouped: bool,
    pub sort: SidebarSort,
    pub toggle_grouped: Handler,
    pub toggle_alpha: Handler,
    pub toggle_status: Handler,
    /// Zähler rechts: laufende, getrennte (ohne Prozess) und wartende Sessions.
    pub running: usize,
    pub detached: usize,
    pub waiting: usize,
    /// Statuseinträge der Extensions, links neben den Nutzungswerten.
    pub ext: Vec<ExtStatus>,
    /// Konten, Nutzung und Status-Display der rechten Seite (`usage_bar`).
    pub usage: Option<BarCtl>,
}

/// Ein Statuseintrag einer Extension (`kadrell.status.set`): Text mit Punkt in der Farbe, mit Klick bei Aktion.
#[derive(Clone)]
pub struct ExtStatus {
    /// Name der Extension (Tooltip).
    pub name: String,
    pub text: String,
    pub dot: Option<Hsla>,
    pub click: Option<Handler>,
}

fn with_click(pill: Pill, h: &Handler) -> Pill {
    let h = h.clone();
    pill.on_click(move |e, w, c| h(e, w, c))
}

/// Eine Taste der linken Leiste: Layout-Symbol und Pfeile sind gezeichnet, der Rest Text.
fn left_pill(i: usize, key: Key, ink: Hsla, ctl: &LayoutCtl, t: &crate::theme::Theme) -> Pill {
    let pill = Pill::new(("bar", i), key_label(key, ctl));
    match key {
        Key::Layout => with_click(pill.icon(icons::layout(ctl.mode, ink)).min_width(26.), &ctl.cycle).tooltip_text(trd!("Nächstes Layout")),
        Key::Less => with_click(pill.icon(icons::arrow(true, ink)).min_width(22.), &ctl.dec).tooltip_text(trd!("Weniger Spalten")),
        Key::More => with_click(pill.icon(icons::arrow(false, ink)).min_width(22.), &ctl.inc).tooltip_text(trd!("Mehr Spalten")),
        Key::Cols => pill,
        Key::Split => with_click(pill, &ctl.cycle_split).tooltip_text(trd!("Nächste Kachel teilt")),
        Key::Auto => with_click(lit(pill, ctl.auto, t.waiting), &ctl.toggle_auto).tooltip_text(trd!("Auto-Modus")),
        Key::Sync => with_click(lit(pill, ctl.sync, t.error), &ctl.toggle_sync).tooltip_text("Sync"),
        Key::Group => with_click(toggled(pill, ctl.grouped), &ctl.toggle_grouped),
        Key::Alpha => with_click(toggled(pill, ctl.sort == SidebarSort::Alpha), &ctl.toggle_alpha),
        Key::Status => with_click(toggled(pill, ctl.sort == SidebarSort::Status), &ctl.toggle_status),
        Key::Zoom => with_click(pill.fill(t.waiting), &ctl.toggle_zoom).tooltip_text(trd!("Zoom aufheben")),
    }
}

fn toggled(pill: Pill, on: bool) -> Pill {
    if on { pill.kind(PillKind::Active) } else { pill }
}

/// Schalter, der in `face` leuchtet, solange er an ist.
fn lit(pill: Pill, on: bool, face: Hsla) -> Pill {
    if on { pill.fill(face) } else { pill }
}

fn left_pills(ink: Hsla, ctl: &LayoutCtl, t: &crate::theme::Theme) -> impl IntoElement + use<> {
    let pills = left_keys(ctl).into_iter().enumerate().map(|(i, k)| left_pill(i, k, ink, ctl, t));
    div().flex().flex_none().items_center().gap(px(4.)).children(pills)
}

/// Geschätzte Breite der linken Tasten samt Rand (Schrift 10 px fett, dicktengleich: 6 px je Zeichen, Kappe 18 px mehr).
fn left_width(ctl: &LayoutCtl) -> f32 {
    let width = |key: Key| match key {
        Key::Layout => 26.,
        Key::Less | Key::More => 22.,
        other => key_label(other, ctl).chars().count() as f32 * 6. + 18.,
    };
    LEFT_INSET + left_keys(ctl).into_iter().map(|k| width(k) + 4.).sum::<f32>()
}

/// Rechts: Konto, „N trennen“, „N warten“, Nutzung und das Status-Display (`usage_bar`). Ohne Anbindung (nur Tests)
/// bleibt die Seite leer.
fn right_side(clock: &str, ctl: &LayoutCtl, cx: &App) -> AnyElement {
    let Some(usage) = &ctl.usage else { return div().into_any_element() };
    let b = BarCtl { left: left_width(ctl), ext: ctl.ext.clone(), ..usage.clone() };
    usage_bar::right_side(clock, &b, &theme(cx)).into_any_element()
}


/// Die freie Fläche der Titelzeile mit dem Fenstertitel: zieht das Fenster.
fn title_area(cx: &App) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .h_full()
        .min_w(px(24.))
        .text_color(theme(cx).sub)
        .window_control_area(WindowControlArea::Drag)
        .child("Kadrell")
}

/// Windows: eigene Titelzeile im Kadrell-Stil, getrennt von der Leiste darunter. Sonst `None`.
pub fn titlebar(menubar: &Entity<MenuBar>, window: &Window, cx: &App) -> Option<impl IntoElement + use<>> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let t = theme(cx);
    let row = div()
        .flex()
        .items_center()
        .h(px(TITLE_HEIGHT - 2.))
        .w_full()
        .child(menubar.clone())
        .child(title_area(cx))
        .child(window_controls(window, cx));
    Some(div().flex().flex_col().flex_none().bg(t.bg).child(row).child(h_seam(&t)))
}

/// Die Leiste mit Layout-Tasten, Nutzung und Status, erste Zeile unter der Titelleiste.
pub fn bar(clock: &str, ctl: &LayoutCtl, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let row = div()
        .flex()
        .items_center()
        .h(px(crate::appearance::current(cx).s(BAR_HEIGHT - 2.)))
        .w_full()
        .pl(px(LEFT_INSET))
        .child(left_pills(t.sub, ctl, &t))
        .child(div().flex_1().min_w(px(24.)))
        .child(right_side(clock, ctl, cx));
    div().flex().flex_col().flex_none().bg(t.panel).child(row).child(h_seam(&t))
}
