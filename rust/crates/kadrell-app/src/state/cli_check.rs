//! Prüfung des `claude`-Binarys (Swift: `AppDelegate.recheckCLI`, `ClaudeCLI.checkVersion`). Fehlt es oder liefert
//! `claude --version` nichts Brauchbares, ist das ein blockierender Fehler (der Leerzustand bietet „Erneut prüfen“).
//! Zu alt blockiert nicht, nur die Leiste warnt. Läuft auf dem Hintergrund-Executor, die Ergebnisse trägt
//! [`AppState::apply_cli_health`] ins Register ein.

use kadrell_claude::cli::{ClaudeCli, SystemRunner, UPDATE_COMMAND, VersionCheck, find_in_path, path_exts};
use kadrell_i18n::trd;

use super::AppState;
use super::launch::LaunchEnv;

/// Installationsbefehl von Windows (PowerShell), laut code.claude.com/docs/en/setup (Stand 2026-11).
pub const INSTALL_COMMAND_WINDOWS: &str = "irm https://claude.ai/install.ps1 | iex";

/// Der Befehl, den „Installationsbefehl kopieren“ in die Zwischenablage legt.
pub fn install_command() -> &'static str {
    if cfg!(windows) { INSTALL_COMMAND_WINDOWS } else { kadrell_claude::cli::INSTALL_COMMAND }
}

/// Ergebnis einer Prüfung: blockierender Fehler (mit „fehlendes Binary“) oder Warnung, nie beides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub error: Option<String>,
    pub missing_binary: bool,
    pub warning: Option<String>,
}

impl Health {
    fn failed(message: String) -> Health {
        Health { error: Some(message), ..Health::default() }
    }
}

fn dotted((major, minor, patch): (u32, u32, u32)) -> String {
    format!("{major}.{minor}.{patch}")
}

/// Meldungen der Versionsprüfung, wie `checkVersion` in Swift.
pub fn of_version_check(check: &VersionCheck) -> Health {
    match check {
        VersionCheck::Ok => Health::default(),
        VersionCheck::RunFailed { binary, first_line } => Health::failed(trd!("%@: claude --version fehlgeschlagen: %@", binary.as_str(), first_line.as_str())),
        VersionCheck::Unparseable { binary, output } => {
            Health::failed(trd!("%@: claude --version liefert kein erkennbares Versionsformat: %@", binary.as_str(), output.as_str()))
        }
        VersionCheck::TooOld { found, tested } => {
            let (found, tested) = (dotted(*found), dotted(*tested));
            Health { warning: Some(trd!("claude %@: älter als die von Kadrell getestete Version %@, bitte mit „%@“ aktualisieren", found.as_str(), tested.as_str(), UPDATE_COMMAND)), ..Health::default() }
        }
    }
}

/// Das Ergebnis, wenn `claude` nicht auffindbar ist.
pub fn missing(binary: &str) -> Health {
    Health { error: Some(trd!("%@: claude nicht gefunden", binary)), missing_binary: true, warning: None }
}

/// Gibt es das Binary? Ein Pfad (mit Ordnertrennern) muss eine Datei sein, ein blanker Name wird im PATH gesucht
/// (Windows mit PATHEXT).
pub fn binary_exists(binary: &str, env: &std::collections::BTreeMap<String, String>) -> bool {
    let is_file = |p: &str| std::path::Path::new(p).is_file();
    if binary.contains('/') || binary.contains('\\') {
        return is_file(binary) || path_exts(env).iter().any(|e| is_file(&format!("{binary}{e}")));
    }
    let path = env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")).map(|(_, v)| v.as_str()).unwrap_or("");
    find_in_path(binary, path, &path_exts(env), &is_file).is_some()
}

/// Die Prüfung selbst: erst die Datei, dann `claude --version`. Blockiert (Prozessaufruf), nur auf einem
/// Hintergrund-Thread aufrufen.
pub fn check(cli: Option<&ClaudeCli>, name: &str) -> Health {
    let Some(cli) = cli else { return missing(name) };
    if !binary_exists(&cli.binary, &cli.environment) {
        return missing(&cli.binary);
    }
    of_version_check(&cli.check_version(&SystemRunner))
}

/// Alles, was die Prüfung auf dem Hintergrund-Thread braucht und was sie zurückgibt: die neu gesuchte Umgebung
/// (nach einer Installation steht `claude` an einem neuen Ort) und das Ergebnis.
pub struct Rechecked {
    pub env: LaunchEnv,
    pub health: Health,
}

/// Sucht `claude` neu und prüft es. `override_binary` ist die Einstellung `claudeBinary` (leer: suchen).
pub fn recheck(previous: &LaunchEnv, override_binary: &str) -> Rechecked {
    let env = LaunchEnv::from_process(override_binary, previous.exe.clone(), previous.socket.clone(), previous.home.clone());
    let name = if override_binary.is_empty() { "claude" } else { override_binary };
    let health = check(env.claude.as_ref(), name);
    Rechecked { env, health }
}

impl AppState {
    /// Trägt das Ergebnis ein: Fehler ins Register (kein Poll räumt ihn ab), Warnung für die Leiste. Gibt `true`
    /// zurück, wenn sich etwas geändert hat (Oberfläche neu zeichnen).
    pub fn apply_cli_health(&mut self, health: Health) -> bool {
        let before = (self.registry_error(), self.version_warning.clone());
        let mut registry = self.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        match &health.error {
            Some(message) => registry.fail(message, health.missing_binary),
            None => {
                registry.clear_error();
            }
        }
        drop(registry);
        self.version_warning = health.warning;
        before != (self.registry_error(), self.version_warning.clone())
    }

    /// Fehler des Registers (claude fehlt, `agents` schlägt fehl), den die Leiste und der Leerzustand zeigen.
    pub fn registry_error(&self) -> Option<String> {
        self.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner).last_error.clone()
    }

    /// Das Register meldet genau das fehlende Binary (Leerzustand bietet den Installationsbefehl).
    pub fn registry_error_is_missing_binary(&self) -> bool {
        self.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner).last_error_is_missing_binary
    }

    /// Ob schon ein Poll durchgelaufen ist (davor zeigt der Leerzustand „Lade Sessions …“).
    pub fn registry_polled(&self) -> bool {
        self.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner).polled
    }

    /// Meldung für die Leiste: der Fehler des Registers, sonst der letzte Startfehler einer Kachel.
    pub fn shown_error(&self) -> Option<String> {
        self.registry_error().or_else(|| self.error.clone())
    }

    /// `claude agents` schlägt fehl (Übernahme beim Start): als Fehler ins Register.
    pub fn fail_registry(&mut self, message: &str) {
        self.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner).fail(message, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binary_blocks_and_offers_the_install_command() {
        let h = missing("claude");
        assert_eq!(h.error.as_deref(), Some("claude: claude nicht gefunden"));
        assert!(h.missing_binary);
        assert_eq!(h.warning, None);
    }

    #[test]
    fn a_failed_version_call_blocks() {
        let h = of_version_check(&VersionCheck::RunFailed { binary: "/x/claude".into(), first_line: "boom".into() });
        assert_eq!(h.error.as_deref(), Some("/x/claude: claude --version fehlgeschlagen: boom"));
        assert!(!h.missing_binary);
    }

    #[test]
    fn an_unrecognisable_version_blocks() {
        let h = of_version_check(&VersionCheck::Unparseable { binary: "claude".into(), output: "hallo".into() });
        assert_eq!(h.error.as_deref(), Some("claude: claude --version liefert kein erkennbares Versionsformat: hallo"));
    }

    #[test]
    fn too_old_only_warns() {
        let h = of_version_check(&VersionCheck::TooOld { found: (2, 1, 1), tested: (2, 1, 273) });
        assert_eq!(h.error, None);
        assert_eq!(h.warning.as_deref(), Some("claude 2.1.1: älter als die von Kadrell getestete Version 2.1.273, bitte mit „claude update“ aktualisieren"));
    }

    #[test]
    fn ok_clears_everything() {
        assert_eq!(of_version_check(&VersionCheck::Ok), Health::default());
    }

    #[test]
    fn the_install_command_fits_the_system() {
        if cfg!(windows) {
            assert!(install_command().starts_with("irm "));
        } else {
            assert!(install_command().starts_with("curl "));
        }
    }

    #[test]
    fn a_bare_name_is_searched_in_path_and_a_path_must_be_a_file() {
        let dir = std::env::temp_dir().join(format!("kadrell-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("claude");
        std::fs::write(&file, "x").unwrap();
        let mut env = std::collections::BTreeMap::new();
        env.insert("PATH".to_string(), dir.to_string_lossy().into_owned());
        assert!(binary_exists("claude", &env));
        assert!(binary_exists(&file.to_string_lossy(), &env));
        assert!(!binary_exists(&dir.join("fehlt").to_string_lossy(), &env));
        assert!(!binary_exists("gibt-es-nicht-kadrell", &env));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
