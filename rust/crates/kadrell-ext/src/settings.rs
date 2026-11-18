//! Werte der `settings` aus dem Manifest. Normale liegen in den Einstellungen des Profils, `secret` im Schlüsselbund (ein
//! Eintrag pro Profil, Extension und Schlüssel), fehlt ein Wert, gilt der Default aus dem Manifest.

use std::sync::Arc;

use serde_json::{Map, Value};

use crate::bridge::{SecretStore, SettingsStore};
use crate::manifest::FoundExtension;

/// Eigener Dienst der Rust-App (kadrell-core `APP_ID`), getrennt von der Swift-App.
pub const KEYCHAIN_SERVICE: &str = "de.malura.kadrell.rust.extension";

/// Zugriff auf die Einstellungswerte einer Extension.
#[derive(Clone)]
pub struct ExtensionSettings {
    settings: Arc<dyn SettingsStore>,
    secrets: Arc<dyn SecretStore>,
    profile: Option<String>,
}

impl ExtensionSettings {
    /// `profile` ist der Profilname (`None` heißt Standardprofil); er trennt die Schlüsselbund-Einträge je Profil.
    pub fn new(
        settings: Arc<dyn SettingsStore>,
        secrets: Arc<dyn SecretStore>,
        profile: Option<String>,
    ) -> ExtensionSettings {
        ExtensionSettings {
            settings,
            secrets,
            profile,
        }
    }

    /// Schlüsselbund-Konto eines Wertes: `<profil>/<extension>/<schlüssel>`.
    pub fn account(&self, name: &str, key: &str) -> String {
        account(self.profile.as_deref(), name, key)
    }

    /// Alle Werte der Extension (gespeicherter Wert, sonst Default). Blockiert, wenn der Schlüsselbund es tut.
    pub fn values(&self, ext: &FoundExtension) -> Map<String, Value> {
        let mut out = Map::new();
        let Some(manifest) = &ext.manifest else {
            return out;
        };
        for s in &manifest.settings {
            if let Some(v) = self
                .stored(&s.kind, &ext.name, &s.key)
                .or_else(|| s.default.clone())
            {
                out.insert(s.key.clone(), v);
            }
        }
        out
    }

    /// `null` oder ein leerer Text löscht den Wert, danach gilt wieder der Default. Unbekannte Schlüssel ignoriert.
    pub fn set(&self, value: &Value, ext: &FoundExtension, key: &str) {
        let Some(setting) = ext
            .manifest
            .as_ref()
            .and_then(|m| m.settings.iter().find(|s| s.key == key))
        else {
            return;
        };
        let text = value.as_str().filter(|t| !t.is_empty());
        match setting.kind.as_str() {
            "secret" => {
                let account = self.account(&ext.name, key);
                match text {
                    Some(t) => {
                        self.secrets.write(KEYCHAIN_SERVICE, &account, t);
                    }
                    None => self.secrets.delete(KEYCHAIN_SERVICE, &account),
                }
            }
            "bool" => self.settings.set(
                &defaults_key(&ext.name, key),
                value.as_bool().map(Value::Bool),
            ),
            "number" => self.settings.set(
                &defaults_key(&ext.name, key),
                number(value).map(number_value),
            ),
            _ => self
                .settings
                .set(&defaults_key(&ext.name, key), text.map(Value::from)),
        }
    }

    fn stored(&self, kind: &str, name: &str, key: &str) -> Option<Value> {
        match kind {
            "secret" => self
                .secrets
                .read(KEYCHAIN_SERVICE, &self.account(name, key))
                .map(Value::from),
            "bool" => self
                .settings
                .get(&defaults_key(name, key))
                .filter(Value::is_boolean),
            "number" => self
                .settings
                .get(&defaults_key(name, key))
                .filter(Value::is_number),
            _ => self
                .settings
                .get(&defaults_key(name, key))
                .filter(Value::is_string),
        }
    }
}

/// Schlüsselbund-Konto: `<profil>/<extension>/<schlüssel>`, ohne Profilname `default`.
pub fn account(profile: Option<&str>, name: &str, key: &str) -> String {
    format!("{}/{name}/{key}", profile.unwrap_or("default"))
}

fn defaults_key(name: &str, key: &str) -> String {
    format!("extensions.{name}.{key}")
}

/// Zahl aus dem Formular: Text mit Punkt oder Komma, oder schon eine Zahl. Alles andere löscht den Wert.
pub fn number(value: &Value) -> Option<f64> {
    if let Some(n) = value.as_f64().filter(|_| value.is_number()) {
        return n.is_finite().then_some(n);
    }
    let text = value.as_str()?.trim().replace(',', ".");
    // "inf" und "nan" parst Rust, das Formular soll sie nicht annehmen (wie Swifts `Double`, das sie ebenfalls
    // kennt, aber danach `isFinite` prüft).
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Eine Zahl als JSON-Wert; ganze Zahlen ohne ".0", wie Swifts `JSONEncoder` sie schreibt.
pub fn number_value(n: f64) -> Value {
    if n == n.round() && n.abs() < 1e15 {
        Value::from(n as i64)
    } else {
        Value::from(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::MemoryStore;
    use crate::manifest::ExtensionManifest;
    use serde_json::json;

    fn ext(settings: Value) -> FoundExtension {
        let manifest = json!({"name": "x", "apiVersion": 1, "settings": settings});
        FoundExtension {
            name: "x".into(),
            dir: ".".into(),
            manifest: ExtensionManifest::parse(manifest.to_string().as_bytes()),
            problem: None,
        }
    }

    #[test]
    fn number_parsing() {
        assert_eq!(number(&json!(" 2,5 ")), Some(2.5));
        assert_eq!(number(&json!("60")), Some(60.0));
        assert_eq!(number(&json!(3)), Some(3.0));
        assert_eq!(number(&json!("")), None);
        assert_eq!(number(&json!("viel")), None);
        assert_eq!(number(&json!("inf")), None);
        assert_eq!(crate::json::number_text(&json!(5)), "5");
        assert_eq!(crate::json::number_text(&json!(2.5)), "2.5");
    }

    #[test]
    fn values_fall_back_to_defaults_and_secrets_live_in_the_store() {
        let store = Arc::new(MemoryStore::new());
        let s = ExtensionSettings::new(store.clone(), store.clone(), Some("tmp".into()));
        let e = ext(json!([
            {"key": "token", "type": "secret", "label": "T"},
            {"key": "mine", "type": "bool", "label": "M", "default": true},
            {"key": "every", "type": "number", "label": "E", "default": 5},
            {"key": "url", "type": "string", "label": "U", "default": "https://x"},
        ]));
        let v = s.values(&e);
        assert_eq!(v.get("token"), None);
        assert_eq!(v["mine"], true);
        assert_eq!(v["every"], 5);
        s.set(&json!("abc"), &e, "token");
        s.set(&json!(false), &e, "mine");
        s.set(&json!("2,5"), &e, "every");
        s.set(&json!("https://y"), &e, "url");
        s.set(&json!("ignoriert"), &e, "unbekannt");
        let v = s.values(&e);
        assert_eq!(v["token"], "abc");
        assert_eq!(v["mine"], false);
        assert_eq!(v["every"], 2.5);
        assert_eq!(v["url"], "https://y");
        assert_eq!(
            store.read(KEYCHAIN_SERVICE, "tmp/x/token").as_deref(),
            Some("abc"),
            "Konto: profil/extension/schlüssel"
        );
        s.set(&Value::Null, &e, "token");
        s.set(&json!(""), &e, "url");
        let v = s.values(&e);
        assert_eq!(
            v.get("token"),
            None,
            "null löscht den Eintrag im Schlüsselbund"
        );
        assert_eq!(v["url"], "https://x", "leerer Text: wieder der Default");
    }

    #[test]
    fn integral_numbers_are_written_without_fraction() {
        assert_eq!(number_value(5.0).to_string(), "5");
        assert_eq!(number_value(2.5).to_string(), "2.5");
    }
}
