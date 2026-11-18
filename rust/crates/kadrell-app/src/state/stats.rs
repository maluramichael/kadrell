//! Lebenszeit-Zähler und Konten im Zustand. Die Zähler gehören `kadrell_core::stats` und liegen in den Einstellungen des
//! Profils (`stats.*`); `kadrell_accounts::Stats` gibt es nur als Port von Swift und wird hier nicht benutzt, damit
//! nichts doppelt zählt. Jede Änderung geht sofort einzeln in die Datei: der Einstellungen-Dialog arbeitet auf einer
//! Kopie, und ohne [`keep_live_keys`] schriebe er beim Schließen alte Zähler und Konten zurück.

use kadrell_core::Settings;
use kadrell_core::stats::{self, StatKey};
use serde_json::Value;

use super::attention::patch_settings;
use super::{AppState, now_secs};
use crate::usage::hub::Persist;

/// Schlüssel, die die App selbst schreibt und der Dialog nie ändert.
fn is_live_key(key: &str) -> bool {
    key.starts_with("stats.") || key == "accounts.index" || key == "accounts.active"
}

/// Übernimmt Zähler und Konten aus dem Live-Stand in die Kopie des Dialogs, damit dessen Diff sie nicht anfasst.
pub fn keep_live_keys(new: &mut Settings, live: &Settings) {
    let stale: Vec<String> = new.keys().filter(|k| is_live_key(k)).cloned().collect();
    for key in stale {
        new.remove(&key);
    }
    for key in live.keys().filter(|k| is_live_key(k)) {
        if let Some(v) = live.get(key) {
            new.set(key, v.clone());
        }
    }
}

impl AppState {
    /// Schreibt die Zähler-Schlüssel, die sich gegenüber der Datei geändert haben könnten.
    fn save_stats(&self) {
        let changes: Vec<(String, Option<Value>)> =
            self.settings.keys().filter(|k| k.starts_with("stats.")).map(|k| (k.clone(), self.settings.get(k).cloned())).collect();
        patch_settings(&self.profile.settings_path(), changes);
    }

    pub fn count_stat(&mut self, key: StatKey, by: i64) {
        if by <= 0 {
            return;
        }
        stats::bump(&mut self.settings, key, by, now_secs());
        self.save_stats();
    }

    /// Eine neue Session oder ein neues Terminal: zählen und den Rekord gleichzeitig offener Sessions nachziehen.
    pub fn count_opened(&mut self, shell: bool) {
        stats::bump(&mut self.settings, if shell { StatKey::Terminals } else { StatKey::Sessions }, 1, now_secs());
        stats::note_concurrent(&mut self.settings, self.store.sessions.sessions.len() as i64, now_secs());
        self.save_stats();
    }

    /// Fokus ist von einer Kachel auf eine andere gewandert.
    pub fn count_focus_switch(&mut self, old: &Option<String>, new: &Option<String>) {
        if old.is_some() && new.is_some() && old != new {
            self.count_stat(StatKey::FocusSwitches, 1);
        }
    }

    /// Neu gespeicherter Stand der Konten (`accounts.index`, `accounts.active`).
    pub fn save_accounts(&mut self, p: &Persist) {
        self.settings.set_accounts_index(Some(&p.index));
        self.settings.set_active_account_id(p.active.as_deref());
        patch_settings(
            &self.profile.settings_path(),
            vec![("accounts.index".into(), Some(Value::String(p.index.clone()))), ("accounts.active".into(), p.active.clone().map(Value::String))],
        );
    }

    /// Schaltet den Auto-Wechsel um (Haken im Konto-Menü).
    pub fn set_autoswitch(&mut self, on: bool) {
        self.settings.set_autoswitch_enabled(on);
        patch_settings(&self.profile.settings_path(), vec![("autoswitch.enabled".into(), Some(Value::Bool(on)))]);
    }

    /// Schaltet eine Meldung („Nicht mehr anzeigen“) dauerhaft ab.
    pub fn set_notice_off(&mut self, key: &str) {
        self.settings.set(key, Value::Bool(false));
        patch_settings(&self.profile.settings_path(), vec![(key.to_string(), Some(Value::Bool(false)))]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_copy_never_reverts_counters_or_accounts() {
        let mut live = Settings::default();
        stats::bump(&mut live, StatKey::Messages, 9, 1_800_000_000.);
        live.set_accounts_index(Some("[]"));
        let mut dialog = Settings::default();
        stats::bump(&mut dialog, StatKey::Messages, 2, 1_700_000_000.);
        dialog.set_accounts_index(Some("[{}]"));
        dialog.set("autoswitch.threshold", Value::from(80));
        keep_live_keys(&mut dialog, &live);
        assert_eq!(stats::count(&dialog, StatKey::Messages), 9);
        assert_eq!(dialog.accounts_index().as_deref(), Some("[]"));
        assert_eq!(dialog.get("autoswitch.threshold"), Some(&Value::from(80)), "andere Schlüssel bleiben");
    }

    #[test]
    fn live_keys_are_recognised() {
        assert!(is_live_key("stats.recordDate") && is_live_key("accounts.active"));
        assert!(!is_live_key("autoswitch.enabled") && !is_live_key("sidebar.sort"));
    }
}
