//! Auswahl, Fokus und Layout der weiteren Fenster und das Fenster, auf das Befehle wirken (`AppDelegate.current`).
//! Fenster 0 bleibt auf den Feldern des `AppState` (`selected`, `focused`, `layout`), jedes weitere Fenster hat hier seine
//! `View`. Befehle (Steuerkanal, Menü, neue Sessions) fragen über `cur_*` nach dem aktuellen Fenster und landen so in
//! dem, das zuletzt Fokus hatte. Alles ohne GPUI testbar.

use std::collections::BTreeMap;

use kadrell_control::LayoutMode;
use serde_json::json;

use super::model::key_for;
use crate::state::AppState;

/// Was ein weiteres Fenster zeigt: Auswahl (in Anzeigereihenfolge), Fokus und Layout.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    pub selected: Vec<String>,
    pub focus: Option<String>,
    pub mode: LayoutMode,
    /// Zoom: nur die Fokus-Kachel. Gehört dem Fenster, wird nicht gespeichert und endet mit einer anderen Auswahl.
    pub zoom: bool,
}

impl View {
    /// Setzt die Auswahl: Unbekanntes fällt weg, Doppelte auch, der Fokus gehört immer zur Auswahl.
    pub fn set(&mut self, keys: Vec<String>, focus: Option<String>, known: &dyn Fn(&str) -> bool) {
        let mut seen = std::collections::HashSet::new();
        let before = std::mem::take(&mut self.selected);
        self.selected = keys.into_iter().filter(|k| known(k) && seen.insert(k.clone())).collect();
        self.zoom &= before == self.selected;
        self.focus = focus.filter(|f| self.selected.contains(f)).or_else(|| self.selected.last().cloned());
    }

    /// Wie `AppState::select_keys`: ersetzen (`add` falsch) oder jede Session umschalten, die letzte bleibt immer; danach
    /// in Baumreihenfolge (`order`).
    pub fn select(&mut self, keys: &[String], add: bool, order: &[String], known: &dyn Fn(&str) -> bool) {
        if !add {
            let focus = keys.first().cloned();
            return self.set(keys.to_vec(), focus, known);
        }
        let mut selected = self.selected.clone();
        let mut focus = self.focus.clone();
        for k in keys {
            if let Some(i) = selected.iter().position(|x| x == k).filter(|_| selected.len() > 1) {
                selected.remove(i);
                if focus.as_deref() == Some(k) {
                    focus = selected.last().cloned();
                }
            } else if !selected.contains(k) {
                selected.push(k.clone());
                focus = Some(k.clone());
            }
        }
        selected.sort_by_key(|k| order.iter().position(|o| o == k).unwrap_or(usize::MAX));
        self.set(selected, focus, known);
    }

    /// Fokus auf eine sichtbare Kachel, die Auswahl bleibt.
    pub fn focus_key(&mut self, key: &str) {
        if self.selected.iter().any(|k| k == key) {
            self.focus = Some(key.to_string());
        }
    }
}

/// Die weiteren Fenster und das aktuelle. `current` ist eine Fensternummer, 0 das Hauptfenster.
#[derive(Debug, Default)]
pub struct Views {
    pub current: usize,
    views: BTreeMap<usize, View>,
}

impl Views {
    pub fn open(&mut self, index: usize, view: View) {
        if index != 0 {
            self.views.insert(index, view);
        }
    }

    /// Schließt ein Fenster; war es das aktuelle, gilt `fallback` (`windows.last`, ohne weitere das Hauptfenster).
    pub fn close(&mut self, index: usize, fallback: usize) {
        self.views.remove(&index);
        if self.current == index {
            self.current = fallback;
        }
    }

    pub fn get(&self, index: usize) -> Option<&View> {
        self.views.get(&index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut View> {
        self.views.get_mut(&index)
    }

    /// Das aktuelle weitere Fenster; `None`, wenn das Hauptfenster aktuell ist (oder das Fenster nicht mehr existiert).
    pub fn current_extra(&self) -> Option<usize> {
        (self.current != 0 && self.views.contains_key(&self.current)).then_some(self.current)
    }

    /// Die Nummer, auf die Befehle wirken: ein offenes weiteres Fenster, sonst das Hauptfenster.
    pub fn target(&self) -> usize {
        self.current_extra().unwrap_or(0)
    }
}

/// Gespeicherte Ansicht von Fenster `index` (Schlüssel mit Suffix); ohne brauchbare Auswahl die erste Session, die das
/// Hauptfenster nicht zeigt.
fn load_view(state: &AppState, index: usize) -> View {
    let s = &state.settings;
    let known = |k: &String| state.session(k).is_some();
    let mut selected: Vec<String> = s.strings_or(&key_for("workspace.selected", index)).into_iter().filter(known).collect();
    if selected.is_empty() {
        let order = state.ordered_keys();
        let pick = order.iter().find(|k| !state.selected.contains(k)).or(order.first());
        selected = pick.cloned().into_iter().collect();
    }
    let focus = s.opt_string(&key_for("workspace.selected.focus", index)).filter(|f| selected.contains(f)).or_else(|| selected.first().cloned());
    let mode = LayoutMode::from_raw(&s.string_or(&key_for("workspace.mode", index), "grid")).unwrap_or_default();
    View { selected, focus, mode, zoom: false }
}

impl AppState {
    // MARK: Fenster öffnen und schließen

    /// Legt die Ansicht von Fenster `index` an (aus den Einstellungen).
    pub fn open_view(&mut self, index: usize) {
        let view = load_view(self, index);
        self.wins.open(index, view);
    }

    /// Schließt die Ansicht; `fallback` wird das aktuelle Fenster, falls es dieses war.
    pub fn close_view(&mut self, index: usize, fallback: usize) {
        self.wins.close(index, fallback);
    }

    /// Fenster `index` ist jetzt vorn (`windowDidBecomeKey`).
    pub fn set_current_window(&mut self, index: usize) {
        self.wins.current = index;
    }

    // MARK: Lesen (aktuelles Fenster)

    /// Auswahl des aktuellen Fensters, soweit es die Sessions noch gibt.
    pub fn cur_selected(&self) -> Vec<String> {
        match self.wins.current_extra().and_then(|i| self.wins.get(i)) {
            Some(v) => v.selected.iter().filter(|k| self.session(k).is_some()).cloned().collect(),
            None => self.selected.clone(),
        }
    }

    /// Fokussierte Session des aktuellen Fensters.
    pub fn cur_focus(&self) -> Option<String> {
        match self.wins.current_extra().and_then(|i| self.wins.get(i)) {
            Some(v) => v.focus.clone().filter(|k| self.session(k).is_some()),
            None => self.focused.clone(),
        }
    }

    /// Zoom des aktuellen Fensters (jedes Fenster zoomt für sich).
    pub fn cur_zoom(&self) -> bool {
        match self.wins.current_extra().and_then(|i| self.wins.get(i)) {
            Some(v) => v.zoom,
            None => self.zoom,
        }
    }

    /// Layout des aktuellen Fensters.
    pub fn cur_layout(&self) -> LayoutMode {
        match self.wins.current_extra().and_then(|i| self.wins.get(i)) {
            Some(v) => v.mode,
            None => self.layout,
        }
    }

    // MARK: Schreiben (aktuelles Fenster)

    /// Zoom des aktuellen Fensters ein/aus (`kadrell zoom`, Menü, Kürzel).
    pub fn cur_toggle_zoom(&mut self) {
        match self.wins.current_extra() {
            Some(i) => self.view_toggle_zoom(i),
            None => self.zoom = !self.zoom,
        }
    }

    /// Zoom von Fenster `index` ein/aus; er wird nicht gespeichert.
    pub fn view_toggle_zoom(&mut self, index: usize) {
        if let Some(v) = self.wins.get_mut(index) {
            v.zoom = !v.zoom;
        }
    }

    /// Zoom von Fenster `index` aus.
    pub fn view_clear_zoom(&mut self, index: usize) {
        if let Some(v) = self.wins.get_mut(index) {
            v.zoom = false;
        }
    }

    /// Nur diese Session im aktuellen Fenster zeigen (`workspace.select([id])` des Fensters, das Fokus hat).
    pub fn cur_show_only(&mut self, key: &str) {
        match self.wins.current_extra() {
            Some(i) => self.view_select(i, &[key.to_string()], false),
            None => self.show_only(key),
        }
    }

    /// Auswahl im aktuellen Fenster setzen (`add` falsch) oder umschalten (`add` wahr).
    pub fn cur_select(&mut self, keys: &[String], add: bool) {
        match self.wins.current_extra() {
            Some(i) => self.view_select(i, keys, add),
            None => self.select_keys(keys, add),
        }
    }

    /// Auswahl und Fokus des aktuellen Fensters setzen (Menü „Alle Sessions auswählen“ und Rücknahme).
    pub fn cur_set_selection(&mut self, keys: Vec<String>, focus: Option<String>) {
        let Some(i) = self.wins.current_extra() else { return self.set_selection(keys, focus) };
        let mut view = self.wins.get(i).cloned().unwrap_or_default();
        view.set(keys, focus, &self.known_fn());
        self.store_view(i, view);
    }

    /// Fokus im aktuellen Fenster auf eine sichtbare Kachel.
    pub fn cur_focus_key(&mut self, key: &str) {
        match self.wins.current_extra() {
            Some(i) => self.view_focus(i, key),
            None => self.focus_key(key),
        }
    }

    /// Layout des aktuellen Fensters.
    pub fn cur_set_layout(&mut self, mode: LayoutMode) {
        match self.wins.current_extra() {
            Some(i) => self.view_set_mode(i, mode),
            None => {
                self.layout = mode;
                self.save_layout();
            }
        }
    }

    // MARK: Schreiben (bestimmtes weiteres Fenster)

    /// Beim Öffnen ohne brauchbare Auswahl (Sessions entstehen erst später): noch einmal wählen, speichern, starten.
    pub fn repick_view(&mut self, index: usize) {
        let view = load_view(self, index);
        if !view.selected.is_empty() {
            self.store_view(index, view);
        }
    }

    fn known_fn(&self) -> impl Fn(&str) -> bool + '_ {
        |k| self.session(k).is_some()
    }

    /// Auswahl von Fenster `index` ändern, speichern und fehlende Prozesse starten lassen.
    pub fn view_select(&mut self, index: usize, keys: &[String], add: bool) {
        let order = self.ordered_keys();
        let mut view = self.wins.get(index).cloned().unwrap_or_default();
        view.select(keys, add, &order, &self.known_fn());
        self.store_view(index, view);
    }

    /// Fokus in Fenster `index`; die Marke „neu“ ist damit quittiert (`onFocusChange` gilt in jedem Fenster).
    pub fn view_focus(&mut self, index: usize, key: &str) {
        let Some(mut view) = self.wins.get(index).cloned() else { return };
        view.focus_key(key);
        if view.focus.as_deref() == Some(key) {
            self.attention.mark_seen(key);
        }
        self.store_view(index, view);
    }

    pub fn view_set_mode(&mut self, index: usize, mode: LayoutMode) {
        let Some(mut view) = self.wins.get(index).cloned() else { return };
        view.mode = mode;
        self.store_view(index, view);
    }

    /// Neue Ansicht von Fenster `index` übernehmen: speichern (nur Geändertes), Sessions ohne Prozess starten.
    fn store_view(&mut self, index: usize, view: View) {
        if self.wins.get(index) == Some(&view) {
            return;
        }
        let selected = view.selected.clone();
        self.save_view_of(index, &view);
        self.wins.open(index, view);
        self.start_now(&selected);
    }

    fn save_view_of(&mut self, index: usize, view: &View) {
        self.save_workspace_setting(&key_for("workspace.selected", index), Some(json!(view.selected)));
        self.save_workspace_setting(&key_for("workspace.selected.focus", index), Some(json!(view.focus)));
        self.save_workspace_setting(&key_for("workspace.mode", index), Some(json!(view.mode.as_str())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    fn all(_: &str) -> bool {
        true
    }

    #[test]
    fn select_replaces_or_toggles_and_keeps_the_last() {
        let order = keys(&["a", "b", "c"]);
        let mut v = View::default();
        v.select(&keys(&["b"]), false, &order, &all);
        assert_eq!((v.selected.clone(), v.focus.clone()), (keys(&["b"]), Some("b".into())));
        v.select(&keys(&["a"]), true, &order, &all);
        assert_eq!(v.selected, keys(&["a", "b"]), "in Baumreihenfolge");
        assert_eq!(v.focus.as_deref(), Some("a"));
        v.select(&keys(&["a"]), true, &order, &all);
        assert_eq!(v.selected, keys(&["b"]));
        v.select(&keys(&["b"]), true, &order, &all);
        assert_eq!(v.selected, keys(&["b"]), "die letzte bleibt");
    }

    #[test]
    fn unknown_keys_fall_away_and_focus_follows_the_selection() {
        let mut v = View::default();
        v.set(keys(&["x", "a", "a"]), Some("x".into()), &|k| k == "a");
        assert_eq!(v.selected, keys(&["a"]));
        assert_eq!(v.focus.as_deref(), Some("a"));
        v.focus_key("zzz");
        assert_eq!(v.focus.as_deref(), Some("a"));
    }

    #[test]
    fn the_current_window_falls_back_when_it_closes() {
        let mut w = Views::default();
        w.open(1, View::default());
        w.open(2, View::default());
        w.current = 2;
        assert_eq!(w.target(), 2);
        w.close(2, 1);
        assert_eq!((w.current, w.target()), (1, 1));
        w.close(1, 0);
        assert_eq!((w.current, w.target()), (0, 0));
        w.open(0, View::default());
        assert!(w.get(0).is_none(), "das Hauptfenster hat keine View");
    }

    #[test]
    fn a_vanished_window_never_receives_commands() {
        let w = Views { current: 3, ..Views::default() };
        assert_eq!(w.target(), 0, "Fenster 3 gibt es nicht");
    }
}
