//! Lesende Systemwerte für `kadrell.system` im Helper: CPU-Last je Kern, Hauptspeicher und Plattenbelegung. Nichts davon
//! läuft von selbst, gemessen wird nur, wenn eine Extension fragt.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};
use sysinfo::{Disks, System};

use crate::manifest::home_dir;

/// Abstand der beiden Messungen beim ersten Aufruf.
pub const FIRST_SAMPLE_WAIT: Duration = Duration::from_millis(250);

/// Last je Kern (0...1) seit dem letzten Aufruf; der erste Aufruf misst zweimal im Abstand von 250 ms.
#[derive(Default)]
pub struct CpuProbe {
    system: Option<System>,
}

impl CpuProbe {
    /// Blockiert beim ersten Aufruf 250 ms, darum auf einem Arbeitsthread benutzen.
    pub fn sample(&mut self) -> Option<Vec<f64>> {
        if self.system.is_none() {
            let mut system = System::new().ok()?;
            system.refresh_cpu_usage();
            std::thread::sleep(FIRST_SAMPLE_WAIT);
            self.system = Some(system);
        }
        let system = self.system.as_mut()?;
        system.refresh_cpu_usage();
        let cores: Vec<f64> = system.cpus().iter().map(|c| f64::from(c.usage() / 100.0).clamp(0.0, 1.0)).collect();
        (!cores.is_empty()).then_some(cores)
    }
}

/// Ergebnis für Lua: `cores` je Kern, `total` als Mittel über alle.
pub fn cpu_fields(cores: &[f64]) -> Map<String, Value> {
    let total = if cores.is_empty() { 0.0 } else { cores.iter().sum::<f64>() / cores.len() as f64 };
    let mut m = Map::new();
    m.insert("cores".into(), json!(cores));
    m.insert("total".into(), json!(total));
    m
}

/// Auslastung des Hauptspeichers: `used` ist Gesamtspeicher minus verfügbarer, `free` der Rest. Bytes und `fraction`
/// 0...1, `None`, wenn das System nichts liefert.
pub fn mem() -> Option<Map<String, Value>> {
    let mut system = System::new().ok()?;
    system.refresh_memory();
    let total = system.total_memory();
    if total == 0 {
        return None;
    }
    let used = system.used_memory().min(total);
    let (t, u) = (total as f64, used as f64);
    let mut m = Map::new();
    m.insert("total".into(), json!(t));
    m.insert("used".into(), json!(u));
    m.insert("free".into(), json!(t - u));
    m.insert("fraction".into(), json!(u / t));
    Some(m)
}

/// Belegung des Volumes, auf dem `path` liegt (`~` geht). Bytes als Zahlen, `None`, wenn der Pfad nicht lesbar ist.
pub fn disk(path: &str) -> Option<Map<String, Value>> {
    let resolved = resolve(path)?;
    let disks = Disks::new_with_refreshed_list().ok()?;
    let best = disks
        .iter()
        .filter(|d| resolved.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().components().count())?;
    let total = best.total_space();
    if total == 0 {
        return None;
    }
    let (t, f) = (total as f64, (best.available_space().min(total)) as f64);
    let mut m = Map::new();
    m.insert("total".into(), json!(t));
    m.insert("free".into(), json!(f));
    m.insert("used".into(), json!(t - f));
    m.insert("fraction".into(), json!((t - f) / t));
    m.insert("name".into(), json!(best.name().to_string_lossy()));
    Some(m)
}

/// Absoluter Pfad ohne `\\?\`-Präfix; `None`, wenn es ihn nicht gibt.
fn resolve(path: &str) -> Option<PathBuf> {
    let expanded = match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => {
            home_dir().join(rest.trim_start_matches(['/', '\\']))
        }
        _ => PathBuf::from(path),
    };
    let canon = std::fs::canonicalize(expanded).ok()?;
    Some(strip_verbatim(&canon))
}

fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => p.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_fields_average() {
        let fields = cpu_fields(&[0.5, 0.25]);
        assert_eq!(fields["total"], 0.375);
        assert_eq!(fields["cores"], json!([0.5, 0.25]));
        assert_eq!(cpu_fields(&[])["total"], 0.0);
    }

    #[test]
    fn real_cpu_cores() {
        let mut probe = CpuProbe::default();
        let cores = probe.sample().expect("cpu");
        assert!(!cores.is_empty());
        assert!(cores.iter().all(|c| (0.0..=1.0).contains(c)));
        assert!(probe.sample().is_some(), "zweiter Aufruf ohne Wartezeit");
    }

    #[test]
    fn real_disk() {
        let root = if cfg!(windows) { "C:\\" } else { "/" };
        let info = disk(root).expect("disk");
        let total = info["total"].as_f64().unwrap();
        let free = info["free"].as_f64().unwrap();
        assert!(total > 0.0);
        assert!(free <= total);
        assert!((0.0..=1.0).contains(&info["fraction"].as_f64().unwrap()));
        assert!(disk("/gibt/es/nicht").is_none());
        assert!(disk("~").is_some(), "~ wird aufgelöst");
    }

    #[test]
    fn real_mem() {
        let mem = mem().expect("mem");
        let (total, used) = (mem["total"].as_f64().unwrap(), mem["used"].as_f64().unwrap());
        let (free, fraction) = (mem["free"].as_f64().unwrap(), mem["fraction"].as_f64().unwrap());
        assert!(total > 0.0);
        assert!(used <= total);
        assert!((used - (total - free)).abs() <= 1.0);
        assert!((fraction - used / total).abs() < 1e-12);
        assert!(fraction <= 1.0);
    }
}
