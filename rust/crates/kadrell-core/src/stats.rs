//! Lebenszeit-Zähler des Profils: geschickte Nachrichten, geöffnete Sessions und Terminals, Kachelwechsel und der
//! Rekord gleichzeitig offener Sessions. Quelle: `Services/Stats.swift`.
//!
//! Sechs Zahlen brauchen keine eigene Datei, sie liegen in den Einstellungen des Profils (`stats.*`). Ein
//! Wegwerfprofil (`--profile tmp`) nimmt seine Zahlen beim Beenden mit, statt sie einzurechnen. Zeiten sind
//! Sekunden seit 1970, der Aufrufer reicht `now` herein (siehe [`now_secs`]).

use crate::settings::Settings;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

/// Die vier einfachen Zähler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatKey {
    Messages,
    Sessions,
    Terminals,
    FocusSwitches,
}

impl StatKey {
    pub const ALL: [StatKey; 4] = [StatKey::Messages, StatKey::Sessions, StatKey::Terminals, StatKey::FocusSwitches];

    pub fn raw(self) -> &'static str {
        match self {
            StatKey::Messages => "messages",
            StatKey::Sessions => "sessions",
            StatKey::Terminals => "terminals",
            StatKey::FocusSwitches => "focusSwitches",
        }
    }

    /// Schlüssel in den Einstellungen, zum Beispiel `stats.messages`.
    pub fn settings_key(self) -> String {
        format!("stats.{}", self.raw())
    }
}

const RECORD_KEY: &str = "stats.record";
const RECORD_DATE_KEY: &str = "stats.recordDate";
const SINCE_KEY: &str = "stats.since";

/// Jetzt in Sekunden seit 1970.
pub fn now_secs() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Stand eines Zählers, 0 solange nichts gezählt wurde.
pub fn count(settings: &Settings, key: StatKey) -> i64 {
    settings.i64_or(&key.settings_key(), 0)
}

/// Zählt `by` dazu. Null oder weniger schreibt nichts, auch nicht den Zählbeginn.
pub fn bump(settings: &mut Settings, key: StatKey, by: i64, now: f64) {
    if by <= 0 {
        return;
    }
    mark_start(settings, now);
    let total = count(settings, key) + by;
    settings.set(&key.settings_key(), Value::from(total));
}

/// Höchststand gleichzeitig offener Sessions, mit dem Tag, an dem er aufgestellt wurde. Ein niedrigerer oder
/// gleicher Stand ändert nichts, auch nicht das Datum.
pub fn note_concurrent(settings: &mut Settings, n: i64, now: f64) {
    mark_start(settings, now);
    if n <= settings.i64_or(RECORD_KEY, 0) {
        return;
    }
    settings.set(RECORD_KEY, Value::from(n));
    settings.set_f64(RECORD_DATE_KEY, now);
}

/// Rekord und der Zeitpunkt, an dem er aufgestellt wurde.
pub fn record(settings: &Settings) -> (i64, Option<f64>) {
    (settings.i64_or(RECORD_KEY, 0), date(settings, RECORD_DATE_KEY))
}

/// Zählbeginn, gesetzt beim ersten gezählten Ereignis. `None`, solange nichts gezählt wurde.
pub fn since(settings: &Settings) -> Option<f64> {
    date(settings, SINCE_KEY)
}

fn date(settings: &Settings, key: &str) -> Option<f64> {
    Some(settings.f64_or(key, 0.0)).filter(|t| *t > 0.0)
}

fn mark_start(settings: &mut Settings, now: f64) {
    if settings.f64_or(SINCE_KEY, 0.0) == 0.0 {
        settings.set_f64(SINCE_KEY, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: f64 = 1_789_479_734.0;

    #[test]
    fn count_starts_at_zero() {
        assert_eq!(count(&Settings::default(), StatKey::Messages), 0);
    }

    #[test]
    fn bump_counts_up() {
        let mut s = Settings::default();
        bump(&mut s, StatKey::Sessions, 1, T0);
        bump(&mut s, StatKey::Sessions, 1, T0);
        assert_eq!(count(&s, StatKey::Sessions), 2);
    }

    #[test]
    fn bump_by_more_than_one() {
        let mut s = Settings::default();
        bump(&mut s, StatKey::Messages, 5, T0);
        assert_eq!(count(&s, StatKey::Messages), 5);
    }

    #[test]
    fn bump_by_zero_writes_nothing() {
        let mut s = Settings::default();
        bump(&mut s, StatKey::Messages, 0, T0);
        bump(&mut s, StatKey::Messages, -3, T0);
        assert!(s.get("stats.messages").is_none());
        assert_eq!(since(&s), None);
    }

    #[test]
    fn counters_are_independent() {
        let mut s = Settings::default();
        bump(&mut s, StatKey::Terminals, 3, T0);
        assert_eq!(count(&s, StatKey::Terminals), 3);
        assert_eq!(count(&s, StatKey::FocusSwitches), 0);
    }

    #[test]
    fn record_keeps_highest_value() {
        let mut s = Settings::default();
        note_concurrent(&mut s, 7, T0);
        note_concurrent(&mut s, 4, T0 + 10.0);
        assert_eq!(record(&s).0, 7);
    }

    #[test]
    fn record_stores_date_of_new_high() {
        let mut s = Settings::default();
        assert_eq!(record(&s).1, None);
        note_concurrent(&mut s, 3, T0);
        assert_eq!(record(&s).1, Some(T0));
        note_concurrent(&mut s, 9, T0 + 60.0);
        assert_eq!(record(&s), (9, Some(T0 + 60.0)));
    }

    /// Ein niedrigerer Stand darf das Rekorddatum nicht überschreiben.
    #[test]
    fn record_date_survives_lower_value() {
        let mut s = Settings::default();
        note_concurrent(&mut s, 5, T0);
        let date = record(&s).1;
        note_concurrent(&mut s, 1, T0 + 99.0);
        note_concurrent(&mut s, 5, T0 + 99.0);
        assert_eq!(record(&s).1, date);
    }

    #[test]
    fn since_is_set_on_first_bump() {
        let mut s = Settings::default();
        assert_eq!(since(&s), None);
        bump(&mut s, StatKey::Sessions, 1, T0);
        assert_eq!(since(&s), Some(T0));
    }

    /// Der Zählbeginn bleibt der erste, nicht der jüngste.
    #[test]
    fn since_does_not_move() {
        let mut s = Settings::default();
        bump(&mut s, StatKey::Sessions, 1, T0);
        bump(&mut s, StatKey::Messages, 1, T0 + 100.0);
        note_concurrent(&mut s, 4, T0 + 200.0);
        assert_eq!(since(&s), Some(T0));
    }

    #[test]
    fn keys_match_the_mac_settings_keys() {
        let keys: Vec<String> = StatKey::ALL.iter().map(|k| k.settings_key()).collect();
        assert_eq!(keys, ["stats.messages", "stats.sessions", "stats.terminals", "stats.focusSwitches"]);
    }
}
