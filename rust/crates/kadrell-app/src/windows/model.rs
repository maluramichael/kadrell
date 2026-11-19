//! Modell der weiteren Fenster (⌘⇧T / Strg+Umschalt+T, `AppDelegate.menuNewWindow` und `MultiWindowTests`): welche Fenster
//! offen sind (pro Profil gespeichert, kommen nach dem Neustart wieder), unter welchen Schlüsseln jedes Fenster seine
//! Auswahl ablegt, und in welchem Fenster ein Terminal hängt. Ein Terminal hängt nur in einem Fenster; die Kachel im
//! anderen zeigt „IN ANDEREM FENSTER“ und holt es per Klick oder Fokus. Alles ohne GPUI testbar.

use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

/// Einstellung mit den Nummern der offenen Fenster (aufsteigend, Fenster 0 ist das Hauptfenster).
pub const OPEN_KEY: &str = "windows.open";

/// Schlüssel, die jedes Fenster für sich führt und beim Schließen verliert (`closeWindow`: Auswahl, Layout, Auto-Modus und
/// die Breiten der Seitenleisten; dazu der Fokus und die Höhe des Docks der Rust-App).
pub const PER_WINDOW_KEYS: [&str; 8] = [
    "workspace.selected",
    "workspace.selected.focus",
    "workspace.mode",
    "workspace.auto",
    "sidebar.width",
    "rightSidebar.width",
    "rightSidebar.visible",
    "leftDock.height",
];

/// Suffix der Schlüssel eines Fensters: Fenster 0 behält die bisherigen Schlüssel, weitere hängen `.<n+1>` an
/// (`MainWindowController.suffix`).
pub fn suffix(index: usize) -> String {
    if index == 0 { String::new() } else { format!(".{}", index + 1) }
}

/// Schlüssel `base` für Fenster `index`.
pub fn key_for(base: &str, index: usize) -> String {
    format!("{base}{}", suffix(index))
}

/// Alle Schlüssel eines Fensters, die beim Schließen gelöscht werden.
pub fn keys_of(index: usize) -> Vec<String> {
    PER_WINDOW_KEYS.iter().map(|k| key_for(k, index)).collect()
}

/// Die offenen Fenster, aufsteigend sortiert und ohne Doppelte; Fenster 0 gibt es immer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSet {
    open: BTreeSet<usize>,
}

impl Default for WindowSet {
    fn default() -> Self {
        WindowSet { open: BTreeSet::from([0]) }
    }
}

impl WindowSet {
    /// Aus dem gespeicherten Wert (`windows.open`); Unbrauchbares und Fehlendes ergibt nur das Hauptfenster.
    pub fn restore(value: Option<&Value>) -> WindowSet {
        let mut open: BTreeSet<usize> = value
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_u64).filter_map(|n| usize::try_from(n).ok()).collect())
            .unwrap_or_default();
        open.insert(0);
        WindowSet { open }
    }

    /// Zum Speichern: aufsteigende Nummern.
    pub fn to_value(&self) -> Value {
        Value::from(self.open.iter().copied().collect::<Vec<_>>())
    }

    /// Nummern der weiteren Fenster, die beim Start wieder aufgehen.
    pub fn extras(&self) -> Vec<usize> {
        self.open.iter().copied().filter(|i| *i != 0).collect()
    }

    /// Kleinste freie Nummer ab 1 (`menuNewWindow`).
    pub fn next_free(&self) -> usize {
        (1..).find(|i| !self.open.contains(i)).unwrap_or(1)
    }

    pub fn open(&mut self, index: usize) {
        self.open.insert(index);
    }

    /// Schließt ein Fenster; das Hauptfenster bleibt immer.
    pub fn close(&mut self, index: usize) {
        if index != 0 {
            self.open.remove(&index);
        }
    }

    pub fn contains(&self, index: usize) -> bool {
        self.open.contains(&index)
    }
}

/// In welchem Fenster welches Terminal hängt (Schlüssel der Session auf Nummer des Fensters).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PaneOwners {
    map: HashMap<String, usize>,
}

impl PaneOwners {
    /// Gleicht mit dem ab, was die Fenster gerade zeigen (`shown`: Fenster auf Schlüssel). Wer sein Terminal nicht mehr
    /// zeigt, gibt es frei (`onReleaseTerminal`), ein freies hängt im ersten Fenster, das es zeigt. Ein belegtes bleibt,
    /// solange sein Fenster es zeigt, sonst wüchse bei jedem Zeichnen das Terminal von Fenster zu Fenster.
    pub fn reconcile(&mut self, shown: &HashMap<usize, Vec<String>>) {
        self.map.retain(|key, window| shown.get(window).is_some_and(|keys| keys.contains(key)));
        let mut windows: Vec<usize> = shown.keys().copied().collect();
        windows.sort_unstable();
        for window in windows {
            for key in &shown[&window] {
                self.map.entry(key.clone()).or_insert(window);
            }
        }
    }

    /// Das Fenster holt sich das Terminal (Klick oder Fokus auf die Kachel).
    pub fn take(&mut self, key: &str, window: usize) {
        self.map.insert(key.to_string(), window);
    }

    pub fn owner(&self, key: &str) -> Option<usize> {
        self.map.get(key).copied()
    }

    /// Hängt das Terminal in einem anderen Fenster? Dann zeigt die Kachel hier den Hinweis.
    pub fn elsewhere(&self, window: usize, key: &str) -> bool {
        self.owner(key).is_some_and(|o| o != window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shown(a: &[&str], b: &[&str]) -> HashMap<usize, Vec<String>> {
        HashMap::from([(0, a.iter().map(|s| s.to_string()).collect()), (1, b.iter().map(|s| s.to_string()).collect())])
    }

    /// `testTerminalHangsInOneWindow`: erstes Fenster bekommt es, das zweite zeigt den Hinweis, Fokus holt es herüber,
    /// Abwählen im Besitzer gibt es dem anderen zurück.
    #[test]
    fn terminal_hangs_in_one_window() {
        let mut o = PaneOwners::default();
        o.reconcile(&shown(&["s"], &["s"]));
        assert_eq!(o.owner("s"), Some(0));
        assert!(!o.elsewhere(0, "s"));
        assert!(o.elsewhere(1, "s"));

        o.take("s", 1);
        o.reconcile(&shown(&["s"], &["s"]));
        assert!(o.elsewhere(0, "s"), "Fenster 0 zeigt jetzt den Hinweis");
        assert!(!o.elsewhere(1, "s"));

        o.reconcile(&shown(&["s"], &[]));
        assert_eq!(o.owner("s"), Some(0), "Fenster 1 hat es abgewählt, Fenster 0 bekommt es zurück");
        assert!(!o.elsewhere(0, "s"));
    }

    #[test]
    fn a_held_terminal_stays_while_its_window_shows_it() {
        let mut o = PaneOwners::default();
        for _ in 0..3 {
            o.reconcile(&shown(&["a", "b"], &["b"]));
        }
        assert_eq!((o.owner("a"), o.owner("b")), (Some(0), Some(0)));
        o.reconcile(&shown(&["a"], &["b"]));
        assert_eq!(o.owner("b"), Some(1), "Fenster 0 zeigt b nicht mehr");
    }

    #[test]
    fn closed_windows_release_everything() {
        let mut o = PaneOwners::default();
        o.reconcile(&shown(&[], &["x"]));
        assert_eq!(o.owner("x"), Some(1));
        o.reconcile(&HashMap::from([(0, vec!["x".to_string()])]));
        assert_eq!(o.owner("x"), Some(0));
    }

    #[test]
    fn suffixes_follow_the_mac_app() {
        assert_eq!(suffix(0), "");
        assert_eq!(suffix(1), ".2");
        assert_eq!(key_for("workspace.selected", 0), "workspace.selected");
        assert_eq!(key_for("workspace.selected", 2), "workspace.selected.3");
        assert!(keys_of(1).contains(&"workspace.mode.2".to_string()));
        assert!(keys_of(0).contains(&"workspace.mode".to_string()));
        // `closeWindow` der Mac-App löscht auch die Breiten der Seitenleisten; das Dock der Rust-App kommt dazu.
        for k in ["sidebar.width.2", "rightSidebar.width.2", "rightSidebar.visible.2", "leftDock.height.2", "workspace.auto.2"] {
            assert!(keys_of(1).contains(&k.to_string()), "{k}");
        }
    }

    /// Fenster kommen nach dem Neustart wieder: speichern, lesen, kleinste freie Nummer.
    #[test]
    fn open_windows_persist_and_come_back() {
        let mut set = WindowSet::default();
        assert_eq!(set.extras(), Vec::<usize>::new());
        assert_eq!(set.next_free(), 1);
        set.open(1);
        set.open(2);
        assert_eq!(set.next_free(), 3);
        set.close(1);
        assert_eq!(set.next_free(), 1, "kleinste freie Nummer");
        set.open(1);
        let saved = set.to_value();
        assert_eq!(saved, json!([0, 1, 2]));
        let back = WindowSet::restore(Some(&saved));
        assert_eq!(back, set);
        assert_eq!(back.extras(), vec![1, 2]);
    }

    #[test]
    fn restore_tolerates_garbage_and_keeps_the_main_window() {
        assert_eq!(WindowSet::restore(None), WindowSet::default());
        assert_eq!(WindowSet::restore(Some(&json!("x"))), WindowSet::default());
        assert_eq!(WindowSet::restore(Some(&json!([2, "a", -1, 2]))).extras(), vec![2]);
        let mut set = WindowSet::default();
        set.close(0);
        assert!(set.contains(0), "das Hauptfenster bleibt");
    }
}
