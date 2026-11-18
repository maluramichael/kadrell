//! Die Credential-Datei der `claude`-CLI. Auf Windows (und Linux) liegt der Live-Eintrag dort statt im
//! Schlüsselbund: `~/.claude/.credentials.json` mit den Schlüsseln `claudeAiOauth` und `mcpOAuth`.
//! Reine Dateifunktionen, auf jedem System testbar (Tests nutzen nur Temp-Ordner).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Pfad der Credential-Datei: `$CLAUDE_CONFIG_DIR/.credentials.json`, sonst `~/.claude/.credentials.json`.
/// (Die Variable `CLAUDE_CONFIG_DIR` ist nicht verifiziert, Annahme: die CLI wertet sie wie `~/.claude` aus.)
pub fn live_credentials_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir).join(".credentials.json"));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".claude")
            .join(".credentials.json"),
    )
}

/// Inhalt der Datei, getrimmt; `None` bei fehlender oder leerer Datei.
pub fn read_live_file(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Schreibt über eine Temp-Datei im selben Ordner und benennt sie um, damit die CLI nie eine halbe Datei liest.
pub fn write_live_file(path: &Path, value: &str) -> bool {
    write_atomic(path, value).is_ok()
}

fn write_atomic(path: &Path, value: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".credentials.{}.tmp", std::process::id()));
    let mut file = fs::File::create(&tmp)?;
    file.write_all(value.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("kadrell-accounts-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_then_read_roundtrip_and_overwrite() {
        let dir = temp_dir("live");
        let file = dir.join(".credentials.json");
        assert_eq!(read_live_file(&file), None);
        assert!(write_live_file(&file, "{\"a\":1}"));
        assert_eq!(read_live_file(&file).as_deref(), Some("{\"a\":1}"));
        assert!(write_live_file(&file, "{\"a\":2}\n"));
        assert_eq!(read_live_file(&file).as_deref(), Some("{\"a\":2}"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_file_counts_as_missing() {
        let dir = temp_dir("empty");
        let file = dir.join(".credentials.json");
        fs::write(&file, "  \n").unwrap();
        assert_eq!(read_live_file(&file), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
