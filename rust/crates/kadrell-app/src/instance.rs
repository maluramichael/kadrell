//! Instanzen und Kommandozeile (Swift: `Profile.launchTemporary`, `Profile.relaunchAfterExit`, `menuInstallCLI`):
//! eine weitere Instanz mit Wegwerfprofil, der Helfer für den Neustart und der Link `~/.local/bin/kadrell` auf das
//! Binary dieser App. Der Mac öffnet das Bündel mit `open`, Windows und Linux starten das Binary selbst; die Befehle
//! baut `kadrell_core::profile` (`relaunch_argv`, `temporary_argv`).

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gpui::{Context, Window};
use kadrell_i18n::trd;

use crate::Shell;

/// Pfad dieses Binaries als Text.
fn current_exe() -> Option<String> {
    std::env::current_exe().ok().map(|p| p.to_string_lossy().into_owned())
}

/// Startet `argv` losgelöst vom Elternprozess: es überlebt das Ende der App (Windows: ohne Konsole, eigene
/// Prozessgruppe; Unix: eigene Sitzung wäre nötig, `sh`/`open` werden von launchd bzw. init übernommen).
fn spawn_detached(argv: &[String]) -> io::Result<()> {
    let (program, args) = argv.split_first().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "leerer Befehl"))?;
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000 | 0x0000_0200); // CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP
    }
    cmd.spawn().map(|_| ())
}

/// Ganz am Ende des Beendens: der Helfer wartet auf das Ende dieses Prozesses und startet dann denselben Build mit
/// demselben Profil neu.
pub fn relaunch_after_exit(profile: Option<&str>) {
    let Some(exe) = current_exe() else { return };
    let argv = kadrell_core::profile::relaunch_argv(std::process::id(), &exe, profile);
    if let Err(e) = spawn_detached(&argv) {
        eprintln!("kadrell: Neustart nicht möglich: {e}");
    }
}

/// „Neue Instanz mit temporärem Profil“: derselbe Build, frisches Profil, beim Beenden weg.
pub fn launch_temporary() -> Result<(), String> {
    let exe = current_exe().ok_or_else(|| trd!("Pfad der App unbekannt"))?;
    spawn_detached(&kadrell_core::profile::temporary_argv(&exe)).map_err(|e| e.to_string())
}

// MARK: Kommandozeilen-Tool

/// Was am Ort des Links liegt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Existing {
    Nothing,
    /// Ein Symlink mit seinem Ziel.
    Symlink(PathBuf),
    /// Eine echte Datei (ein fremdes Programm unter dem Namen): bleibt unangetastet.
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Der Link zeigt schon auf diese App.
    Done,
    /// Link neu anlegen, falls `remove_first` zuvor den alten (anderes Ziel) entfernen.
    Create { remove_first: bool },
    Refuse,
}

/// Entscheidung ohne Nebenwirkung: ein Symlink mit anderem Ziel wird ersetzt, eine echte Datei nie überschrieben.
pub fn decide(existing: &Existing, target: &Path) -> Step {
    match existing {
        Existing::Nothing => Step::Create { remove_first: false },
        Existing::Symlink(to) if to == target => Step::Done,
        Existing::Symlink(_) => Step::Create { remove_first: true },
        Existing::File => Step::Refuse,
    }
}

/// Ort des Links: `~/.local/bin/kadrell` (Windows: `kadrell.exe`, wie die Installation der Terminal-App).
pub fn link_path(home: &Path) -> PathBuf {
    home.join(".local").join("bin").join(if cfg!(windows) { "kadrell.exe" } else { "kadrell" })
}

fn existing_at(link: &Path) -> Existing {
    match std::fs::read_link(link) {
        Ok(to) => Existing::Symlink(to),
        Err(_) if link.exists() || std::fs::symlink_metadata(link).is_ok() => Existing::File,
        Err(_) => Existing::Nothing,
    }
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn make_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

/// Legt den Link an (Swift: `menuInstallCLI`). Der Fehler ist der Text für den Dialog „Installation fehlgeschlagen“.
pub fn install_cli(link: &Path, target: &Path) -> Result<(), String> {
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    match decide(&existing_at(link), target) {
        Step::Done => Ok(()),
        Step::Refuse => Err(trd!("%@ existiert schon und ist kein Symlink", link.to_string_lossy().into_owned())),
        Step::Create { remove_first } => {
            if remove_first {
                std::fs::remove_file(link).map_err(|e| e.to_string())?;
            }
            make_symlink(target, link).map_err(|e| e.to_string())
        }
    }
}

/// Gibt es einen Editor und einen Ordner, den er öffnen kann?
pub fn can_open_editor(command: &str, cwd: &str) -> bool {
    !command.trim().is_empty() && !cwd.is_empty()
}

impl Shell {
    /// Menü „Kommandozeilen-Tool installieren …“: Link anlegen, das Ergebnis als Meldung.
    pub(crate) fn install_cli_action(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let home = PathBuf::from(self.state.home());
        let link = link_path(&home);
        let result = std::env::current_exe().map_err(|_| trd!("Pfad der App unbekannt")).and_then(|target| install_cli(&link, &target).map(|()| link.to_string_lossy().into_owned()));
        match result {
            Ok(path) => self.tell(trd!("Kommandozeilen-Tool installiert"), trd!("%@ zeigt auf diese App. `kadrell help` listet die Befehle. In Sessions dieser App steht der Pfad zusätzlich in $KADRELL.", path), trd!("OK"), window, cx),
            Err(message) => self.tell(trd!("Installation fehlgeschlagen"), message, trd!("OK"), window, cx),
        }
    }

    /// Fehlermeldung als Dialog, zusätzlich im Log (Swift: `sheets.report`).
    pub(crate) fn report_error(&mut self, detail: String, window: &mut Window, cx: &mut Context<Self>) {
        eprintln!("kadrell: {detail}");
        self.tell(trd!("Claude CLI meldet einen Fehler"), detail, trd!("OK"), window, cx);
    }

    /// ⌥E: Ordner der Fokus-Session im eingestellten Editor. Ohne Editor oder Ordner geschieht nichts (die Mac-App
    /// piept), ein Startfehler erscheint als Meldung (Swift: `openEditor`).
    pub(crate) fn open_editor_hotkey(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let command = self.state.settings.editor_command();
        let cwd = self.state.focused.as_deref().and_then(|k| self.state.session(k)).map(|s| s.cwd.clone()).unwrap_or_default();
        if !can_open_editor(&command, &cwd) {
            return;
        }
        if let Err(e) = crate::ui::groups::rows::spawn_editor(&command, &cwd) {
            self.report_error(e.to_string(), window, cx);
        }
    }

    /// Menü „Neue Instanz mit temporärem Profil“.
    pub(crate) fn temporary_instance_action(&mut self) {
        if let Err(message) = launch_temporary() {
            eprintln!("kadrell: temporäre Instanz nicht gestartet: {message}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kadrell-instance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_link_to_another_target_is_replaced_but_a_real_file_never() {
        let target = Path::new("/neu/kadrell");
        assert_eq!(decide(&Existing::Nothing, target), Step::Create { remove_first: false });
        assert_eq!(decide(&Existing::Symlink(target.into()), target), Step::Done);
        assert_eq!(decide(&Existing::Symlink("/alt/kadrell".into()), target), Step::Create { remove_first: true });
        assert_eq!(decide(&Existing::File, target), Step::Refuse);
    }

    #[test]
    fn the_editor_needs_a_command_and_a_folder() {
        assert!(can_open_editor("code", "C:/p"));
        assert!(!can_open_editor("  ", "C:/p"));
        assert!(!can_open_editor("code", ""));
    }

    #[test]
    fn the_link_lives_in_the_local_bin_of_the_home() {
        let link = link_path(Path::new("/home/m"));
        assert!(link.ends_with(if cfg!(windows) { ".local/bin/kadrell.exe" } else { ".local/bin/kadrell" }.replace('/', std::path::MAIN_SEPARATOR_STR)));
    }

    /// Eine fremde Datei unter dem Namen bleibt unangetastet, der Dialog nennt den Pfad.
    #[test]
    fn install_refuses_to_overwrite_a_real_file() {
        let dir = temp("refuse");
        let link = dir.join(".local").join("bin").join("kadrell");
        std::fs::create_dir_all(link.parent().unwrap()).unwrap();
        std::fs::write(&link, "fremdes Programm").unwrap();
        let error = install_cli(&link, Path::new("/x/kadrell")).unwrap_err();
        assert!(error.contains("existiert schon und ist kein Symlink") && error.contains("kadrell"), "{error}");
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "fremdes Programm");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Anlegen mit Zwischenordnern, Ersetzen bei anderem Ziel, Wiederholen ohne Fehler. Windows braucht für Symlinks
    /// den Entwicklermodus: ohne ihn endet der Test an der Fehlermeldung des Systems (kein Ausfall der Logik).
    #[test]
    fn install_creates_missing_folders_and_replaces_a_stale_link() {
        let dir = temp("link");
        let (a, b) = (dir.join("a.bin"), dir.join("b.bin"));
        std::fs::write(&a, "a").unwrap();
        std::fs::write(&b, "b").unwrap();
        let link = dir.join("neu").join("bin").join("kadrell");
        if let Err(e) = install_cli(&link, &a) {
            assert!(cfg!(windows), "{e}");
            std::fs::remove_dir_all(&dir).unwrap();
            return;
        }
        assert_eq!(std::fs::read_link(&link).unwrap(), a);
        install_cli(&link, &a).unwrap();
        install_cli(&link, &b).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), b, "anderes Ziel ersetzt den Link");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
