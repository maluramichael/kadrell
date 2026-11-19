//! Benutzte Ordner (`folders-uses.json`): jeder Start einer Session merkt ihren Ordner für die Frecency von ⌘N und
//! Schnellstart (Swift: `FolderIndex.shared.recordUse` in `startSession`).

use kadrell_claude::folder_index::FolderIndex;

use super::{AppState, now_secs};

impl AppState {
    /// Index der Ordner aus dem Profilordner (frisch gelesen, die Dateien sind die Quelle der Wahrheit).
    pub fn folder_index(&self) -> FolderIndex {
        FolderIndex::load(&self.profile.dir.to_string_lossy().replace('\\', "/"), &self.home())
    }

    /// Merkt, dass in `cwd` eine Session gestartet wurde.
    pub fn record_folder_use(&self, cwd: &str) {
        if !cwd.is_empty() {
            self.folder_index().record_use(&cwd.replace('\\', "/"), now_secs());
        }
    }
}
