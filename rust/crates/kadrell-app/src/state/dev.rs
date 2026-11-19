//! Entwicklungs-Schalter für Screenshots ohne Eingabe: `KADRELL_DEV_TILES=term,missing,ended,failed,starting` legt je
//! Eintrag eine Session an und versetzt ihre Kachel in den Zustand, damit alle Körper der Kachel zu sehen sind, ohne einen
//! echten Fehlstart nachzustellen. Nur für das Wegwerfprofil gedacht.

use std::time::Instant;

use super::tiles::{Run, Tile};
use super::AppState;

/// Letzter Bildschirm einer beendeten Kachel für die Ansicht.
const SAMPLE: [&str; 5] = ["> fasse die Änderungen zusammen", "", "⏺ Drei Dateien geändert, Tests laufen durch.", "  Soll ich einen Commit anlegen?", "> /exit"];

impl AppState {
    /// Legt die Sessions aus `KADRELL_DEV_TILES` an und wählt sie alle aus. `true`: es gab etwas zu tun.
    pub fn apply_dev_tiles(&mut self) -> bool {
        let Ok(spec) = std::env::var("KADRELL_DEV_TILES") else { return false };
        let base = std::env::temp_dir().join(format!("kadrell-dev-tiles-{}", std::process::id()));
        let kinds: Vec<&str> = spec.split(',').map(str::trim).filter(|k| !k.is_empty()).collect();
        let mut keys = Vec::new();
        for (i, kind) in kinds.iter().enumerate() {
            let dir = base.join(format!("{kind}{i}"));
            if *kind != "missing" {
                let _ = std::fs::create_dir_all(&dir);
            }
            let cwd = dir.to_string_lossy().replace('\\', "/");
            let key = if *kind == "term" { self.new_shell_session(&cwd, false) } else { self.new_claude_session(&cwd, None, false) };
            self.set_dev_state(&key, kind);
            keys.push(key);
        }
        self.set_selection(keys.clone(), keys.first().cloned());
        for (key, kind) in keys.iter().zip(&kinds) {
            self.set_dev_state(key, kind);
        }
        !keys.is_empty()
    }

    fn set_dev_state(&mut self, key: &str, kind: &str) {
        let rows: Vec<String> = SAMPLE.iter().map(|s| s.to_string()).collect();
        let tile = match kind {
            "ended" => Tile { state: Run::Ended, last_rows: super::lifecycle::ended_rows(rows), ..Tile::blank() },
            "failed" => Tile { state: Run::Ended, last_rows: rows.into_iter().take(1).collect(), exit_code: Some(1), started_at: Some(Instant::now()), ..Tile::blank() },
            "starting" => Tile::blank(),
            _ => return,
        };
        self.wanted.retain(|k| k != key);
        self.starts.forget(&[key.to_string()]);
        self.tiles.insert(key.to_string(), tile);
    }
}
