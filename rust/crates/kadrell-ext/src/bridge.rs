//! Was die App der Extension-Schicht beisteuert. Die Crate kennt weder den Steuerkanal noch den Schlüsselbund noch die
//! Einstellungsdatei, sie spricht nur diese Traits. Alle Methoden dürfen blockieren; die Crate ruft sie nie aus ihrem
//! internen Zustandslock heraus auf, sondern von Arbeitsthreads.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

/// Antwort auf einen Steuerbefehl (`kadrell.run`), wie die Ausgabe des CLI-Tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlResponse {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl ControlResponse {
    pub fn ok(stdout: &str) -> ControlResponse {
        ControlResponse {
            status: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    pub fn fail(message: &str) -> ControlResponse {
        ControlResponse {
            status: 1,
            stdout: String::new(),
            stderr: message.to_string(),
        }
    }
}

/// Führt einen `kadrell`-Befehl aus (`["ls", "--json"]`, `["select", "-t", key]`) und liefert Status und Ausgabe.
/// Die App bindet hier ihren Steuerkanal an; jede Anfrage muss eine Antwort bekommen.
pub trait ControlRunner: Send + Sync {
    fn run(&self, argv: &[String]) -> ControlResponse;
}

impl<F> ControlRunner for F
where
    F: Fn(&[String]) -> ControlResponse + Send + Sync,
{
    fn run(&self, argv: &[String]) -> ControlResponse {
        self(argv)
    }
}

/// Schlüsselbund (macOS) oder Credential Manager (Windows). `service` und `account` bestimmt die Crate.
pub trait SecretStore: Send + Sync {
    fn read(&self, service: &str, account: &str) -> Option<String>;
    /// `true`, wenn gespeichert.
    fn write(&self, service: &str, account: &str, value: &str) -> bool;
    fn delete(&self, service: &str, account: &str);
}

/// Einstellungen des Profils als Schlüssel-Wert-Speicher (Werte sind JSON). Die Crate benutzt `extensions.enabled`,
/// `extensions.origins`, `extensions.acknowledged` und `extensions.<name>.<key>`.
pub trait SettingsStore: Send + Sync {
    fn get(&self, key: &str) -> Option<Value>;
    /// `None` löscht den Wert.
    fn set(&self, key: &str, value: Option<Value>);
}

/// Speicher im Arbeitsspeicher für Tests und einfache Einbettungen.
#[derive(Default)]
pub struct MemoryStore {
    secrets: Mutex<HashMap<(String, String), String>>,
    settings: Mutex<HashMap<String, Value>>,
}

impl MemoryStore {
    pub fn new() -> MemoryStore {
        MemoryStore::default()
    }
}

impl SecretStore for MemoryStore {
    fn read(&self, service: &str, account: &str) -> Option<String> {
        self.secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(service.into(), account.into()))
            .cloned()
    }

    fn write(&self, service: &str, account: &str, value: &str) -> bool {
        self.secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert((service.into(), account.into()), value.into());
        true
    }

    fn delete(&self, service: &str, account: &str) {
        self.secrets
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&(service.to_string(), account.to_string()));
    }
}

impl SettingsStore for MemoryStore {
    fn get(&self, key: &str) -> Option<Value> {
        self.settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned()
    }

    fn set(&self, key: &str, value: Option<Value>) {
        let mut map = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        match value {
            Some(v) => map.insert(key.to_string(), v),
            None => map.remove(key),
        };
    }
}

/// Schlüssel in `SettingsStore`.
pub mod keys {
    pub const ENABLED: &str = "extensions.enabled";
    pub const ORIGINS: &str = "extensions.origins";
    pub const ACKNOWLEDGED: &str = "extensions.acknowledged";
}

/// Liste der eingeschalteten Extensions.
pub fn enabled_extensions(store: &dyn SettingsStore) -> Vec<String> {
    string_list(store.get(keys::ENABLED))
}

pub fn set_enabled_extensions(store: &dyn SettingsStore, names: &[String]) {
    store.set(keys::ENABLED, Some(Value::from(names.to_vec())));
}

/// Extensions, deren erstes Einschalten der Nutzer schon bestätigt hat (fremder Code läuft ohne Sandbox).
pub fn acknowledged_extensions(store: &dyn SettingsStore) -> Vec<String> {
    string_list(store.get(keys::ACKNOWLEDGED))
}

pub fn acknowledge_extension(store: &dyn SettingsStore, name: &str) {
    let mut list = acknowledged_extensions(store);
    if !list.iter().any(|n| n == name) {
        list.push(name.to_string());
        store.set(keys::ACKNOWLEDGED, Some(Value::from(list)));
    }
}

/// Herkunft installierter Extensions: Ordnername zu `owner/repo`.
pub fn extension_origins(store: &dyn SettingsStore) -> HashMap<String, String> {
    store
        .get(keys::ORIGINS)
        .and_then(|v| v.as_object().cloned())
        .map(|m| {
            m.into_iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

pub fn set_extension_origin(store: &dyn SettingsStore, name: &str, origin: &str) {
    let mut origins = extension_origins(store);
    origins.insert(name.to_string(), origin.to_string());
    let map: serde_json::Map<String, Value> = origins
        .into_iter()
        .map(|(k, v)| (k, Value::from(v)))
        .collect();
    store.set(keys::ORIGINS, Some(Value::Object(map)));
}

fn string_list(v: Option<Value>) -> Vec<String> {
    v.and_then(|v| v.as_array().cloned())
        .map(|a| {
            a.into_iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_list_roundtrip() {
        let store = MemoryStore::new();
        assert!(enabled_extensions(&store).is_empty());
        set_enabled_extensions(&store, &["a".into(), "b".into()]);
        assert_eq!(enabled_extensions(&store), ["a", "b"]);
    }

    #[test]
    fn origins_and_acknowledged() {
        let store = MemoryStore::new();
        set_extension_origin(&store, "demo", "me/demo");
        assert_eq!(
            extension_origins(&store).get("demo").map(String::as_str),
            Some("me/demo")
        );
        acknowledge_extension(&store, "demo");
        acknowledge_extension(&store, "demo");
        assert_eq!(acknowledged_extensions(&store), ["demo"]);
    }

    #[test]
    fn closures_are_control_runners() {
        let runner = |argv: &[String]| ControlResponse::ok(&argv.join(" "));
        assert_eq!(ControlRunner::run(&runner, &["ls".into()]).stdout, "ls");
    }
}
