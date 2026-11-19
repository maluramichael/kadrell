//! Einstellungen der Arbeitsfläche, die beim Bedienen entstehen (Verhältnisse der Layouts, Teilungen von Frei,
//! Spaltenzahlen, Auto-Modus). Geschrieben wird nur der eine Schlüssel, damit der Einstellungen-Dialog und andere
//! Schreiber nichts verlieren (wie bei `set_sidebar`). Dazu der Fokus auf Sessions außerhalb der Auswahl.

use serde_json::Value;

use super::AppState;
use super::attention::patch_settings;

impl AppState {
    /// Setzt oder entfernt (`None`) einen Schlüssel im Zustand und in `settings.json` des Profils.
    pub fn save_workspace_setting(&mut self, key: &str, value: Option<Value>) {
        match &value {
            Some(v) => self.settings.set(key, v.clone()),
            None => self.settings.remove(key),
        }
        patch_settings(&self.profile.settings_path(), vec![(key.to_string(), value)]);
    }

    /// Fokus auf eine Session, die nicht zur Auswahl gehört: im Auto-Modus zeigt die Arbeitsfläche auch Sessions, die
    /// niemand ausgewählt hat, und der Fokus darf dort liegen. Die Auswahl bleibt, die Marke „neu“ ist quittiert.
    pub fn focus_any(&mut self, key: &str) {
        if self.store.sessions.get(key).is_none() || self.focused.as_deref() == Some(key) {
            return;
        }
        self.count_focus_switch(&self.focused.clone(), &Some(key.to_string()));
        self.last_focused = self.focused.take();
        self.focused = Some(key.to_string());
        self.attention.mark_seen(key);
    }

    /// Sessions ohne Prozess sofort starten lassen, etwa die Auswahl eines weiteren Fensters (`start_selected_now`).
    pub fn start_now(&mut self, keys: &[String]) {
        let waiting: Vec<String> = keys.iter().filter(|k| self.run(k) == super::tiles::Run::NotStarted && !self.wanted.contains(k)).cloned().collect();
        self.starts.enqueue(waiting.clone());
        for key in self.starts.take_all(&waiting) {
            if !self.wanted.contains(&key) {
                self.wanted.push(key);
            }
        }
    }
}
