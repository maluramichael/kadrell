//! Wo die laufende App lauscht: auf macOS und Unix ein Unix-Socket im Profilordner (Dateirechte 0600), auf
//! Windows eine Endpunktdatei mit Port und Token (TCP auf 127.0.0.1, wie die erste Windows-Version). `KADRELL_SOCKET`
//! zeigt in jeder Kachel auf diesen Pfad und überschreibt in der CLI den Standard.

use kadrell_core::Paths;
use kadrell_core::profile::TEMP_NAME;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Dateiname im Profilordner: Unix-Socket auf Unix, Endpunktdatei auf Windows.
pub const ENDPOINT_FILE: &str = if cfg!(windows) { "kadrell.endpoint" } else { "kadrell.sock" };

/// Pfad des Steuerkanals im Profilordner `dir`.
pub fn endpoint_path(dir: &Path) -> PathBuf {
    dir.join(ENDPOINT_FILE)
}

/// Standardpfad für die CLI ohne `KADRELL_SOCKET`: Standardprofil oder benanntes Profil. `None` beim Wegwerfprofil
/// `tmp`: dessen Ordner trägt die pid der Instanz im Namen, ohne `KADRELL_SOCKET` ist es nicht auffindbar.
pub fn default_endpoint_path(paths: &Paths, profile: Option<&str>) -> Option<PathBuf> {
    let dir = match profile {
        None => paths.default_profile_dir(),
        Some(TEMP_NAME) => return None,
        Some(name) => paths.named_profile_dir(name),
    };
    Some(endpoint_path(&dir))
}

/// Inhalt der Endpunktdatei (Windows): Port, Token und pid der Instanz. Auf Unix ungenutzt, aber überall lesbar,
/// damit Format und Rundlauf auf jedem System getestet werden.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Endpoint {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

impl Endpoint {
    pub fn read(path: &Path) -> Option<Endpoint> {
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_vec(self)?)
    }

    /// Zufälliges Token, 64 Hex-Zeichen.
    pub fn new_token() -> String {
        format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paths_follow_the_profile() {
        let paths = Paths::new("/h", "/h/support", "/tmp");
        let file = ENDPOINT_FILE;
        assert_eq!(default_endpoint_path(&paths, None), Some(Path::new("/h/support").join(file)));
        assert_eq!(default_endpoint_path(&paths, Some("work")), Some(Path::new("/h/support").join("profiles").join("work").join(file)));
        assert_eq!(default_endpoint_path(&paths, Some("tmp")), None);
    }

    #[test]
    fn endpoint_file_round_trip() {
        let dir = crate::testing::TempDir::new();
        let path = dir.path().join("kadrell.endpoint");
        assert_eq!(Endpoint::read(&path), None);
        let ep = Endpoint { port: 4711, token: Endpoint::new_token(), pid: 7 };
        ep.write(&path).unwrap();
        assert_eq!(Endpoint::read(&path), Some(ep.clone()));
        assert_eq!(ep.token.len(), 64);
        std::fs::write(&path, "kaputt").unwrap();
        assert_eq!(Endpoint::read(&path), None);
    }
}
