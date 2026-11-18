//! Gemeinsame Bausteine der Tasten der Leiste (Swift: `StatusBarView.key/segment/hitRects`): Tasten feuern beim
//! Drücken (nicht beim Loslassen), die Trefferfläche ist die ganze Höhe der Leiste, der Tooltip heißt „Label: Wert“
//! (ohne Wert nur das Label) und die getroffene Taste sitzt danach 80 ms eingesunken (nicht bei „Bewegung
//! reduzieren“). Dieselbe Liste der Trefferflächen liefert Beschriftung und Wert für Bildschirmleser.

use std::rc::Rc;

use gpui::{App, ElementId, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Role, StatefulInteractiveElement, Styled, Window, div};
use kadrell_i18n::trd;

use super::layout::LayoutMode;
use super::tooltip;

/// Aktion einer Taste; bekommt das Drücken (die Menüs klappen darunter auf).
pub type Press = Rc<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

/// Meldet die gedrückte Taste (Kennung), damit sie 80 ms eingesunken gezeichnet wird.
pub type Flash = Rc<dyn Fn(&'static str, &mut App)>;

/// So lange sitzt die getroffene Taste eingesunken (Millisekunden).
pub const FLASH_MS: u64 = 80;

/// Trefferfläche mit Beschriftung und Wert, wie `HitRegion` in Swift.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Region {
    pub label: String,
    pub value: Option<String>,
}

impl Region {
    pub fn new(label: impl Into<String>, value: Option<String>) -> Region {
        Region { label: label.into(), value }
    }

    /// Tooltip: „Label: Wert“, ohne Wert nur das Label.
    pub fn tooltip(&self) -> String {
        match &self.value {
            Some(v) => format!("{}: {v}", self.label),
            None => self.label.clone(),
        }
    }
}

/// „an“ oder „aus“.
pub fn on_off(on: bool) -> String {
    if on { trd!("an") } else { trd!("aus") }
}

/// Titel eines Layouts (Menü und Tooltip der Layout-Taste).
pub fn layout_title(mode: LayoutMode) -> String {
    match mode {
        LayoutMode::Grid => trd!("Grid"),
        LayoutMode::Main => trd!("Haupt + Spalte"),
        LayoutMode::Spiral => trd!("Spirale"),
        LayoutMode::Custom => trd!("Frei"),
        LayoutMode::Scroll => trd!("Scrollen"),
        LayoutMode::Row => trd!("Reihe"),
        LayoutMode::Stack => trd!("Stack"),
    }
}

/// Vorgelesen für die Teilrichtung: automatisch, rechts, unten.
pub fn split_spoken(dir: char) -> String {
    match dir {
        'r' => trd!("rechts"),
        'd' => trd!("unten"),
        _ => trd!("automatisch"),
    }
}

/// Die nächste Teilrichtung: auto, rechts, unten, wieder auto.
pub fn next_split(dir: char) -> char {
    match dir {
        'a' => 'r',
        'r' => 'd',
        _ => 'a',
    }
}

/// Spalten als Wert: „automatisch“ oder die Zahl.
pub fn columns_spoken(cols: usize) -> String {
    if cols == 0 { trd!("automatisch") } else { cols.to_string() }
}

/// Eine Taste mit voller Höhe als Trefferfläche. `id` ist die Kennung für das Nachleuchten, `key` die Taste selbst.
pub fn press_key<E: Into<ElementId>, K: IntoElement>(element: E, id: &'static str, region: &Region, key: K, press: &Press, flash: &Flash) -> impl IntoElement + use<E, K> {
    let (press, flash) = (press.clone(), flash.clone());
    let mut el = div()
        .id(element)
        .flex()
        .flex_none()
        .items_center()
        .h_full()
        .cursor_pointer()
        .role(Role::Button)
        .aria_label(region.label.clone())
        .tooltip(tooltip::text(region.tooltip()))
        .on_mouse_down(MouseButton::Left, move |e, w, c| {
            flash(id, c);
            press(e, w, c);
        })
        .child(key);
    if let Some(v) = &region.value {
        el = el.aria_description(v.clone());
    }
    el
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tooltip_joins_label_and_value() {
        assert_eq!(Region::new("Sync", Some("an".into())).tooltip(), "Sync: an");
        assert_eq!(Region::new("Zoom aufheben", None).tooltip(), "Zoom aufheben");
    }

    #[test]
    fn the_split_cycles_auto_right_down() {
        assert_eq!([next_split('a'), next_split('r'), next_split('d')], ['r', 'd', 'a']);
        assert_eq!((split_spoken('r').as_str(), split_spoken('d').as_str(), split_spoken('a').as_str()), ("rechts", "unten", "automatisch"));
    }

    #[test]
    fn the_layout_menu_lists_all_seven_modes_in_order() {
        let titles: Vec<String> = LayoutMode::ALL.iter().map(|m| layout_title(*m)).collect();
        assert_eq!(titles, ["Grid", "Haupt + Spalte", "Spirale", "Frei", "Scrollen", "Reihe", "Stack"]);
    }

    #[test]
    fn columns_read_automatic_or_the_number() {
        assert_eq!((columns_spoken(0).as_str(), columns_spoken(3).as_str()), ("automatisch", "3"));
        assert_eq!((on_off(true).as_str(), on_off(false).as_str()), ("an", "aus"));
    }
}
