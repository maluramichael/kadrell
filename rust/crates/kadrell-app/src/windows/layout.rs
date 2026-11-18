//! Aufteilung eines Hauptfensters (`MainWindowController` der Mac-App): Baum links, darunter das Dock der Extensions,
//! die Arbeitsfläche, rechts die Seitenleiste der Extensions. GPUI hat kein Autosave für Trenner, deshalb stehen die
//! Breiten in den Einstellungen, mit denselben Schlüsseln wie in der Mac-App (`sidebar.width[.N]`,
//! `rightSidebar.width[.N]`, `rightSidebar.visible[.N]`). Alles rein rechnerisch, ohne GPUI testbar.

use kadrell_core::Settings;
use serde_json::{Value, json};

use super::model::key_for;

/// Baum und rechte Seitenleiste sind mindestens so breit; schmaler lassen sie sich nicht ziehen.
pub const MIN_SIDE: f32 = 120.;
/// Breite, solange nichts gespeichert ist (`restoreSidebarWidth`).
pub const DEFAULT_SIDE: f32 = 260.;
/// Mindesthöhen in der linken Spalte: der Baum oben und das Dock unten bleiben greifbar (`treeMin`, `dockMin`).
pub const TREE_MIN_H: f32 = 80.;
pub const DOCK_MIN_H: f32 = 60.;
/// Anteil des Docks beim ersten Erscheinen: unten etwa 220 pt, höchstens die halbe Spalte.
pub const DOCK_DEFAULT_H: f32 = 220.;
/// Breite der Griffzone um einen Trenner (`ThinSplitView.grabWidth`).
pub const GRAB: f32 = 8.;

pub const TREE_WIDTH_KEY: &str = "sidebar.width";
pub const RIGHT_WIDTH_KEY: &str = "rightSidebar.width";
pub const RIGHT_VISIBLE_KEY: &str = "rightSidebar.visible";
/// Das Dock unten in der linken Spalte (`KadrellLeftDock[.N]` ist in der Mac-App ein Autosave des Splits).
pub const DOCK_HEIGHT_KEY: &str = "leftDock.height";

/// Baum und rechte Seitenleiste sind je höchstens halb so breit wie das Fenster (nie unter dem Minimum).
pub fn max_side(window_width: f32) -> f32 {
    (window_width / 2.).max(MIN_SIDE)
}

/// Breite im erlaubten Bereich für ein Fenster dieser Breite.
pub fn clamp_side(width: f32, window_width: f32) -> f32 {
    if !width.is_finite() {
        return DEFAULT_SIDE.min(max_side(window_width));
    }
    width.max(MIN_SIDE).min(max_side(window_width))
}

/// Höhe des Docks im erlaubten Bereich: der Baum behält `TREE_MIN_H`, das Dock `DOCK_MIN_H`.
pub fn clamp_dock(height: f32, column_height: f32) -> f32 {
    let max = (column_height - TREE_MIN_H).max(DOCK_MIN_H);
    if height.is_finite() { height.clamp(DOCK_MIN_H, max) } else { DOCK_DEFAULT_H.clamp(DOCK_MIN_H, max) }
}

/// Höhe beim ersten Erscheinen: `max(Höhe - 220, Höhe / 2)` ist die Trennerlage, das Dock bekommt den Rest.
pub fn default_dock(column_height: f32) -> f32 {
    clamp_dock(DOCK_DEFAULT_H.min(column_height / 2.), column_height)
}

/// Das Dock unter dem Baum gibt es nur mit mindestens einem Panel (`applyLeftDock`).
pub fn show_dock(panels: usize) -> bool {
    panels > 0
}

/// Die rechte Seitenleiste gibt es nur mit Panels und solange der Nutzer sie nicht ausgeblendet hat (`applyRightSidebar`).
pub fn show_right(user_visible: bool, panels: usize) -> bool {
    user_visible && panels > 0
}

/// Zustand der Aufteilung eines Fensters; `index` ist die Nummer des Fensters (0 Hauptfenster).
#[derive(Debug, Clone, PartialEq)]
pub struct Sides {
    index: usize,
    /// Baum ausgeblendet (⌘B): die ganze linke Spalte fehlt, kein Trenner bleibt übrig. Nicht gespeichert.
    pub tree_hidden: bool,
    pub tree_w: f32,
    /// Breite, auf die der Baum beim nächsten Einblenden zurückkommt (`lastSidebarWidth`).
    last_tree_w: f32,
    /// Vom Nutzer nicht ausgeblendet (⌘⌥B), gespeichert pro Fenster. Ohne Panels fehlt der Bereich unabhängig davon.
    pub right_visible: bool,
    pub right_w: f32,
    /// Höhe des Docks; `None`, bis es zum ersten Mal erscheint (dann gilt `default_dock`).
    pub dock_h: Option<f32>,
}

/// Eine Änderung für `settings.json`: Schlüssel und neuer Wert.
pub type Change = (String, Value);

impl Sides {
    /// Stand aus den Einstellungen. Unlesbares oder Fehlendes ergibt die Voreinstellung (260 breit, rechts sichtbar).
    pub fn restore(settings: &Settings, index: usize) -> Sides {
        let width = |key: &str| settings.f64_or(&key_for(key, index), f64::from(DEFAULT_SIDE)) as f32;
        let tree_w = width(TREE_WIDTH_KEY).max(MIN_SIDE);
        Sides {
            index,
            tree_hidden: false,
            tree_w,
            last_tree_w: tree_w,
            right_visible: settings.bool_or(&key_for(RIGHT_VISIBLE_KEY, index), true),
            right_w: width(RIGHT_WIDTH_KEY).max(MIN_SIDE),
            dock_h: settings.get(&key_for(DOCK_HEIGHT_KEY, index)).and_then(Value::as_f64).map(|h| h as f32),
        }
    }

    /// Für Screenshots ohne Eingabe: `KADRELL_DEV_TREE_WIDTH=380` setzt die Breite des Baums, `KADRELL_DEV_HIDE_TREE=1`
    /// blendet ihn aus, `KADRELL_DEV_DOCK_HEIGHT=300` setzt die Höhe des Docks.
    pub fn apply_dev(&mut self) {
        let number = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<f32>().ok());
        if let Some(w) = number("KADRELL_DEV_TREE_WIDTH") {
            self.tree_w = w.max(MIN_SIDE);
            self.last_tree_w = self.tree_w;
        }
        self.tree_hidden = std::env::var_os("KADRELL_DEV_HIDE_TREE").is_some();
        self.dock_h = number("KADRELL_DEV_DOCK_HEIGHT").or(self.dock_h);
    }

    fn change(&self, key: &str, value: Value) -> Change {
        (key_for(key, self.index), value)
    }

    /// Baum ein/aus (⌘B): ausgeblendet kommt er auf der zuletzt genutzten Breite zurück (mindestens 120).
    pub fn toggle_tree(&mut self) {
        if self.tree_hidden {
            self.tree_w = self.last_tree_w;
        } else {
            self.last_tree_w = self.tree_w.max(MIN_SIDE);
        }
        self.tree_hidden = !self.tree_hidden;
    }

    /// Die rechte Seitenleiste ein/aus (⌘⌥B), gibt die zu speichernde Änderung zurück.
    pub fn toggle_right(&mut self) -> Change {
        self.right_visible = !self.right_visible;
        self.change(RIGHT_VISIBLE_KEY, json!(self.right_visible))
    }

    /// Trenner des Baums gezogen: `x` ist die Mausposition von der linken Kante der Fläche aus.
    pub fn drag_tree(&mut self, x: f32, window_width: f32) -> Change {
        self.tree_w = clamp_side(x, window_width);
        self.last_tree_w = self.tree_w;
        self.change(TREE_WIDTH_KEY, json!(f64::from(self.tree_w)))
    }

    /// Trenner der rechten Seitenleiste gezogen: `from_right` ist der Abstand der Maus vom rechten Rand der Fläche.
    pub fn drag_right(&mut self, from_right: f32, window_width: f32) -> Change {
        self.right_w = clamp_side(from_right, window_width);
        self.change(RIGHT_WIDTH_KEY, json!(f64::from(self.right_w)))
    }

    /// Trenner des Docks gezogen: `from_bottom` ist der Abstand der Maus vom unteren Rand der linken Spalte.
    pub fn drag_dock(&mut self, from_bottom: f32, column_height: f32) -> Change {
        let h = clamp_dock(from_bottom, column_height);
        self.dock_h = Some(h);
        self.change(DOCK_HEIGHT_KEY, json!(f64::from(h)))
    }

    /// Breite des Baums bei dieser Fensterbreite, so wie sie gezeichnet wird.
    pub fn tree_width(&self, window_width: f32) -> f32 {
        clamp_side(self.tree_w, window_width)
    }

    pub fn right_width(&self, window_width: f32) -> f32 {
        clamp_side(self.right_w, window_width)
    }

    /// Höhe des Docks, so wie sie gezeichnet wird.
    pub fn dock_height(&self, column_height: f32) -> f32 {
        match self.dock_h {
            Some(h) => clamp_dock(h, column_height),
            None => default_dock(column_height),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(pairs: &[(&str, Value)]) -> Settings {
        let mut s = Settings::default();
        for (k, v) in pairs {
            s.set(k, v.clone());
        }
        s
    }

    #[test]
    fn defaults_without_settings() {
        let s = Sides::restore(&Settings::default(), 0);
        assert_eq!((s.tree_w, s.right_w), (260., 260.));
        assert!(s.right_visible && !s.tree_hidden);
        assert_eq!(s.dock_h, None);
    }

    #[test]
    fn restores_per_window_keys() {
        let st = settings(&[("sidebar.width.2", json!(310.0)), ("rightSidebar.visible.2", json!(false)), ("rightSidebar.width.2", json!(200.0)), ("sidebar.width", json!(400.0))]);
        let s = Sides::restore(&st, 1);
        assert_eq!((s.tree_w, s.right_w), (310., 200.));
        assert!(!s.right_visible);
        assert_eq!(Sides::restore(&st, 0).tree_w, 400.);
        assert!(Sides::restore(&st, 0).right_visible, "Fenster 0 liest die Schlüssel ohne Suffix");
    }

    #[test]
    fn saved_width_below_the_minimum_comes_back_as_120() {
        let s = Sides::restore(&settings(&[("sidebar.width", json!(40.0))]), 0);
        assert_eq!(s.tree_w, MIN_SIDE);
    }

    #[test]
    fn tree_and_right_are_at_most_half_the_window() {
        assert_eq!(clamp_side(900., 1000.), 500.);
        assert_eq!(clamp_side(50., 1000.), 120.);
        assert_eq!(clamp_side(300., 1000.), 300.);
        assert_eq!(clamp_side(300., 100.), 120., "bei sehr schmalem Fenster gilt das Minimum");
        let s = Sides::restore(&settings(&[("sidebar.width", json!(700.0))]), 0);
        assert_eq!(s.tree_width(1000.), 500., "gezeichnet wird höchstens die halbe Breite");
        assert_eq!(s.tree_width(2000.), 700.);
    }

    /// `testHidingSidebarRemovesDividerAndRestoresWidth`: ausgeblendet kommt der Baum auf der alten Breite zurück.
    #[test]
    fn hiding_the_tree_keeps_the_width() {
        let mut s = Sides::restore(&Settings::default(), 0);
        s.drag_tree(333., 1000.);
        s.toggle_tree();
        assert!(s.tree_hidden);
        s.toggle_tree();
        assert!(!s.tree_hidden);
        assert_eq!(s.tree_w, 333.);
    }

    #[test]
    fn dragging_saves_under_the_window_suffix() {
        let mut s = Sides::restore(&Settings::default(), 2);
        assert_eq!(s.drag_tree(280., 1000.), ("sidebar.width.3".to_string(), json!(280.0)));
        assert_eq!(s.drag_right(10., 1000.), ("rightSidebar.width.3".to_string(), json!(120.0)), "nicht schmaler als 120");
        assert_eq!(s.toggle_right(), ("rightSidebar.visible.3".to_string(), json!(false)));
        assert_eq!(s.toggle_right().1, json!(true));
        let mut main = Sides::restore(&Settings::default(), 0);
        assert_eq!(main.drag_tree(2000., 1000.).0, "sidebar.width");
        assert_eq!(main.tree_w, 500.);
    }

    /// `testLeftDockDividerConstrainedAndGrabbable`: der Baum behält 80, das Dock 60.
    #[test]
    fn dock_keeps_its_minimum_heights() {
        assert_eq!(clamp_dock(10., 700.), DOCK_MIN_H);
        assert_eq!(clamp_dock(9999., 700.), 700. - TREE_MIN_H);
        assert_eq!(clamp_dock(200., 700.), 200.);
        assert_eq!(clamp_dock(200., 100.), DOCK_MIN_H, "in einer sehr niedrigen Spalte gewinnt das Dock-Minimum");
    }

    /// `testLeftDockAppearsWithPanelAndHidesWhenEmpty`: ohne Panel kein Dock, mit Panel erscheint es, leer ist es wieder weg.
    #[test]
    fn left_dock_appears_with_a_panel_and_hides_when_empty() {
        assert!(!show_dock(0), "ohne Panel kein Dock");
        assert!(show_dock(1), "mit Panel erscheint das Dock in der linken Spalte");
        assert!(!show_dock(0), "leeres Panel nimmt das Dock wieder raus");
    }

    #[test]
    fn right_sidebar_needs_panels_and_the_users_consent() {
        assert!(show_right(true, 2));
        assert!(!show_right(false, 2), "vom Nutzer ausgeblendet");
        assert!(!show_right(true, 0), "ohne Panels fehlt der Bereich");
    }

    #[test]
    fn dock_starts_at_220_or_half() {
        assert_eq!(default_dock(700.), 220.);
        assert_eq!(default_dock(300.), 150.);
        let mut s = Sides::restore(&Settings::default(), 0);
        assert_eq!(s.dock_height(700.), 220.);
        s.drag_dock(300., 700.);
        assert_eq!(s.dock_height(700.), 300.);
        assert_eq!(s.dock_height(200.), 120., "die Spalte wurde niedriger, der Baum behält 80");
    }

    #[test]
    fn dock_height_is_restored() {
        let st = settings(&[("leftDock.height", json!(180.0))]);
        assert_eq!(Sides::restore(&st, 0).dock_h, Some(180.));
        assert_eq!(Sides::restore(&Settings::default(), 0).dock_h, None);
    }
}
