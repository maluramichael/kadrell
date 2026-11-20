//! Eigene Hooks des Nutzers: ausführbare Skripte unter `~/.config/kadrell/hooks/<ereignis>`, wie tmux `set-hook`
//! (`Services/Hooks.swift`). Aufruf ohne Warten: `$1` Ordner, `$2` sessionId von Claude Code, `$3` Titel, dazu
//! `KADRELL_*` in der Umgebung. Fehlt das Skript, passiert nichts.
//!
//! Unix: nur Dateien des eigenen Benutzers, die weder Gruppe noch andere schreiben dürfen (wie sshd bei
//! `authorized_keys`), das gilt auch für den Ordner. Windows kennt diese Rechte nicht; dort liegt der Ordner im
//! Benutzerprofil, und ein Skript zählt als ausführbar, wenn es `<ereignis>.exe`, `.cmd` oder `.bat` heißt.

use kadrell_core::Session;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    SessionNew,
    SessionFocus,
    SessionRemove,
}

impl HookEvent {
    /// Dateiname des Skripts und Wert von `KADRELL_EVENT`.
    pub fn as_str(self) -> &'static str {
        match self {
            HookEvent::SessionNew => "session-new",
            HookEvent::SessionFocus => "session-focus",
            HookEvent::SessionRemove => "session-remove",
        }
    }
}

/// Was `fire` getan hat, für Protokoll und Tests.
#[derive(Debug, PartialEq, Eq)]
pub enum Fired {
    /// Kein (ausführbares) Skript für das Ereignis.
    Missing,
    /// Das Skript oder der Ordner gehört nicht dem Benutzer oder ist für andere beschreibbar.
    Untrusted(PathBuf),
    Started(PathBuf),
    /// Der Start scheiterte, mit der Meldung des Systems.
    Failed(PathBuf, String),
}

/// `~/.config/kadrell/hooks` (auch unter Windows, wie die Mac-App es dokumentiert).
pub fn hooks_dir(home: &Path) -> PathBuf {
    home.join(".config").join("kadrell").join("hooks")
}

/// Startet den Hook für `event`, ohne zu warten. `branch` ist der Branch des Ordners, den der Hook sieht (bei
/// aktivem Worktree dessen, siehe `Session::active_worktree`), `base_env` die Umgebung der Kacheln.
pub fn fire(dir: &Path, event: HookEvent, session: &Session, branch: Option<&str>, base_env: &[(String, String)]) -> Fired {
    let Some(path) = script_path(dir, event) else { return Fired::Missing };
    if !(trusted(&path) && trusted(dir)) {
        return Fired::Untrusted(path);
    }
    let cwd = session.active_worktree.clone().unwrap_or_else(|| session.cwd.clone());
    let mut cmd = Command::new(&path);
    cmd.args([cwd.as_str(), session.session_id.as_str(), session.title().as_str()]);
    cmd.envs(base_env.iter().map(|(k, v)| (k, v)));
    cmd.env("KADRELL_EVENT", event.as_str())
        .env("KADRELL_CWD", &cwd)
        .env("KADRELL_SESSION_ID", &session.session_id)
        .env("KADRELL_SESSION_KEY", &session.id)
        .env("KADRELL_TITLE", session.title())
        .env("KADRELL_BRANCH", branch.unwrap_or(""));
    if Path::new(&cwd).is_dir() {
        cmd.current_dir(&cwd);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Hooks laufen still im Hintergrund, ohne das Flag öffnet unter Windows jeder ein Konsolenfenster.
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
    match cmd.spawn() {
        Ok(mut child) => {
            // Abräumen, sonst bleibt der beendete Hook als Zombie stehen.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Fired::Started(path)
        }
        Err(e) => Fired::Failed(path, e.to_string()),
    }
}

#[cfg(unix)]
fn script_path(dir: &Path, event: HookEvent) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(event.as_str());
    let meta = std::fs::metadata(&path).ok()?;
    (meta.is_file() && meta.permissions().mode() & 0o111 != 0).then_some(path)
}

#[cfg(not(unix))]
fn script_path(dir: &Path, event: HookEvent) -> Option<PathBuf> {
    ["exe", "cmd", "bat"].iter().map(|ext| dir.join(format!("{}.{ext}", event.as_str()))).find(|p| p.is_file())
}

/// Wie sshd bei `authorized_keys`: nur Dateien des eigenen Benutzers, die weder Gruppe noch andere schreiben dürfen.
#[cfg(unix)]
pub fn trusted(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: `geteuid` hat keine Voraussetzungen.
    let me = unsafe { libc::geteuid() };
    std::fs::metadata(path).is_ok_and(|m| m.uid() == me && m.mode() & 0o022 == 0)
}

/// Windows: es gibt keine Gruppen- und Weltrechte wie unter Unix, die Datei muss nur existieren.
#[cfg(not(unix))]
pub fn trusted(path: &Path) -> bool {
    path.exists()
}

#[cfg(test)]
mod tests {
    /// Wartet, bis das Skript seine Ausgabe vollständig geschrieben hat (Datei da und mit Zeilenende), höchstens 30 s.
    /// Nur auf die Datei zu warten reicht nicht: sie entsteht, bevor der Inhalt drin ist.
    #[allow(dead_code)]
    fn wait_for_output(out: &std::path::Path) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            if std::fs::read_to_string(out).is_ok_and(|t| t.ends_with('\n')) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    use super::*;
    use crate::testing::TempDir;

    fn session() -> Session {
        let mut s = Session::new("key-1", "/nonexistent-dir", 0.0, "sid-1", "Titel");
        s.custom_name = Some("Mein Titel".into());
        s
    }

    #[test]
    fn event_names() {
        assert_eq!(HookEvent::SessionNew.as_str(), "session-new");
        assert_eq!(HookEvent::SessionFocus.as_str(), "session-focus");
        assert_eq!(HookEvent::SessionRemove.as_str(), "session-remove");
        assert_eq!(hooks_dir(Path::new("/h")), Path::new("/h").join(".config").join("kadrell").join("hooks"));
    }

    #[test]
    fn missing_hook_is_a_no_op() {
        let dir = TempDir::new();
        assert_eq!(fire(dir.path(), HookEvent::SessionNew, &session(), None, &[]), Fired::Missing);
        assert_eq!(fire(&dir.path().join("gibt-es-nicht"), HookEvent::SessionNew, &session(), None, &[]), Fired::Missing);
    }

    #[cfg(unix)]
    #[test]
    fn trust_rules() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let path = dir.path().join("hook");
        std::fs::write(&path, "#!/bin/sh\n").unwrap();
        let chmod = |mode| std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        chmod(0o755);
        assert!(trusted(&path));
        chmod(0o775);
        assert!(!trusted(&path), "Gruppe darf schreiben");
        chmod(0o757);
        assert!(!trusted(&path), "andere dürfen schreiben");
        assert!(!trusted(Path::new("/nonexistent/hook")));
        // SAFETY: `geteuid` hat keine Voraussetzungen.
        if unsafe { libc::geteuid() } != 0 {
            assert!(!trusted(Path::new("/bin/sh")), "gehört root");
        }
    }

    #[cfg(unix)]
    #[test]
    fn fires_with_arguments_and_environment() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let out = dir.path().join("out.txt");
        let script = dir.path().join("session-new");
        std::fs::write(&script, format!("#!/bin/sh\necho \"$1|$2|$3|$KADRELL_EVENT|$KADRELL_SESSION_KEY|$KADRELL_BRANCH|$EXTRA\" > '{}'\n", out.display())).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let fired = fire(dir.path(), HookEvent::SessionNew, &session(), Some("main"), &[("EXTRA".into(), "x".into())]);
        assert_eq!(fired, Fired::Started(script.clone()));
        wait_for_output(&out);
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "/nonexistent-dir|sid-1|Mein Titel|session-new|key-1|main|x\n");
        // Eine für andere beschreibbare Datei wird nicht gestartet.
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o757)).unwrap();
        assert_eq!(fire(dir.path(), HookEvent::SessionNew, &session(), None, &[]), Fired::Untrusted(dir.path().join("session-new")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_finds_cmd_scripts() {
        let dir = TempDir::new();
        let out = dir.path().join("out.txt");
        std::fs::write(dir.path().join("session-new.cmd"), format!("@echo off\r\necho %1;%2;%KADRELL_EVENT%;%KADRELL_BRANCH%> \"{}\"\r\n", out.display())).unwrap();
        let fired = fire(dir.path(), HookEvent::SessionNew, &session(), Some("dev"), &[]);
        assert_eq!(fired, Fired::Started(dir.path().join("session-new.cmd")));
        wait_for_output(&out);
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains("sid-1") && text.contains("session-new;dev"), "{text}");
    }
}
