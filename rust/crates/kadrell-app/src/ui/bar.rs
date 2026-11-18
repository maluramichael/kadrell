//! Obere Leiste (Tasten links, Nutzung und Status rechts) und unter Windows die eigene Titelzeile darüber
//! (Menüs links, Titel in der Mitte, Fensterknöpfe rechts). Auf macOS liefert das System die Titelleiste.
//! Die Tasten der linken Seite folgen `StatusBarView.drawLeft`: Layout (mit Menü), Spalten, Teilrichtung, AUTO, SYNC,
//! GRP/FLACH, SORT und ZOOM.

use gpui::{
    AnyElement, App, Entity, Hsla, InteractiveElement, IntoElement, ParentElement, Role, StatefulInteractiveElement, Styled, Window,
    WindowControlArea, div, px,
};

use super::bar_keys::{self, Flash, Press, Region};
use super::chrome::{h_seam, window_controls};
use super::icons;
use super::layout::LayoutMode;
use super::menubar::{MenuBar, TITLE_HEIGHT};
use super::pill::{Pill, PillKind};
use super::theme;
use super::usage_bar::{self, BarCtl};
use kadrell_core::SidebarSort;
use kadrell_i18n::trd;

/// Rand der Leiste links (Swift: `edge`).
const LEFT_INSET: f32 = 6.;

/// Eine Taste der linken Seite. Welche da sind, hängt vom Layout ab (Spalten im Grid und Scrollen, TEILT in Frei)
/// und vom Zustand (ZOOM nur bei Zoom). `Cols` ist nur Text zwischen den Pfeilen, keine Taste.
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
    Sort,
    Zoom,
}

impl Key {
    /// Kennung für das Nachleuchten nach dem Drücken.
    fn id(self) -> &'static str {
        match self {
            Key::Layout => "layout",
            Key::Less => "less",
            Key::Cols => "cols",
            Key::More => "more",
            Key::Split => "split",
            Key::Auto => "auto",
            Key::Sync => "sync",
            Key::Group => "group",
            Key::Sort => "sort",
            Key::Zoom => "zoom",
        }
    }
}

fn left_keys(ctl: &LayoutCtl) -> Vec<Key> {
    let mut keys = vec![Key::Layout];
    if ctl.cols_label.is_some() {
        keys.extend([Key::Less, Key::Cols, Key::More]);
    }
    if ctl.split.is_some() {
        keys.push(Key::Split);
    }
    keys.extend([Key::Auto, Key::Sync, Key::Group, Key::Sort]);
    if ctl.zoom {
        keys.push(Key::Zoom);
    }
    keys
}

fn sort_text(sort: SidebarSort) -> String {
    match sort {
        SidebarSort::Off => "SORT".into(),
        SidebarSort::Alpha => "A–Z".into(),
        SidebarSort::Status => "STATUS".into(),
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

/// Text der Taste (die Pfeile und das Layout-Symbol sind gezeichnet).
fn key_label(key: Key, ctl: &LayoutCtl) -> String {
    match key {
        Key::Layout => String::new(),
        Key::Less | Key::More => String::new(),
        Key::Cols => ctl.cols_label.clone().unwrap_or_default(),
        Key::Split => split_label(ctl.split.unwrap_or('a')),
        Key::Auto => "AUTO".into(),
        Key::Sync => "SYNC".into(),
        Key::Group => if ctl.grouped { "GRP".into() } else { trd!("FLACH") },
        Key::Sort => sort_text(ctl.sort),
        Key::Zoom => "ZOOM".into(),
    }
}

/// Beschriftung und Wert der Trefferfläche (Tooltip „Label: Wert“, Bildschirmleser).
fn region_of(key: Key, ctl: &LayoutCtl) -> Region {
    let spoken_cols = Some(bar_keys::columns_spoken(ctl.cols));
    match key {
        Key::Layout => Region::new(trd!("Layout"), Some(bar_keys::layout_title(ctl.mode))),
        Key::Less => Region::new(trd!("Weniger Spalten"), spoken_cols),
        Key::More => Region::new(trd!("Mehr Spalten"), spoken_cols),
        Key::Cols => Region::new("", None),
        Key::Split => Region::new(trd!("Nächste Kachel teilt"), Some(bar_keys::split_spoken(ctl.split.unwrap_or('a')))),
        Key::Auto => Region::new(trd!("Auto-Modus"), Some(bar_keys::on_off(ctl.auto))),
        Key::Sync => Region::new("Sync", Some(bar_keys::on_off(ctl.sync))),
        Key::Group => Region::new(trd!("Gruppierung nach Projekt"), Some(bar_keys::on_off(ctl.grouped))),
        Key::Sort => Region::new(trd!("Sortierung"), Some(if ctl.sort == SidebarSort::Off { trd!("aus") } else { sort_text(ctl.sort) })),
        Key::Zoom => Region::new(trd!("Zoom aufheben"), None),
    }
}

/// Alle Trefferflächen der linken Seite in der Reihenfolge der Tasten.
pub fn left_regions(ctl: &LayoutCtl) -> Vec<Region> {
    left_keys(ctl).into_iter().filter(|k| *k != Key::Cols).map(|k| region_of(k, ctl)).collect()
}

fn press_of(key: Key, ctl: &LayoutCtl) -> Press {
    match key {
        Key::Layout => ctl.pick_layout.clone(),
        Key::Less => ctl.dec.clone(),
        Key::More => ctl.inc.clone(),
        Key::Split => ctl.cycle_split.clone(),
        Key::Auto => ctl.toggle_auto.clone(),
        Key::Sync => ctl.toggle_sync.clone(),
        Key::Group => ctl.toggle_grouped.clone(),
        Key::Sort => ctl.cycle_sort.clone(),
        Key::Zoom | Key::Cols => ctl.toggle_zoom.clone(),
    }
}

pub type Handler = std::rc::Rc<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;

/// Zustand und Aktionen der Tasten der linken Seite. Die Aktionen feuern beim Drücken (`bar_keys::Press`).
#[derive(Clone)]
pub struct LayoutCtl {
    pub mode: LayoutMode,
    /// Beschriftung der Spaltentaste im Grid und beim Scrollen, sonst keine Spaltentasten.
    pub cols_label: Option<String>,
    /// Spaltenzahl dazu (0 = automatisch), für Tooltip und Bildschirmleser.
    pub cols: usize,
    /// Klick aufs Layout-Symbol: Menü mit allen Layouts.
    pub pick_layout: Press,
    pub dec: Press,
    pub inc: Press,
    /// Frei: Teilrichtung an der Fokus-Kachel (`r`, `d`, `a`), sonst keine TEILT-Taste.
    pub split: Option<char>,
    pub cycle_split: Press,
    /// AUTO (gelb), SYNC (rot) und ZOOM (gelb) sind an, wenn sie leuchten; ein Druck schaltet um.
    pub auto: bool,
    pub sync: bool,
    pub zoom: bool,
    pub toggle_auto: Press,
    pub toggle_sync: Press,
    pub toggle_zoom: Press,
    /// Gruppierung des Baums (GRP/FLACH) und seine Sortierung (SORT, A–Z, STATUS reihum).
    pub grouped: bool,
    pub sort: SidebarSort,
    pub toggle_grouped: Press,
    pub cycle_sort: Press,
    /// Die zuletzt gedrückte Taste sitzt noch eingesunken da (80 ms), und wie man sie meldet.
    pub pressed: Option<&'static str>,
    pub flash: Flash,
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

/// Beschriftung in einer Farbe, die nicht die der Kappe ist (gedämpft aus, Teilrichtung).
fn colored(text: String, color: Hsla) -> impl IntoElement {
    div().text_color(color).child(text)
}

/// Schalter: an = eingesunken in `face`, aus = erhaben mit gedämpfter Schrift.
fn switch(id: usize, label: String, on: bool, face: Hsla, t: &crate::theme::Theme) -> Pill {
    let pill = Pill::new(("bar", id), label.clone());
    if on { pill.kind(PillKind::Active).fill(face) } else { pill.icon(colored(label, t.muted)) }
}

/// Breite der Pfeiltasten: Glyphe in 12 px fett plus 12 px Rand (Swift: `arrowKey`).
const ARROW_WIDTH: f32 = 20.;

/// Pfeil der Spaltentasten als Textglyphe „‹“ bzw. „›“, 12 px fett in `color` (Swift: `arrowKey`).
fn arrow_glyph(glyph: &'static str, color: Hsla) -> impl IntoElement {
    div().text_size(px(12.)).font_weight(gpui::FontWeight::BOLD).text_color(color).child(glyph)
}

/// Die Kappe einer Taste, noch ohne Trefferfläche.
fn cap(i: usize, key: Key, ctl: &LayoutCtl, t: &crate::theme::Theme) -> Pill {
    let label = key_label(key, ctl);
    let pill = match key {
        Key::Layout => Pill::new(("bar", i), "").icon(icons::layout(ctl.mode, t.sub)).min_width(26.),
        Key::Less => Pill::new(("bar", i), "").icon(arrow_glyph("‹", t.sub)).min_width(ARROW_WIDTH),
        Key::More => Pill::new(("bar", i), "").icon(arrow_glyph("›", t.sub)).min_width(ARROW_WIDTH),
        Key::Split => {
            let dir = ctl.split.unwrap_or('a');
            Pill::new(("bar", i), label.clone()).icon(colored(label, if dir == 'a' { t.muted } else { t.fg }))
        }
        Key::Auto => switch(i, label, ctl.auto, t.waiting, t),
        Key::Sync => switch(i, label, ctl.sync, t.error, t),
        Key::Group => switch(i, label, !ctl.grouped, t.sub, t),
        Key::Sort => switch(i, label, ctl.sort != SidebarSort::Off, t.sub, t),
        Key::Zoom => Pill::new(("bar", i), label).kind(PillKind::Active).fill(t.waiting),
        Key::Cols => Pill::new(("bar", i), label),
    };
    sunk_while_pressed(pill, key, ctl, t)
}

/// Nach dem Druck sitzt die Taste kurz eingesunken, auch wenn sie sonst erhaben ist.
fn sunk_while_pressed(pill: Pill, key: Key, ctl: &LayoutCtl, t: &crate::theme::Theme) -> Pill {
    if ctl.pressed == Some(key.id()) { pill.kind(PillKind::Active).fill(t.surface) } else { pill }
}

/// Spaltenzahl zwischen den Pfeilen: Text, keine Taste.
fn cols_text(ctl: &LayoutCtl, t: &crate::theme::Theme) -> AnyElement {
    let color = if ctl.cols == 0 { t.muted } else { t.fg };
    div().flex_none().mx(px(2.)).text_size(px(10.)).font_weight(gpui::FontWeight::BOLD).text_color(color).child(key_label(Key::Cols, ctl)).into_any_element()
}

fn left_key(i: usize, key: Key, ctl: &LayoutCtl, t: &crate::theme::Theme) -> AnyElement {
    if key == Key::Cols {
        return cols_text(ctl, t);
    }
    let pill = cap(i, key, ctl, t);
    bar_keys::press_key(("bar-hit", i), key.id(), &region_of(key, ctl), pill, &press_of(key, ctl), &ctl.flash).into_any_element()
}

fn left_pills(ctl: &LayoutCtl, t: &crate::theme::Theme) -> impl IntoElement + use<> {
    let pills = left_keys(ctl).into_iter().enumerate().map(|(i, k)| left_key(i, k, ctl, t));
    div().flex().flex_none().items_center().h_full().gap(px(4.)).children(pills)
}

/// Geschätzte Breite der linken Tasten samt Rand (Schrift 10 px fett, dicktengleich: 6 px je Zeichen, Kappe 18 px mehr).
fn left_width(ctl: &LayoutCtl) -> f32 {
    let width = |key: Key| match key {
        Key::Layout => 26.,
        Key::Less | Key::More => ARROW_WIDTH,
        Key::Cols => key_label(key, ctl).chars().count() as f32 * 6. - 14.,
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

/// Höhe der Zeile über der Fuge: `round(30 * Größe)` insgesamt, davon 2 px für die Fuge (`Theme.barHeight`).
fn row_height(scale: f32) -> f32 {
    crate::appearance::metrics::bar_height(scale) - 2.
}

/// Die Leiste mit Layout-Tasten, Nutzung und Status, erste Zeile unter der Titelleiste. Eine einzige Fläche für
/// Bildschirmleser (Rolle Werkzeugleiste, „Statusleiste“), die Tasten darin sind Knöpfe.
pub fn bar(clock: &str, ctl: &LayoutCtl, cx: &App) -> impl IntoElement + use<> {
    let t = theme(cx);
    let row = div()
        .id("status-bar")
        .role(Role::Toolbar)
        .aria_label(trd!("Statusleiste"))
        .flex()
        .items_center()
        .h(px(row_height(crate::appearance::current(cx).scale)))
        .w_full()
        .pl(px(LEFT_INSET))
        .child(left_pills(ctl, &t))
        .child(div().flex_1().min_w(px(24.)))
        .child(right_side(clock, ctl, cx));
    div().flex().flex_col().flex_none().bg(t.panel).child(row).child(h_seam(&t))
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::*;

    fn press() -> Press {
        Rc::new(|_, _, _| {})
    }

    pub(crate) fn ctl(mode: LayoutMode) -> LayoutCtl {
        LayoutCtl {
            mode,
            cols_label: matches!(mode, LayoutMode::Grid | LayoutMode::Scroll).then(|| "AUTO SP".to_string()),
            cols: 0,
            pick_layout: press(),
            dec: press(),
            inc: press(),
            split: (mode == LayoutMode::Custom).then_some('a'),
            cycle_split: press(),
            auto: false,
            sync: false,
            zoom: false,
            toggle_auto: press(),
            toggle_sync: press(),
            toggle_zoom: press(),
            grouped: true,
            sort: SidebarSort::Off,
            toggle_grouped: press(),
            cycle_sort: press(),
            pressed: None,
            flash: Rc::new(|_, _| {}),
            running: 0,
            detached: 0,
            waiting: 0,
            ext: Vec::new(),
            usage: None,
        }
    }

    fn labels(ctl: &LayoutCtl) -> Vec<String> {
        left_regions(ctl).into_iter().map(|r| r.label).collect()
    }

    /// Swift: `testStatusBarButtons`, die linke Hälfte der Liste (die rechte steht in `usage_bar`).
    #[test]
    fn the_grid_bar_lists_layout_columns_and_the_switches() {
        let regions = left_regions(&LayoutCtl { sync: true, ..ctl(LayoutMode::Grid) });
        let labels: Vec<&str> = regions.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Layout", "Weniger Spalten", "Mehr Spalten", "Auto-Modus", "Sync", "Gruppierung nach Projekt", "Sortierung"]);
        assert_eq!(regions[4].value.as_deref(), Some("an"), "Sync an");
        assert_eq!(regions[5].value.as_deref(), Some("an"), "Gruppierung ist im Default an");
        assert_eq!(regions[1].value.as_deref(), Some("automatisch"));
        assert_eq!(regions[0].value.as_deref(), Some("Grid"));
    }

    /// Swift: `testStatusBarButtons` vollständig, links und rechts zusammen.
    #[test]
    fn the_whole_bar_lists_its_buttons_in_reading_order() {
        let right = usage_bar::tests::test_ctl();
        let right = BarCtl { waiting: 2, ..right };
        let mut all = left_regions(&LayoutCtl { sync: true, ..ctl(LayoutMode::Grid) });
        all.extend(usage_bar::right_regions(&right, &crate::usage::logic::BarPlan::new(0)));
        let labels: Vec<&str> = all.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Layout", "Weniger Spalten", "Mehr Spalten", "Auto-Modus", "Sync", "Gruppierung nach Projekt", "Sortierung", "Wartende Sessions", "Status"]
        );
        assert_eq!(all[7].value.as_deref(), Some("2"));
    }

    #[test]
    fn split_only_in_free_and_zoom_only_while_zoomed() {
        assert!(labels(&ctl(LayoutMode::Custom)).contains(&"Nächste Kachel teilt".to_string()));
        assert!(!labels(&ctl(LayoutMode::Grid)).contains(&"Nächste Kachel teilt".to_string()));
        assert!(!labels(&ctl(LayoutMode::Stack)).contains(&"Weniger Spalten".to_string()));
        let zoomed = labels(&LayoutCtl { zoom: true, ..ctl(LayoutMode::Stack) });
        assert_eq!(zoomed.last().map(String::as_str), Some("Zoom aufheben"));
    }

    #[test]
    fn grouping_reads_grp_or_flach_and_sorting_cycles_its_label() {
        let c = ctl(LayoutMode::Grid);
        assert_eq!(key_label(Key::Group, &c), "GRP");
        assert_eq!(key_label(Key::Group, &LayoutCtl { grouped: false, ..c.clone() }), "FLACH");
        let texts: Vec<String> = [SidebarSort::Off, SidebarSort::Alpha, SidebarSort::Status].map(|s| key_label(Key::Sort, &LayoutCtl { sort: s, ..c.clone() })).to_vec();
        assert_eq!(texts, ["SORT", "A–Z", "STATUS"]);
        let sorted = region_of(Key::Sort, &LayoutCtl { sort: SidebarSort::Alpha, ..c.clone() });
        assert_eq!(sorted.tooltip(), "Sortierung: A–Z");
        assert_eq!(region_of(Key::Sort, &c).tooltip(), "Sortierung: aus");
        assert_eq!(region_of(Key::Group, &c).tooltip(), "Gruppierung nach Projekt: an");
    }

    #[test]
    fn columns_and_split_speak_their_value() {
        let c = LayoutCtl { cols: 3, ..ctl(LayoutMode::Grid) };
        assert_eq!(region_of(Key::Less, &c).tooltip(), "Weniger Spalten: 3");
        let free = LayoutCtl { split: Some('r'), ..ctl(LayoutMode::Custom) };
        assert_eq!(region_of(Key::Split, &free).tooltip(), "Nächste Kachel teilt: rechts");
        assert_eq!(key_label(Key::Split, &free), "TEILT →");
    }

    #[test]
    fn the_bar_height_is_thirty_scaled_with_the_seam_inside() {
        assert_eq!(row_height(1.), 28.);
        assert_eq!(row_height(1.3), 37.);
    }
}
