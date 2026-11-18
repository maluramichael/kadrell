//! Profile wie auf dem Mac: ohne Namen das Standardprofil unter `%APPDATA%\Kadrell`, `--profile <name>` unter
//! `profiles\<name>`, `--profile tmp` ein Wegwerfprofil in `%TEMP%\kadrell-tmp-<pid>`, das beim Beenden
//! verschwindet und die Einstellungen des Standardprofils übernimmt. Je Profil läuft höchstens eine Instanz.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub struct Profile {
    pub dir: PathBuf,
    pub temporary: bool,
}

pub fn home() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn default_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home().join("AppData/Roaming")).join("Kadrell")
}

impl Profile {
    pub fn open(name: Option<&str>) -> std::io::Result<Profile> {
        let (dir, temporary) = match name {
            None => (default_dir(), false),
            Some("tmp") => (std::env::temp_dir().join(format!("kadrell-tmp-{}", std::process::id())), true),
            Some(n) => (default_dir().join("profiles").join(n), false),
        };
        std::fs::create_dir_all(&dir)?;
        if temporary {
            cleanup_stale_temp_profiles();
            if let Ok(data) = std::fs::read(default_dir().join(SETTINGS)) {
                std::fs::write(dir.join(SETTINGS), data)?;
            }
        }
        Ok(Profile { dir, temporary })
    }

    pub fn endpoint_path(&self) -> PathBuf {
        self.dir.join(ENDPOINT)
    }

    pub fn settings(&self) -> Settings {
        std::fs::read(self.dir.join(SETTINGS)).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
    }

    /// Beim Beenden: der Endpunkt verschwindet, ein Temp-Profil ganz.
    pub fn close(&self) {
        let _ = std::fs::remove_file(self.endpoint_path());
        if self.temporary {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

pub const ENDPOINT: &str = "kadrell.endpoint";
const SETTINGS: &str = "settings.json";

/// Reste abgestürzter Temp-Profile: Ordner, deren Instanz nicht mehr antwortet.
fn cleanup_stale_temp_profiles() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let own = name == format!("kadrell-tmp-{}", std::process::id());
        if name.starts_with("kadrell-tmp-") && !own && !alive(&entry.path()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// Antwortet die Instanz hinter diesem Profilordner noch?
pub fn alive(dir: &Path) -> bool {
    crate::protocol::Endpoint::read(&dir.join(ENDPOINT))
        .is_some_and(|e| std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], e.port).into(), std::time::Duration::from_millis(300)).is_ok())
}

/// `settings.json` im Profilordner, von Hand gepflegt, bis es einen Einstellungsdialog gibt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub claude: ClaudeSettings,
    /// Darf eine Kachel Sessions anderer Gruppen steuern (Mac: „Sessions dürfen andere Sessions steuern“).
    pub control_other_sessions: bool,
    /// Pfad zu `claude`, leer = über PATH suchen.
    pub claude_binary: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { claude: ClaudeSettings::default(), control_other_sessions: true, claude_binary: String::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct ClaudeSettings {
    pub allow_bypass: bool,
    /// `--permission-mode`, leer = Claude-Standard.
    pub mode: String,
    pub model: String,
    pub effort: String,
    /// Fullscreen-Renderer (`CLAUDE_CODE_NO_FLICKER=1`), wie auf dem Mac standardmäßig an.
    pub fullscreen: bool,
}

impl Default for ClaudeSettings {
    fn default() -> Self {
        ClaudeSettings { allow_bypass: false, mode: String::new(), model: String::new(), effort: String::new(), fullscreen: true }
    }
}

impl ClaudeSettings {
    /// Start-Flags wie `ClaudeCLI.launchArgs`; Modell und Effort der Session gehen der Einstellung vor.
    pub fn launch_args(&self, model: Option<&str>, effort: Option<&str>) -> Vec<String> {
        let mut args = Vec::new();
        if self.allow_bypass {
            args.push("--allow-dangerously-skip-permissions".to_string());
        }
        for (flag, value) in [("--permission-mode", Some(self.mode.as_str())), ("--model", model.or(Some(&self.model))), ("--effort", effort.or(Some(&self.effort)))] {
            if let Some(v) = value.filter(|v| !v.is_empty()) {
                args.extend([flag.to_string(), v.to_string()]);
            }
        }
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_args_prefer_session_values() {
        let s = ClaudeSettings { allow_bypass: true, mode: "bypassPermissions".into(), model: "opus".into(), ..Default::default() };
        assert_eq!(
            s.launch_args(Some("sonnet"), Some("high")),
            ["--allow-dangerously-skip-permissions", "--permission-mode", "bypassPermissions", "--model", "sonnet", "--effort", "high"]
        );
        assert_eq!(ClaudeSettings::default().launch_args(None, None), Vec::<String>::new());
    }

    #[test]
    fn settings_parse_partial_json() {
        let s: Settings = serde_json::from_str(r#"{"claude":{"mode":"auto"}}"#).unwrap();
        assert_eq!(s.claude.mode, "auto");
        assert!(s.claude.fullscreen);
        assert!(s.control_other_sessions);
    }
}
