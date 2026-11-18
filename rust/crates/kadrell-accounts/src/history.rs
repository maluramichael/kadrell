//! Verlauf der Limits je Konto (Port von `UsageHistory.swift`), eine JSON-Zeile pro Messung im Profilordner
//! (`usage-history.jsonl`). Der Endpunkt liefert nur den aktuellen Stand, deshalb beginnt der Verlauf erst mit der
//! ersten Messung. Eine angeschnittene Zeile fällt beim Lesen raus.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::account::CachedUsage;

/// Ein gemessener Nutzungsstand eines Kontos. Kurze Schlüssel, weil davon über ein Jahr sechsstellig viele Zeilen
/// entstehen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageSample {
    pub t: f64,
    pub account: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fable: Option<i32>,
}

impl UsageSample {
    pub fn new(t: f64, account: &str) -> Self {
        Self {
            t,
            account: account.into(),
            session: None,
            weekly: None,
            fable: None,
        }
    }

    pub fn from_cached(account: &str, usage: &CachedUsage) -> Self {
        Self {
            t: usage.at,
            account: account.into(),
            session: usage.session,
            weekly: usage.weekly,
            fable: usage.fable,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.session.is_none() && self.weekly.is_none() && self.fable.is_none()
    }
}

pub struct UsageHistory {
    path: PathBuf,
    /// `None` bis zum ersten Lesen: läuft die Statistik nie, muss auch die Datei nie in den Speicher.
    loaded: Option<Vec<UsageSample>>,
}

impl UsageHistory {
    pub fn new(path: PathBuf) -> Self {
        Self { path, loaded: None }
    }

    /// Hängt eine Messung an. Leere Messungen (alle Werte fehlen) werden übersprungen.
    pub fn append(&mut self, sample: UsageSample) {
        if sample.is_empty() {
            return;
        }
        let Ok(mut line) = serde_json::to_string(&sample) else {
            return;
        };
        line.push('\n');
        if let Some(loaded) = &mut self.loaded {
            loaded.push(sample);
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let written = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut f| f.write_all(line.as_bytes()));
        if let Err(e) = written {
            log::warn!("usage-history: {e}");
        }
    }

    pub fn samples(&mut self) -> &[UsageSample] {
        if self.loaded.is_none() {
            let data = std::fs::read(&self.path).unwrap_or_default();
            self.loaded = Some(Self::parse(&data));
        }
        self.loaded.as_deref().unwrap_or_default()
    }

    /// Eine Zeile je Messung; kaputte (etwa angeschnittene) Zeilen werden übersprungen.
    pub fn parse(data: &[u8]) -> Vec<UsageSample> {
        data.split(|b| *b == b'\n')
            .filter_map(|line| serde_json::from_slice(line).ok())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kadrell-accounts-hist-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("usage-history.jsonl")
    }

    fn full(t: f64, account: &str, session: i32, weekly: i32, fable: i32) -> UsageSample {
        UsageSample {
            session: Some(session),
            weekly: Some(weekly),
            fable: Some(fable),
            ..UsageSample::new(t, account)
        }
    }

    fn weekly(t: f64, account: &str, weekly: i32) -> UsageSample {
        UsageSample {
            weekly: Some(weekly),
            ..UsageSample::new(t, account)
        }
    }

    #[test]
    fn append_survives_restart() {
        let path = temp_file("restart");
        let mut history = UsageHistory::new(path.clone());
        history.append(full(1.0, "a", 5, 6, 7));
        history.append(weekly(2.0, "b", 8));
        let loaded = UsageHistory::new(path.clone()).samples().to_vec();
        assert_eq!(loaded, vec![full(1.0, "a", 5, 6, 7), weekly(2.0, "b", 8)]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn skips_empty_measurement() {
        let path = temp_file("empty");
        let mut history = UsageHistory::new(path.clone());
        history.append(UsageSample::new(1.0, "a"));
        assert!(history.samples().is_empty());
        assert!(!path.exists(), "nichts geschrieben");
    }

    #[test]
    fn append_after_load_is_visible() {
        let path = temp_file("after");
        let mut history = UsageHistory::new(path.clone());
        assert!(history.samples().is_empty());
        history.append(weekly(3.0, "a", 1));
        assert_eq!(history.samples().len(), 1);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn parse_skips_broken_line() {
        let data = b"{\"t\":1,\"account\":\"a\",\"weekly\":2}\n{\"t\":2,\"acc";
        assert_eq!(UsageHistory::parse(data), vec![weekly(1.0, "a", 2)]);
    }

    #[test]
    fn missing_values_are_not_written() {
        let line = serde_json::to_string(&weekly(1.0, "a", 2)).unwrap();
        assert_eq!(line, r#"{"t":1.0,"account":"a","weekly":2}"#);
    }
}
