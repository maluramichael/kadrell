//! Schalter für Sichtprüfungen ohne Eingabe, nur für die Entwicklung (die App liest sie beim Start):
//!
//! - `KADRELL_DEV_SEED=<n>`: legt n Terminal-Sessions an (ohne Claude), nur in einem Temp-Profil.
//! - `KADRELL_DEV_ZOOM=1`, `KADRELL_DEV_SYNC=1`, `KADRELL_DEV_AUTO=1`, `KADRELL_DEV_TREEFOCUS=1`: Zustand setzen.
//! - `KADRELL_DEV_SPLITS=dr`: Teilrichtungen für Frei, `KADRELL_DEV_SCROLL_COLS=3`: sichtbare Spalten beim Scrollen.
//! - `KADRELL_DEV_PREVIEW=<Gruppe>,<Session>`: Vorschau dieser Position im Baum.
//!
//! Zusammen mit `KADRELL_DEV_LAYOUT` und `KADRELL_DEV_SELECT` aus `lib.rs`.

use super::preview::Keyboard;
use crate::Shell;
use crate::ui::model::flat_index;

/// Namen der Wegwerf-Sessions, damit Gruppen und Kacheln in Screenshots lesbar sind.
const NAMES: [&str; 6] = ["backend", "frontend", "docs", "tests", "infra", "mail"];

fn flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| v == "1")
}

impl Shell {
    /// Setzt die Schalter oben. Legt zuerst die Sessions an, damit `KADRELL_DEV_SELECT` sie findet.
    pub(crate) fn apply_workspace_dev(&mut self) {
        self.seed_dev_sessions();
        self.state.zoom |= flag("KADRELL_DEV_ZOOM");
        self.ws.sync |= flag("KADRELL_DEV_SYNC");
        self.ws.tree_focus |= flag("KADRELL_DEV_TREEFOCUS");
        if let Ok(splits) = std::env::var("KADRELL_DEV_SPLITS") {
            self.layout.splits = splits;
        }
        if let Some(cols) = std::env::var("KADRELL_DEV_SCROLL_COLS").ok().and_then(|v| v.parse::<usize>().ok()) {
            self.layout.scroll_cols = cols.clamp(1, 12);
        }
        if flag("KADRELL_DEV_AUTO") {
            self.ws.auto.set(true);
        }
    }

    /// Die Vorschau erst, wenn der Baum steht (nach `rebuild`).
    pub(crate) fn apply_dev_preview(&mut self) {
        let Ok(spec) = std::env::var("KADRELL_DEV_PREVIEW") else { return };
        let Some(key) = crate::wire::keys_at(&self.groups, &spec).into_iter().next() else { return };
        self.ws.preview.step(&[key], None, Keyboard::Tile, 0);
    }

    fn seed_dev_sessions(&mut self) {
        let count = std::env::var("KADRELL_DEV_SEED").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        if count == 0 || !self.state.profile.temporary || !flat_index(&self.groups).is_empty() {
            return;
        }
        let home = self.state.home();
        let elsewhere = std::env::temp_dir().to_string_lossy().replace('\\', "/");
        for (i, name) in NAMES.iter().enumerate().take(count) {
            let cwd = if i % 3 == 2 { elsewhere.clone() } else { home.clone() };
            let key = self.state.new_shell_session(&cwd, false);
            self.state.rename_session(&key, name);
        }
        self.rebuild();
    }
}
