//! Speicher für Marke „neu“ und Tipp-Flags: die Einstellungen des Profils. Geschrieben wird nur der eine Schlüssel
//! (laden, ändern, speichern), damit der Einstellungen-Dialog, der die ganze Datei schreibt, nichts verliert.

use std::path::PathBuf;

use kadrell_claude::attention::AttentionStore;
use kadrell_core::Settings;
use serde_json::{Value, json};

/// Schreibt einen einzelnen Schlüssel in `settings.json` des Profils.
pub fn patch_setting(path: &std::path::Path, key: &str, value: Value) {
    let mut settings = Settings::load(path);
    settings.set(key, value);
    let _ = settings.save(path);
}

pub struct ProfileAttention {
    pub path: PathBuf,
}

impl AttentionStore for ProfileAttention {
    fn get_list(&self, key: &str) -> Vec<String> {
        Settings::load(&self.path).strings_or(key)
    }

    fn set_list(&mut self, key: &str, value: &[String]) {
        patch_setting(&self.path, key, json!(value));
    }

    fn get_bool(&self, key: &str) -> bool {
        Settings::load(&self.path).bool_or(key, false)
    }

    fn set_bool(&mut self, key: &str, value: bool) {
        patch_setting(&self.path, key, Value::Bool(value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_one_key_and_keeps_the_rest() {
        let dir = std::env::temp_dir().join(format!("kadrell-attn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, r#"{"claude.model":"opus"}"#).unwrap();
        let mut store = ProfileAttention { path: path.clone() };
        store.set_list("sessions.unseen", &["a".into(), "b".into()]);
        store.set_bool("tip.waiting.shown", true);
        assert_eq!(store.get_list("sessions.unseen"), ["a", "b"]);
        assert!(store.get_bool("tip.waiting.shown"));
        assert_eq!(Settings::load(&path).claude_model(), "opus");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
