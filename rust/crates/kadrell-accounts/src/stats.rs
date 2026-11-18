//! HINWEIS: Die App benutzt `kadrell_core::stats` (arbeitet direkt auf `Settings`); dieser Port bleibt nur als
//! Referenz, damit nichts doppelt zählt.
//!
//! Lebenszeit-Zähler des Profils (Port von `Stats.swift`): geschickte Nachrichten, geöffnete Sessions und Terminals,
//! Kachelwechsel und der Rekord gleichzeitig offener Sessions. Sechs Zahlen brauchen keine eigene Datei, sie liegen
//! in den Einstellungen des Profils (`stats.*`, hier hinter [`KeyValueStore`]); ein Wegwerfprofil (`--profile tmp`)
//! nimmt seine Zahlen beim Beenden mit, statt sie einzurechnen.

use std::collections::HashMap;

/// Schlüssel-Wert-Speicher der Profil-Einstellungen (Zahlen). In `kadrell-core` an die Settings anschließen.
pub trait KeyValueStore: Send {
    fn get(&self, key: &str) -> Option<f64>;
    fn set(&mut self, key: &str, value: f64);
}

/// Speicher im Arbeitsspeicher für Tests.
#[derive(Debug, Default)]
pub struct MemoryKeyValues(pub HashMap<String, f64>);

impl KeyValueStore for MemoryKeyValues {
    fn get(&self, key: &str) -> Option<f64> {
        self.0.get(key).copied()
    }

    fn set(&mut self, key: &str, value: f64) {
        self.0.insert(key.into(), value);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsKey {
    Messages,
    Sessions,
    Terminals,
    FocusSwitches,
}

impl StatsKey {
    pub const ALL: [StatsKey; 4] = [
        StatsKey::Messages,
        StatsKey::Sessions,
        StatsKey::Terminals,
        StatsKey::FocusSwitches,
    ];

    pub fn defaults_key(self) -> &'static str {
        match self {
            StatsKey::Messages => "stats.messages",
            StatsKey::Sessions => "stats.sessions",
            StatsKey::Terminals => "stats.terminals",
            StatsKey::FocusSwitches => "stats.focusSwitches",
        }
    }
}

const RECORD_KEY: &str = "stats.record";
const RECORD_DATE_KEY: &str = "stats.recordDate";
const SINCE_KEY: &str = "stats.since";

pub struct Stats<S: KeyValueStore> {
    store: S,
}

impl<S: KeyValueStore> Stats<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    fn int(&self, key: &str) -> i64 {
        self.store.get(key).unwrap_or(0.0) as i64
    }

    fn date(&self, key: &str) -> Option<f64> {
        self.store.get(key).filter(|t| *t > 0.0)
    }

    pub fn count(&self, key: StatsKey) -> i64 {
        self.int(key.defaults_key())
    }

    pub fn bump(&mut self, key: StatsKey, by: i64, now: f64) {
        if by <= 0 {
            return;
        }
        self.mark_start(now);
        let next = self.count(key) + by;
        self.store.set(key.defaults_key(), next as f64);
    }

    /// Höchststand gleichzeitig offener Sessions, mit dem Tag, an dem er aufgestellt wurde.
    /// Ein niedrigerer Stand ändert nichts, auch nicht das Datum.
    pub fn note_concurrent(&mut self, n: i64, now: f64) {
        self.mark_start(now);
        if n > self.int(RECORD_KEY) {
            self.store.set(RECORD_KEY, n as f64);
            self.store.set(RECORD_DATE_KEY, now);
        }
    }

    pub fn record(&self) -> (i64, Option<f64>) {
        (self.int(RECORD_KEY), self.date(RECORD_DATE_KEY))
    }

    /// Zählbeginn, gesetzt beim ersten gezählten Ereignis. `None`, solange nichts gezählt wurde.
    pub fn since(&self) -> Option<f64> {
        self.date(SINCE_KEY)
    }

    fn mark_start(&mut self, now: f64) {
        if self.store.get(SINCE_KEY).unwrap_or(0.0) == 0.0 {
            self.store.set(SINCE_KEY, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> Stats<MemoryKeyValues> {
        Stats::new(MemoryKeyValues::default())
    }

    #[test]
    fn count_starts_at_zero() {
        assert_eq!(stats().count(StatsKey::Messages), 0);
    }

    #[test]
    fn bump_counts_up() {
        let mut s = stats();
        s.bump(StatsKey::Sessions, 1, 10.0);
        s.bump(StatsKey::Sessions, 1, 11.0);
        assert_eq!(s.count(StatsKey::Sessions), 2);
    }

    #[test]
    fn bump_by_more_than_one() {
        let mut s = stats();
        s.bump(StatsKey::Messages, 5, 10.0);
        assert_eq!(s.count(StatsKey::Messages), 5);
    }

    #[test]
    fn bump_by_zero_writes_nothing() {
        let mut s = stats();
        s.bump(StatsKey::Messages, 0, 10.0);
        assert_eq!(s.store().get("stats.messages"), None);
        assert_eq!(s.since(), None);
    }

    #[test]
    fn counters_are_independent() {
        let mut s = stats();
        s.bump(StatsKey::Terminals, 3, 10.0);
        assert_eq!(s.count(StatsKey::Terminals), 3);
        assert_eq!(s.count(StatsKey::FocusSwitches), 0);
    }

    #[test]
    fn record_keeps_highest_value() {
        let mut s = stats();
        s.note_concurrent(7, 10.0);
        s.note_concurrent(4, 11.0);
        assert_eq!(s.record().0, 7);
    }

    #[test]
    fn record_stores_date_of_new_high() {
        let mut s = stats();
        assert_eq!(s.record().1, None);
        s.note_concurrent(3, 10.0);
        assert_eq!(s.record().1, Some(10.0));
        s.note_concurrent(9, 20.0);
        assert_eq!(s.record(), (9, Some(20.0)));
    }

    /// Ein niedrigerer Stand darf das Rekorddatum nicht überschreiben.
    #[test]
    fn record_date_survives_lower_value() {
        let mut s = stats();
        s.note_concurrent(5, 10.0);
        let date = s.record().1;
        s.note_concurrent(1, 20.0);
        assert_eq!(s.record().1, date);
    }

    #[test]
    fn since_is_set_on_first_bump() {
        let mut s = stats();
        assert_eq!(s.since(), None);
        s.bump(StatsKey::Sessions, 1, 10.0);
        assert_eq!(s.since(), Some(10.0));
    }

    /// Der Zählbeginn bleibt der erste, nicht der jüngste.
    #[test]
    fn since_does_not_move() {
        let mut s = stats();
        s.bump(StatsKey::Sessions, 1, 10.0);
        let first = s.since();
        s.bump(StatsKey::Messages, 1, 20.0);
        s.note_concurrent(4, 30.0);
        assert_eq!(s.since(), first);
    }
}
