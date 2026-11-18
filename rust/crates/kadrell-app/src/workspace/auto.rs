//! Auto-Modus (AUTO): aus der Auswahl oder allen Sessions nur die zeigen, die gerade etwas von dir wollen (wartend)
//! oder arbeiten. Wer nicht mehr passt, bleibt 3 s stehen (Nachlauf), damit nichts unter den Fingern wegspringt.
//! Verschwindet die Fokus-Kachel, geht der Fokus an die neu aufgetauchte oder die erste. Port von `tiles`,
//! `updateLinger` und `autoRefocus` aus `WorkspaceView.swift`.

use std::collections::{HashMap, HashSet};

use kadrell_core::Settings;

/// So lange bleibt eine Kachel nach dem letzten Treffer stehen.
pub const LINGER_SECS: f64 = 3.;

/// Welche Zustände gezeigt werden und woraus gefiltert wird (Einstellungen `auto.*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    pub waiting: bool,
    pub running: bool,
    pub all_sessions: bool,
}

impl Filter {
    pub fn from_settings(s: &Settings) -> Filter {
        Filter { waiting: s.bool_or("auto.waiting", true), running: s.bool_or("auto.running", false), all_sessions: s.bool_or("auto.allSessions", true) }
    }
}

#[derive(Default, Debug)]
pub struct Auto {
    pub on: bool,
    /// Die Kacheln des letzten Abgleichs, in Reihenfolge des Pools.
    pub shown: Vec<String>,
    /// Schlüssel und Zeitpunkt, bis zu dem die Kachel nachläuft.
    linger: HashMap<String, f64>,
    last_matching: HashSet<String>,
    last_tiles: Vec<String>,
}

impl Auto {
    pub fn new(on: bool) -> Auto {
        Auto { on, ..Auto::default() }
    }

    /// Ein- oder ausschalten: der Nachlauf fängt von vorn an.
    pub fn set(&mut self, on: bool) {
        self.on = on;
        self.linger.clear();
        self.last_matching.clear();
        self.last_tiles.clear();
        self.shown.clear();
    }

    /// Läuft noch ein Nachlauf ab? Dann muss ein Takt weiter prüfen.
    pub fn lingering(&self) -> bool {
        self.on && !self.linger.is_empty()
    }

    /// Wer eben noch passte und jetzt nicht mehr, läuft 3 s nach.
    fn update_linger(&mut self, pool: &[String], matching: &HashSet<String>, now: f64) {
        for id in self.last_matching.difference(matching) {
            self.linger.entry(id.clone()).or_insert(now + LINGER_SECS);
        }
        self.linger.retain(|k, until| *until > now && !matching.contains(k) && pool.contains(k));
        self.last_matching = matching.clone();
    }

    /// Gleicht ab: berechnet die Kacheln aus `pool` (Baumreihenfolge oder Auswahl) und dem Treffertest. Gibt
    /// `true` zurück, wenn sich die Kacheln geändert haben.
    pub fn update(&mut self, pool: &[String], matches: impl Fn(&str) -> bool, now: f64) -> bool {
        if !self.on {
            return false;
        }
        let matching: HashSet<String> = pool.iter().filter(|k| matches(k)).cloned().collect();
        self.update_linger(pool, &matching, now);
        let tiles: Vec<String> = pool.iter().filter(|k| matching.contains(*k) || self.linger.contains_key(*k)).cloned().collect();
        let changed = tiles != self.shown;
        self.shown = tiles;
        changed
    }

    /// Ist die Fokus-Kachel weg, bekommt eine neu aufgetauchte (sonst die erste) den Fokus. `None`: bleibt. Merkt sich
    /// die Kacheln für das nächste Mal, auch wenn nichts umfokussiert wird. Nicht in der Vorschau.
    pub fn refocus(&mut self, focus: Option<&str>, previewing: bool) -> Option<String> {
        let tiles = self.shown.clone();
        let last = std::mem::replace(&mut self.last_tiles, tiles.clone());
        let first = tiles.first()?;
        if !self.on || previewing || focus.is_some_and(|f| tiles.iter().any(|t| t == f)) {
            return None;
        }
        Some(tiles.iter().find(|t| !last.contains(t)).unwrap_or(first).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> Vec<String> {
        ["a", "b", "c"].iter().map(|s| s.to_string()).collect()
    }

    fn matching(keys: &'static [&'static str]) -> impl Fn(&str) -> bool {
        move |k| keys.contains(&k)
    }

    #[test]
    fn only_matching_sessions_are_tiles_in_pool_order() {
        let mut a = Auto::new(true);
        assert!(a.update(&pool(), matching(&["c", "a"]), 0.));
        assert_eq!(a.shown, ["a", "c"]);
        assert!(!a.update(&pool(), matching(&["c", "a"]), 1.), "nichts geändert");
    }

    #[test]
    fn off_shows_nothing_and_does_not_track() {
        let mut a = Auto::new(false);
        assert!(!a.update(&pool(), matching(&["a"]), 0.));
        assert!(a.shown.is_empty());
        assert_eq!(a.refocus(Some("x"), false), None);
    }

    #[test]
    fn a_session_that_stops_matching_lingers_for_three_seconds() {
        let mut a = Auto::new(true);
        a.update(&pool(), matching(&["a", "b"]), 10.);
        assert!(!a.lingering());
        assert!(!a.update(&pool(), matching(&["b"]), 11.), "a bleibt");
        assert_eq!(a.shown, ["a", "b"]);
        assert!(a.lingering());
        assert!(!a.update(&pool(), matching(&["b"]), 13.9));
        assert!(a.update(&pool(), matching(&["b"]), 14.1), "nach 3 s ist die Kachel weg");
        assert_eq!(a.shown, ["b"]);
        assert!(!a.lingering());
    }

    #[test]
    fn a_session_that_matches_again_stops_lingering() {
        let mut a = Auto::new(true);
        a.update(&pool(), matching(&["a"]), 0.);
        a.update(&pool(), matching(&[]), 1.);
        assert!(a.lingering());
        a.update(&pool(), matching(&["a"]), 2.);
        assert!(!a.lingering());
        assert_eq!(a.shown, ["a"]);
    }

    #[test]
    fn a_removed_session_does_not_linger() {
        let mut a = Auto::new(true);
        a.update(&pool(), matching(&["a", "b"]), 0.);
        let without_a: Vec<String> = vec!["b".into(), "c".into()];
        a.update(&without_a, matching(&["b"]), 1.);
        assert_eq!(a.shown, ["b"]);
        assert!(!a.lingering());
    }

    #[test]
    fn refocus_prefers_the_newly_appeared_tile_else_the_first() {
        let mut a = Auto::new(true);
        a.update(&pool(), matching(&["a"]), 0.);
        assert_eq!(a.refocus(None, false).as_deref(), Some("a"), "ohne Fokus: die erste");
        assert_eq!(a.refocus(Some("a"), false), None, "Fokus liegt auf einer Kachel");
        a.update(&pool(), matching(&["a", "c"]), 1.);
        assert_eq!(a.refocus(Some("zz"), false).as_deref(), Some("c"), "die neue Kachel");
        assert_eq!(a.refocus(Some("c"), false), None);
    }

    #[test]
    fn refocus_waits_during_a_preview_but_still_remembers_the_tiles() {
        let mut a = Auto::new(true);
        a.update(&pool(), matching(&["a"]), 0.);
        assert_eq!(a.refocus(Some("zz"), true), None);
        a.update(&pool(), matching(&["a", "b"]), 1.);
        assert_eq!(a.refocus(Some("zz"), false).as_deref(), Some("b"), "a war schon da, b ist neu");
    }

    #[test]
    fn no_tiles_means_nothing_to_focus() {
        let mut a = Auto::new(true);
        a.update(&pool(), matching(&[]), 0.);
        assert_eq!(a.refocus(Some("a"), false), None);
    }

    #[test]
    fn switching_resets_the_state() {
        let mut a = Auto::new(true);
        a.update(&pool(), matching(&["a"]), 0.);
        a.update(&pool(), matching(&[]), 1.);
        a.set(true);
        assert!(!a.lingering());
        assert!(a.shown.is_empty());
    }

    #[test]
    fn the_filter_reads_the_auto_settings_with_their_defaults() {
        let s = Settings::default();
        assert_eq!(Filter::from_settings(&s), Filter { waiting: true, running: false, all_sessions: true });
        let mut s = Settings::default();
        s.set("auto.running", serde_json::json!(true));
        s.set("auto.waiting", serde_json::json!(false));
        s.set("auto.allSessions", serde_json::json!(false));
        assert_eq!(Filter::from_settings(&s), Filter { waiting: false, running: true, all_sessions: false });
    }
}
