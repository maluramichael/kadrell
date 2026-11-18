//! Laden und Speichern der JSON-Dateien (`sessions.json`, `groups.json`, `closed.json`), gleiches Verhalten wie
//! `Services/JSONFile.swift`: tolerant gegen kaputte Dateien, kaputte Originale wandern nach
//! `<name>.corrupt-<datum>.json`, geschrieben wird atomar über eine Temp-Datei.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Ergebnis von [`load_array`].
#[derive(Debug, Clone, PartialEq)]
pub struct LoadResult<T> {
    pub items: Vec<T>,
    /// Datei war kaputt (ganz oder teilweise) und wurde vor dem Laden gesichert.
    pub corrupted: bool,
    /// Der Umschlag nennt eine höhere Version, als diese Kadrell-Version kennt (Downgrade): nicht mehr speichern,
    /// sonst geht verloren, was die neuere Version geschrieben hat.
    pub newer_than_known: bool,
    /// Meldung für den Nutzer (kaputte Datei), sonst `None`.
    pub message: Option<String>,
}

impl<T> LoadResult<T> {
    fn items(items: Vec<T>) -> LoadResult<T> {
        LoadResult { items, corrupted: false, newer_than_known: false, message: None }
    }
}

/// `{"version": 1, "items": [...]}`
#[derive(serde::Deserialize)]
struct Envelope<T> {
    version: i64,
    items: Vec<T>,
}

/// Lädt ein Array `[T]`, tolerant gegen kaputte Dateien und einzelne kaputte Einträge:
/// - Datei fehlt: leerer Start, kein Fehler.
/// - Ganze Datei unlesbar oder nur einzelne Einträge kaputt: Original nach `<name>.corrupt-<datum>.json` gesichert,
///   brauchbare Einträge bleiben erhalten.
/// - Ein nacktes Array (Format vor der Versionierung) gilt als Version 0.
pub fn load_array<T: DeserializeOwned>(path: &Path, current_version: i64) -> LoadResult<T> {
    let Ok(data) = std::fs::read(path) else { return LoadResult::items(Vec::new()) };
    if let Ok(full) = serde_json::from_slice::<Envelope<T>>(&data) {
        return LoadResult { newer_than_known: full.version > current_version, ..LoadResult::items(full.items) };
    }
    if let Ok(items) = serde_json::from_slice::<Vec<T>>(&data) {
        return LoadResult::items(items);
    }
    recover(path, &data)
}

/// Rettet aus einer kaputten Datei, was sich lesen lässt, und sichert das Original.
fn recover<T: DeserializeOwned>(path: &Path, data: &[u8]) -> LoadResult<T> {
    let raw = serde_json::from_slice::<Value>(data).ok().and_then(raw_entries).unwrap_or_default();
    let recovered: Vec<T> = raw.iter().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect();
    quarantine(path);
    let name = file_name(path);
    let message = if raw.is_empty() {
        format!("{name} ist kaputt, Original gesichert, Datei beginnt leer")
    } else {
        format!("{name}: {} von {} Einträgen kaputt, Rest übernommen, Original gesichert", raw.len() - recovered.len(), raw.len())
    };
    LoadResult { items: recovered, corrupted: true, newer_than_known: false, message: Some(message) }
}

/// Die Einträge eines Arrays oder eines Umschlags `{"items": [...]}`.
fn raw_entries(value: Value) -> Option<Vec<Value>> {
    match value {
        Value::Array(a) => Some(a),
        Value::Object(mut o) => match o.remove("items") {
            Some(Value::Array(a)) => Some(a),
            _ => None,
        },
        _ => None,
    }
}

/// Schreibt `items`. Bis Version 1 als nacktes Array: ältere Kadrell-Versionen lesen einen Umschlag als kaputt und
/// würden die Datei sonst leer überschreiben. Erst ein echter Formatwechsel (Version ab 2) schreibt den Umschlag.
/// Der Fehler ist die Meldung für den Nutzer.
pub fn save_array<T: Serialize>(items: &[T], path: &Path, version: i64) -> Result<(), String> {
    let encoded = if version >= 2 {
        serde_json::to_vec_pretty(&serde_json::json!({ "version": version, "items": items }))
    } else {
        serde_json::to_vec_pretty(items)
    };
    write(path, encoded)
}

/// Wie [`load_array`], aber für ein `{String: V}` (zum Beispiel `folders-uses.json`), ohne Versionierung. Die
/// zweite Rückgabe ist die Meldung, wenn die Datei kaputt war.
pub fn load_dict<V: DeserializeOwned>(path: &Path) -> (BTreeMap<String, V>, Option<String>) {
    let Ok(data) = std::fs::read(path) else { return (BTreeMap::new(), None) };
    if let Ok(d) = serde_json::from_slice(&data) {
        return (d, None);
    }
    quarantine(path);
    (BTreeMap::new(), Some(format!("{} ist kaputt, Original gesichert, Datei beginnt leer", file_name(path))))
}

/// Gegenstück zu [`load_dict`].
pub fn save_dict<V: Serialize>(dict: &BTreeMap<String, V>, path: &Path) -> Result<(), String> {
    write(path, serde_json::to_vec(dict))
}

/// Schreibt Bytes atomar: erst in `<datei>.tmp`, dann darüber umbenennen, damit ein Absturz mitten im Schreiben
/// keine halbe Datei hinterlässt. Legt den Ordner an.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn write(path: &Path, encoded: serde_json::Result<Vec<u8>>) -> Result<(), String> {
    let failed = |e: String| format!("Speichern von {} fehlgeschlagen: {e}", file_name(path));
    let data = encoded.map_err(|e| failed(e.to_string()))?;
    write_atomic(path, &data).map_err(|e| failed(e.to_string()))
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Verschiebt eine kaputte Datei aus dem Weg, bevor je wieder hineingeschrieben wird.
fn quarantine(path: &Path) {
    if !path.exists() {
        return;
    }
    let base = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let dest = path.with_file_name(format!("{base}.corrupt-{}.json", corrupt_stamp(SystemTime::now())));
    let _ = std::fs::remove_file(&dest);
    let _ = std::fs::rename(path, dest);
}

/// `yyyy-MM-dd` in UTC, wie der Zeitstempel im Namen gesicherter Dateien.
pub fn corrupt_stamp(now: SystemTime) -> String {
    let secs = now.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Datum aus Tagen seit 1970-01-01 (proleptischer gregorianischer Kalender).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testutil::TempDir;
    use serde::Deserialize;
    use std::time::Duration;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Item {
        id: String,
    }

    fn item(id: &str) -> Item {
        Item { id: id.into() }
    }

    fn quarantined(dir: &Path, base: &str) -> bool {
        std::fs::read_dir(dir).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with(&format!("{base}.corrupt-")))
    }

    #[test]
    fn missing_file_starts_empty_without_error() {
        let d = TempDir::new();
        let r = load_array::<Item>(&d.path().join("x.json"), 1);
        assert!(r.items.is_empty() && !r.corrupted && r.message.is_none());
    }

    #[test]
    fn version_one_writes_bare_array_and_roundtrips() {
        let d = TempDir::new();
        let p = d.path().join("sub/x.json");
        save_array(&[item("a"), item("b")], &p, 1).unwrap();
        assert!(std::fs::read_to_string(&p).unwrap().trim_start().starts_with('['));
        assert_eq!(load_array::<Item>(&p, 1).items, vec![item("a"), item("b")]);
        assert!(!p.with_file_name("x.json.tmp").exists(), "keine Temp-Datei übrig");
    }

    #[test]
    fn version_two_writes_envelope_and_newer_is_flagged() {
        let d = TempDir::new();
        let p = d.path().join("x.json");
        save_array(&[item("a")], &p, 2).unwrap();
        let r = load_array::<Item>(&p, 2);
        assert_eq!(r.items, vec![item("a")]);
        assert!(!r.newer_than_known);
        assert!(load_array::<Item>(&p, 1).newer_than_known);
    }

    #[test]
    fn corrupt_file_is_quarantined_and_starts_empty() {
        let d = TempDir::new();
        let p = d.path().join("groups.json");
        std::fs::write(&p, "{kaputt").unwrap();
        let r = load_array::<Item>(&p, 1);
        assert!(r.items.is_empty() && r.corrupted);
        assert!(r.message.unwrap().contains("groups.json ist kaputt"));
        assert!(quarantined(d.path(), "groups"));
        assert!(!p.exists());
    }

    #[test]
    fn single_broken_entry_keeps_the_rest() {
        let d = TempDir::new();
        let p = d.path().join("groups.json");
        std::fs::write(&p, r##"[{"id":"g1"},{"id":123}]"##).unwrap();
        let r = load_array::<Item>(&p, 1);
        assert_eq!(r.items, vec![item("g1")]);
        assert_eq!(r.message.as_deref(), Some("groups.json: 1 von 2 Einträgen kaputt, Rest übernommen, Original gesichert"));
    }

    #[test]
    fn broken_entry_inside_envelope_is_recovered() {
        let d = TempDir::new();
        let p = d.path().join("x.json");
        std::fs::write(&p, r#"{"version":2,"items":[{"id":"a"},{"nope":1}]}"#).unwrap();
        assert_eq!(load_array::<Item>(&p, 2).items, vec![item("a")]);
        assert!(quarantined(d.path(), "x"));
    }

    #[test]
    fn dict_roundtrip_and_corruption() {
        let d = TempDir::new();
        let p = d.path().join("uses.json");
        let mut m = BTreeMap::new();
        m.insert("/p".to_string(), 3_i64);
        save_dict(&m, &p).unwrap();
        assert_eq!(load_dict::<i64>(&p), (m, None));
        std::fs::write(&p, "[1,2]").unwrap();
        let (empty, msg) = load_dict::<i64>(&p);
        assert!(empty.is_empty() && msg.is_some());
        assert!(quarantined(d.path(), "uses"));
    }

    #[test]
    fn stamp_is_utc_date() {
        assert_eq!(corrupt_stamp(SystemTime::UNIX_EPOCH), "1970-01-01");
        assert_eq!(corrupt_stamp(SystemTime::UNIX_EPOCH + Duration::from_secs(1_789_479_734)), "2026-09-15");
        assert_eq!(corrupt_stamp(SystemTime::UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29");
    }
}
