//! Geheimnisse hinter einem Trait (Port von `Keychain.swift`).
//!
//! - macOS: das `/usr/bin/security`-CLI, genau wie die Swift-App. Nur über dasselbe Binärprogramm erreicht man den
//!   Eintrag `Claude Code-credentials` ohne Nachfrage.
//! - Windows: der Live-Eintrag `Claude Code-credentials` ist die Datei `~/.claude/.credentials.json` (die `claude`-CLI
//!   legt ihn dort ab, im Credential Manager liegt er nicht). Die Slots der hinterlegten Konten
//!   (`de.malura.kadrell.account`) liegen im Windows Credential Manager als generische Einträge
//!   `<service>/<account>`; ein Eintrag fasst höchstens 2560 Bytes, ein OAuth-Block ist deutlich kleiner.
//! - Tests: [`MemorySecretStore`].

use std::collections::BTreeMap;
use std::sync::Mutex;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// Der Eintrag, den die `claude`-CLI liest.
pub const LIVE_SERVICE: &str = "Claude Code-credentials";
/// Die Slots der hinterlegten Konten (`account` ist die Konto-Id).
/// Eigener Dienst der Rust-App (kadrell-core `APP_ID`), getrennt von den Konten der Swift-App.
pub const STORE_SERVICE: &str = "de.malura.kadrell.rust.account";

pub trait SecretStore: Send + Sync {
    /// Wert eines Eintrags, `None` bei fehlendem Eintrag oder Fehler. `account` grenzt zusätzlich ein.
    fn read(&self, service: &str, account: Option<&str>) -> Option<String>;
    /// Legt an oder überschreibt. `false` bei einem Fehler.
    fn write(&self, service: &str, account: &str, value: &str) -> bool;
    fn delete(&self, service: &str, account: &str);
}

/// Der Speicher der laufenden Plattform. Auf anderen Systemen gibt es keinen (die App läuft nur auf macOS und Windows).
#[cfg(target_os = "macos")]
pub fn default_secret_store() -> Box<dyn SecretStore> {
    Box::new(macos::SecuritySecretStore)
}

#[cfg(windows)]
pub fn default_secret_store() -> Box<dyn SecretStore> {
    Box::new(windows::WindowsSecretStore::new(
        crate::live_file::live_credentials_path(),
    ))
}

/// Speicher im Arbeitsspeicher für Tests, fasst nie echte Zugangsdaten an.
#[derive(Default)]
pub struct MemorySecretStore {
    items: Mutex<BTreeMap<(String, String), String>>,
    fail_writes: Mutex<bool>,
    writes: Mutex<Vec<(String, String)>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Legt einen Eintrag direkt an (Testaufbau).
    pub fn with(self, service: &str, account: &str, value: &str) -> Self {
        self.items
            .lock()
            .unwrap()
            .insert((service.into(), account.into()), value.into());
        self
    }

    /// Lässt alle weiteren Schreibvorgänge scheitern.
    pub fn set_fail_writes(&self, fail: bool) {
        *self.fail_writes.lock().unwrap() = fail;
    }

    /// Wert ohne Trait-Umweg, für Prüfungen in Tests.
    pub fn get(&self, service: &str, account: &str) -> Option<String> {
        self.items
            .lock()
            .unwrap()
            .get(&(service.to_string(), account.to_string()))
            .cloned()
    }

    /// Reihenfolge der erfolgreichen Schreibvorgänge als (Service, Konto).
    pub fn write_log(&self) -> Vec<(String, String)> {
        self.writes.lock().unwrap().clone()
    }
}

impl SecretStore for MemorySecretStore {
    fn read(&self, service: &str, account: Option<&str>) -> Option<String> {
        let items = self.items.lock().unwrap();
        let found = match account {
            Some(a) => items.get(&(service.to_string(), a.to_string())),
            None => items
                .iter()
                .find(|((s, _), _)| s == service)
                .map(|(_, v)| v),
        };
        found
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    fn write(&self, service: &str, account: &str, value: &str) -> bool {
        if *self.fail_writes.lock().unwrap() {
            return false;
        }
        self.items
            .lock()
            .unwrap()
            .insert((service.into(), account.into()), value.into());
        self.writes
            .lock()
            .unwrap()
            .push((service.into(), account.into()));
        true
    }

    fn delete(&self, service: &str, account: &str) {
        self.items
            .lock()
            .unwrap()
            .remove(&(service.to_string(), account.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_reads_writes_and_deletes() {
        let s = MemorySecretStore::new();
        assert_eq!(s.read("svc", Some("a")), None);
        assert!(s.write("svc", "a", "wert"));
        assert_eq!(s.read("svc", Some("a")).as_deref(), Some("wert"));
        assert_eq!(
            s.read("svc", None).as_deref(),
            Some("wert"),
            "ohne Konto: irgendein Eintrag des Service"
        );
        assert_eq!(s.read("svc", Some("b")), None);
        s.delete("svc", "a");
        assert_eq!(s.read("svc", None), None);
    }

    #[test]
    fn failing_writes_leave_items_untouched() {
        let s = MemorySecretStore::new().with("svc", "a", "alt");
        s.set_fail_writes(true);
        assert!(!s.write("svc", "a", "neu"));
        assert_eq!(s.get("svc", "a").as_deref(), Some("alt"));
    }
}
