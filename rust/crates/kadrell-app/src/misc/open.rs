//! Klick auf einen Link im Terminal (Strg+Klick, Mac ⌘-Klick): URLs öffnet das System, Pfade ab dem Ordner der Session
//! (`kadrell_claude::link_target`). Text und Ordner gehen in den eingestellten Editor, Bilder, PDFs und andere
//! Binärdateien in die Standard-App (Swift: `openPath` im AppDelegate).

use std::path::Path;
use std::process::{Command, Stdio};

use kadrell_claude::link_target::{self, LinkTarget};

/// Was ein Klick tut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opening {
    /// Standard-Programm des Systems (Browser, Mailprogramm).
    Url(String),
    /// Datei oder Ordner in der Standard-App des Systems.
    System(String),
    /// Datei oder Ordner im eingestellten Editor: Kommando und Pfad.
    Editor { command: String, path: String },
}

/// Entscheidung ohne Nebenwirkung. `editor` ist die Einstellung „Editor“ (leer: keiner), `text_like` sagt, ob der Pfad
/// in den Editor gehört (`link_target::prefers_editor`).
pub fn decide(target: LinkTarget, editor: &str, text_like: impl Fn(&str, bool) -> bool) -> Opening {
    match target {
        LinkTarget::Url(url) => Opening::Url(url),
        LinkTarget::Path { path, is_directory } => {
            let command = editor.trim();
            if command.is_empty() || !text_like(&path, is_directory) {
                Opening::System(path)
            } else {
                Opening::Editor { command: command.to_string(), path }
            }
        }
    }
}

/// Link → Aktion: relativ zu `base` (Ordner der Session) aufgelöst. `None`: es gibt nichts zu öffnen.
pub fn plan(link: &str, base: &str, home: &str, editor: &str) -> Option<Opening> {
    let target = link_target::resolve(link, base, home)?;
    Some(decide(target, editor, link_target::prefers_editor))
}

/// Führt die Aktion aus. Der Fehler ist eine Zeile für die Anzeige.
pub fn run(opening: &Opening) -> Result<(), String> {
    match opening {
        Opening::Url(url) => open_with_system(url),
        Opening::System(path) => open_with_system(&native_path(path)),
        Opening::Editor { command, path } => open_with_editor(command, &native_path(path)),
    }
}

/// Windows kennt `/` in Pfaden, `explorer` und die meisten Editoren erwarten aber `\`.
fn native_path(path: &str) -> String {
    if cfg!(windows) { path.replace('/', "\\") } else { path.to_string() }
}

#[cfg(target_os = "windows")]
fn open_with_system(target: &str) -> Result<(), String> {
    // `explorer` öffnet URLs, Dateien und Ordner; ohne `cmd` gibt es kein Quoting-Problem bei `&` in URLs.
    spawn(Command::new("explorer.exe").arg(target))
}

#[cfg(target_os = "macos")]
fn open_with_system(target: &str) -> Result<(), String> {
    spawn(Command::new("open").arg(target))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_with_system(target: &str) -> Result<(), String> {
    spawn(Command::new("xdg-open").arg(target))
}

fn spawn(cmd: &mut Command) -> Result<(), String> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn open_with_editor(command: &str, path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").raw_arg(format!("{command} \"{path}\""));
    spawn(&mut cmd)
}

#[cfg(not(windows))]
fn open_with_editor(command: &str, path: &str) -> Result<(), String> {
    let quoted = format!("'{}'", path.replace('\'', "'\\''"));
    spawn(Command::new("/bin/sh").args(["-lc", &format!("{command} {quoted}")]))
}

/// Ordner, ab dem relative Pfade gelten: der Ordner der Session, sonst das Home-Verzeichnis.
pub fn base_for(cwd: &str, home: &str) -> String {
    if !cwd.is_empty() && Path::new(cwd).exists() { cwd.to_string() } else { home.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(_: &str, _: bool) -> bool {
        true
    }

    fn binary(_: &str, _: bool) -> bool {
        false
    }

    fn file(path: &str) -> LinkTarget {
        LinkTarget::Path { path: path.into(), is_directory: false }
    }

    #[test]
    fn urls_always_go_to_the_system() {
        assert_eq!(decide(LinkTarget::Url("https://x.y".into()), "code", text), Opening::Url("https://x.y".into()));
    }

    #[test]
    fn text_goes_to_the_editor_when_one_is_set() {
        assert_eq!(decide(file("/a/b.rs"), " code ", text), Opening::Editor { command: "code".into(), path: "/a/b.rs".into() });
    }

    #[test]
    fn binaries_and_missing_editor_use_the_default_app() {
        assert_eq!(decide(file("/a/b.png"), "code", binary), Opening::System("/a/b.png".into()));
        assert_eq!(decide(file("/a/b.rs"), "  ", text), Opening::System("/a/b.rs".into()));
    }

    #[test]
    fn plan_resolves_relative_to_the_session_folder_and_ignores_missing_files() {
        let dir = std::env::temp_dir().join(format!("kadrell-open-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "hallo").unwrap();
        let base = dir.to_string_lossy().replace('\\', "/");
        let opened = plan("notes.txt:3", &base, "/home", "code");
        assert!(matches!(&opened, Some(Opening::Editor { path, .. }) if path.ends_with("notes.txt")), "{opened:?}");
        assert_eq!(plan("fehlt.txt", &base, "/home", "code"), None);
        assert_eq!(plan("https://x.y/z", &base, "/home", ""), Some(Opening::Url("https://x.y/z".into())));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn base_falls_back_to_home() {
        assert_eq!(base_for("", "/home/x"), "/home/x");
        assert_eq!(base_for("/gibt/es/nicht/9f3a", "/home/x"), "/home/x");
    }
}
