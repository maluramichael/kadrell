//! Profile wie auf dem Mac (`App/Profile.swift`): ohne Namen das Standardprofil, `--profile <name>` unter
//! `profiles/<name>`, `--profile tmp` ein Wegwerfprofil in `<Temp>/kadrell-rust-tmp-<pid>`, das beim Beenden verschwindet
//! und die Einstellungen des Standardprofils übernimmt. Mehrere Profile laufen parallel, dasselbe Profil nur
//! einmal (Lock-Datei).
//!
//! Ob ein Prozess noch lebt, hängt vom Betriebssystem ab und kommt vom Aufrufer (`is_alive`, `is_instance`).

use crate::paths::{Paths, TEMP_PREFIX};
use crate::settings::Settings;
use crate::store::Store;
use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Seek, Write};
use std::path::{Path, PathBuf};

/// Name des Wegwerfprofils.
pub const TEMP_NAME: &str = "tmp";
/// Umgebungsvariable, die das Profil wählt, wenn kein `--profile` angegeben ist.
pub const PROFILE_ENV: &str = "KADRELL_PROFILE";

const SETTINGS_FILE: &str = "settings.json";
const LOCK_FILE: &str = "kadrell.lock";
/// Zweite Datei mit der pid: unter Windows sperrt die Lock-Datei auch das Lesen durch andere Prozesse.
const PID_FILE: &str = "kadrell.pid";
const QUIT_MARKER: &str = "quit.marker";

/// So lange nach einem Beenden durch den Nutzer startet kein CLI-Aufruf die App von selbst wieder (Sekunden).
pub const QUIT_MARKER_WINDOW: f64 = 600.0;

/// Ist ein Prozess mit dieser pid noch da? Vom Aufrufer, weil das vom Betriebssystem abhängt.
pub type PidAlive<'a> = &'a dyn Fn(u32) -> bool;

/// Profil dieser Instanz: eigener Ordner für Sessions, Gruppen, Einstellungen und Steuerkanal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// `None` = Standardprofil.
    pub name: Option<String>,
    pub dir: PathBuf,
    /// Wegwerfprofil (`tmp`): verschwindet beim Beenden.
    pub temporary: bool,
    /// pid der Instanz, deren Ordner das ist (bei `tmp` Teil des Ordnernamens).
    pub pid: u32,
}

/// Ergebnis von [`Profile::acquire`].
#[derive(Debug)]
pub enum Acquire {
    /// Das Profil gehört jetzt dieser Instanz, solange der Wert lebt.
    Locked(InstanceLock),
    /// Eine echte zweite Kadrell-Instanz hält das Profil, mit ihrer pid.
    Running(u32),
    /// Ein Fremdprozess hält die Lock-Datei (Backup, Cloud-Sync, Spotlight) oder sie zeigt auf eine tote pid
    /// (abgestürzte Instanz): nicht blockieren, diese Instanz startet trotzdem.
    Unprotected,
}

/// Hält die Sperre auf dem Profil. Lässt man den Wert fallen oder endet der Prozess, ist das Profil frei.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl Profile {
    /// `--profile <name>` aus den Argumenten, sonst `KADRELL_PROFILE` aus `env_profile`. Ungültige Namen gelten
    /// als nicht angegeben.
    pub fn name_from_args(args: &[String], env_profile: Option<&str>) -> Option<String> {
        if let Some(value) = args.iter().position(|a| a == "--profile").and_then(|i| args.get(i + 1)) {
            return clean_name(value);
        }
        env_profile.and_then(clean_name)
    }

    /// Wie [`Profile::name_from_args`] mit den echten Argumenten und der echten Umgebung.
    pub fn name_from_process() -> Option<String> {
        let args: Vec<String> = std::env::args().collect();
        Profile::name_from_args(&args, std::env::var(PROFILE_ENV).ok().as_deref())
    }

    /// Legt den Profilordner an. Ein Wegwerfprofil räumt zuerst die Reste abgestürzter Wegwerfprofile weg (siehe
    /// [`sweep_stale`]) und übernimmt die Einstellungen des Standardprofils ohne Auswahl und Lesestand.
    pub fn open(name: Option<&str>, paths: &Paths, pid: u32, is_alive: PidAlive) -> std::io::Result<Profile> {
        let (dir, temporary) = match name {
            None => (paths.default_profile_dir(), false),
            Some(TEMP_NAME) => (paths.temp_profile_dir(pid), true),
            Some(n) => (paths.named_profile_dir(n), false),
        };
        if temporary {
            sweep_stale(paths, pid, is_alive);
        }
        std::fs::create_dir_all(&dir)?;
        let profile = Profile { name: name.map(str::to_string), dir, temporary, pid };
        if temporary {
            Settings::load(&paths.default_profile_dir().join(SETTINGS_FILE)).for_temp_profile().save(&profile.settings_path())?;
        }
        Ok(profile)
    }

    pub fn settings_path(&self) -> PathBuf {
        self.dir.join(SETTINGS_FILE)
    }

    pub fn sessions_path(&self) -> PathBuf {
        self.dir.join("sessions.json")
    }

    pub fn groups_path(&self) -> PathBuf {
        self.dir.join("groups.json")
    }

    /// Einstellungen des Profils (leer, wenn es noch keine Datei gibt).
    pub fn settings(&self) -> Settings {
        Settings::load(&self.settings_path())
    }

    pub fn save_settings(&self, settings: &Settings) -> std::io::Result<()> {
        settings.save(&self.settings_path())
    }

    /// Sessions und Gruppen des Profils.
    pub fn open_store(&self, home: &str) -> Store {
        Store::open(&self.dir, home)
    }

    /// Titelzusatz fürs Fenster, leer im Standardprofil.
    pub fn label(&self) -> String {
        self.name.as_ref().map(|n| format!(" · {n}")).unwrap_or_default()
    }

    /// Sperrt das Profil für diese Instanz. `is_instance(pid)` sagt, ob hinter der pid in der Lock-Datei eine
    /// laufende Kadrell-Instanz steckt; nur dann kommt [`Acquire::Running`] zurück. Alles andere blockiert nicht,
    /// sonst macht ein beliebiger Prozess, der `kadrell.lock` offen hält, die App unstartbar.
    pub fn acquire(&self, is_instance: PidAlive) -> Acquire {
        let path = self.dir.join(LOCK_FILE);
        let Ok(file) = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(&path) else { return Acquire::Unprotected };
        match file.try_lock() {
            Ok(()) => {
                self.write_pid(&file);
                Acquire::Locked(InstanceLock { _file: file })
            }
            Err(TryLockError::WouldBlock) => match self.holder_pid().filter(|p| *p != self.pid && is_instance(*p)) {
                Some(pid) => Acquire::Running(pid),
                None => Acquire::Unprotected,
            },
            Err(TryLockError::Error(_)) => Acquire::Unprotected,
        }
    }

    fn write_pid(&self, file: &File) {
        let text = format!("{}\n", self.pid);
        let mut f = file;
        let _ = file.set_len(0);
        let _ = f.rewind();
        let _ = f.write_all(text.as_bytes());
        let _ = std::fs::write(self.dir.join(PID_FILE), &text);
    }

    /// pid, die in die Lock-Datei (oder die Zweitdatei) geschrieben wurde.
    fn holder_pid(&self) -> Option<u32> {
        [PID_FILE, LOCK_FILE].iter().find_map(|name| std::fs::read_to_string(self.dir.join(name)).ok()?.trim().parse::<u32>().ok().filter(|p| *p > 0))
    }

    /// Beim Beenden: ein Wegwerfprofil verschwindet ganz (Ordner samt Einstellungen), sonst bleibt alles liegen
    /// bis auf die pid-Datei.
    pub fn clean_up(&self) {
        if self.temporary {
            let _ = std::fs::remove_dir_all(&self.dir);
        } else {
            let _ = std::fs::remove_file(self.dir.join(PID_FILE));
        }
    }
}

/// Erlaubt sind `A-Z a-z 0-9 . _ -`, Leerzeichen an den Enden fallen weg, leer gilt als nicht angegeben.
pub fn clean_name(s: &str) -> Option<String> {
    let n = s.trim_matches([' ', '\t']);
    let valid = !n.is_empty() && n.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    valid.then(|| n.to_string())
}

/// Reste von Wegwerfprofilen, deren Prozess nicht mehr läuft (Absturz, Testläufe). Läuft bei jedem Start. Nur
/// Ordner `kadrell-rust-tmp-<pid>` mit lesbarer pid, nie der eigene und nie einer mit lebendem Prozess.
pub fn sweep_stale(paths: &Paths, own_pid: u32, is_alive: PidAlive) {
    let Ok(entries) = std::fs::read_dir(&paths.temp) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let pid = name.strip_prefix(TEMP_PREFIX).and_then(|p| p.parse::<u32>().ok());
        if let Some(pid) = pid.filter(|p| *p != own_pid && !is_alive(*p)) {
            let _ = std::fs::remove_dir_all(paths.temp_profile_dir(pid));
        }
    }
}

/// pid aus der pid-Datei im Profilordner `dir` (von der Instanz beim Start geschrieben, beim Beenden entfernt).
pub fn recorded_pid(dir: &Path) -> Option<u32> {
    std::fs::read_to_string(dir.join(PID_FILE)).ok()?.trim().parse::<u32>().ok().filter(|p| *p > 0)
}

// MARK: Beenden-Marker

fn quit_marker(dir: &Path) -> PathBuf {
    dir.join(QUIT_MARKER)
}

/// Beim Beenden (nicht beim Neustart) setzen, bevor der Steuerkanal verschwindet: Skripte, Statusleisten oder
/// Claude-Sessions rufen die CLI auch im Moment des Beendens auf, und die startet die App sonst sofort wieder.
/// `now` in Sekunden seit 1970.
pub fn mark_quit(dir: &Path, now: f64) {
    let _ = std::fs::write(quit_marker(dir), now.to_string());
}

/// Jeder Start räumt den Marker weg: wer die App selbst öffnet, will sie laufen haben.
pub fn clear_quit_marker(dir: &Path) {
    let _ = std::fs::remove_file(quit_marker(dir));
}

/// Wurde die App vor Kurzem vom Nutzer beendet? Dann soll die CLI sie nicht neu starten.
pub fn quit_recently(dir: &Path, now: f64) -> bool {
    let Some(t) = std::fs::read_to_string(quit_marker(dir)).ok().and_then(|s| s.trim().parse::<f64>().ok()) else { return false };
    let age = now - t;
    (0.0..QUIT_MARKER_WINDOW).contains(&age)
}

/// Shell-Befehl (macOS), der wartet, bis `pid` weg ist (dann ist der Profil-Lock frei), und danach denselben Build
/// mit demselben Profil öffnet. Kein `-n`: die alte Instanz ist dann beendet, `open` startet frisch.
pub fn relaunch_command(pid: u32, bundle_path: &str, profile: Option<&str>) -> Vec<String> {
    let quoted = format!("'{}'", bundle_path.replace('\'', "'\\''"));
    let profile_args = profile.map(|p| format!(" --args --profile {p}")).unwrap_or_default();
    vec!["/bin/sh".into(), "-c".into(), format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; exec /usr/bin/open {quoted}{profile_args}")]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testutil::TempDir;

    fn paths(d: &TempDir) -> Paths {
        Paths::new(d.path().join("home"), d.path().join("support"), d.path().join("tmp"))
    }

    fn setup(d: &TempDir) -> Paths {
        let p = paths(d);
        std::fs::create_dir_all(&p.app_support).unwrap();
        std::fs::create_dir_all(&p.temp).unwrap();
        p
    }

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn profile_name_from_args_and_env() {
        assert_eq!(Profile::name_from_args(&args(&["kadrell", "--profile", "work"]), None), Some("work".into()));
        assert_eq!(Profile::name_from_args(&args(&["kadrell"]), Some("env")), Some("env".into()));
        assert_eq!(Profile::name_from_args(&args(&["kadrell", "--profile", "a"]), Some("env")), Some("a".into()), "Argument vor Umgebung");
        assert_eq!(Profile::name_from_args(&args(&["kadrell", "--profile"]), None), None);
        assert_eq!(Profile::name_from_args(&args(&["kadrell", "--profile", "../x"]), Some("env")), None, "ungültiger Name gilt als nicht angegeben");
        assert_eq!(clean_name("  a.b_c-1 "), Some("a.b_c-1".into()));
        assert_eq!(clean_name("a b"), None);
        assert_eq!(clean_name(""), None);
        assert_eq!(clean_name("ä"), None);
    }

    #[test]
    fn default_named_and_temp_dirs() {
        let d = TempDir::new();
        let p = setup(&d);
        let alive = |_: u32| true;
        let default = Profile::open(None, &p, 7, &alive).unwrap();
        assert_eq!((default.dir.clone(), default.temporary, default.label()), (p.app_support.clone(), false, String::new()));
        let named = Profile::open(Some("work"), &p, 7, &alive).unwrap();
        assert_eq!(named.dir, p.app_support.join("profiles").join("work"));
        assert!(named.dir.is_dir());
        assert_eq!(named.label(), " · work");
        let tmp = Profile::open(Some("tmp"), &p, 7, &alive).unwrap();
        assert!(tmp.temporary);
        assert_eq!(tmp.dir, p.temp.join("kadrell-rust-tmp-7"));
        assert!(tmp.dir.is_dir());
    }

    #[test]
    fn temp_profile_copies_default_settings_without_selection() {
        let d = TempDir::new();
        let p = setup(&d);
        let mut base = Settings::default();
        base.set_claude_model("opus");
        base.set("workspace.selected", serde_json::json!("k1"));
        base.set("folders.uses", serde_json::json!({"/p": 3}));
        base.save(&p.default_profile_dir().join("settings.json")).unwrap();
        let tmp = Profile::open(Some("tmp"), &p, 11, &|_| true).unwrap();
        let copied = tmp.settings();
        assert_eq!(copied.claude_model(), "opus");
        assert!(!copied.contains("workspace.selected") && !copied.contains("folders.uses"));
        let mut changed = tmp.settings();
        changed.set_claude_model("haiku");
        tmp.save_settings(&changed).unwrap();
        assert_eq!(Settings::load(&p.default_profile_dir().join("settings.json")).claude_model(), "opus", "Standardprofil bleibt unberührt");
        tmp.clean_up();
        assert!(!tmp.dir.exists(), "das Wegwerfprofil verschwindet samt Einstellungen");
    }

    #[test]
    fn sweep_removes_only_dead_foreign_temp_profiles() {
        let d = TempDir::new();
        let p = setup(&d);
        for name in ["kadrell-rust-tmp-100", "kadrell-rust-tmp-200", "kadrell-rust-tmp-300", "kadrell-rust-tmp-x", "andere"] {
            std::fs::create_dir_all(p.temp.join(name)).unwrap();
        }
        // 100 tot, 200 lebt, 300 ist die eigene Instanz.
        sweep_stale(&p, 300, &|pid| pid == 200);
        assert!(!p.temp.join("kadrell-rust-tmp-100").exists());
        assert!(p.temp.join("kadrell-rust-tmp-200").exists());
        assert!(p.temp.join("kadrell-rust-tmp-300").exists());
        assert!(p.temp.join("kadrell-rust-tmp-x").exists(), "unlesbare pid bleibt");
        assert!(p.temp.join("andere").exists());
    }

    #[test]
    fn opening_a_temp_profile_sweeps_stale_ones() {
        let d = TempDir::new();
        let p = setup(&d);
        std::fs::create_dir_all(p.temp.join("kadrell-rust-tmp-5")).unwrap();
        Profile::open(Some("tmp"), &p, 6, &|_| false).unwrap();
        assert!(!p.temp.join("kadrell-rust-tmp-5").exists());
        assert!(p.temp.join("kadrell-rust-tmp-6").exists());
    }

    #[test]
    fn lock_is_single_instance_per_profile() {
        let d = TempDir::new();
        let p = setup(&d);
        let first = Profile::open(Some("a"), &p, 1000, &|_| true).unwrap();
        let second = Profile { pid: 2000, ..first.clone() };
        let lock = match first.acquire(&|_| true) {
            Acquire::Locked(l) => l,
            other => panic!("{other:?}"),
        };
        assert!(matches!(second.acquire(&|pid| pid == 1000), Acquire::Running(1000)), "echte zweite Instanz");
        assert!(matches!(second.acquire(&|_| false), Acquire::Unprotected), "Fremdprozess oder tote pid blockiert nicht");
        assert_eq!(first.holder_pid(), Some(1000));
        drop(lock);
        assert!(matches!(second.acquire(&|_| true), Acquire::Locked(_)), "nach dem Loslassen ist das Profil frei");
        assert_eq!(second.holder_pid(), Some(2000));
    }

    #[test]
    fn different_profiles_run_in_parallel() {
        let d = TempDir::new();
        let p = setup(&d);
        let a = Profile::open(Some("a"), &p, 1, &|_| true).unwrap();
        let b = Profile::open(Some("b"), &p, 2, &|_| true).unwrap();
        let (la, lb) = (a.acquire(&|_| true), b.acquire(&|_| true));
        assert!(matches!((&la, &lb), (Acquire::Locked(_), Acquire::Locked(_))));
    }

    #[test]
    fn relaunch_command_waits_then_opens_same_bundle() {
        let c = relaunch_command(4242, "/Applications/Kadrell.app", Some("work"));
        assert_eq!(c[0], "/bin/sh");
        assert_eq!(c[1], "-c");
        assert!(c[2].contains("kill -0 4242"), "{}", c[2]);
        assert!(c[2].contains("/usr/bin/open '/Applications/Kadrell.app' --args --profile work"), "{}", c[2]);
    }

    /// Standardprofil: kein `--profile`-Argument. Pfad mit Leerzeichen bleibt in Anführungszeichen.
    #[test]
    fn relaunch_command_default_profile_and_spaces() {
        let c = relaunch_command(1, "/My Apps/Kadrell.app", None);
        assert!(c[2].contains("open '/My Apps/Kadrell.app'"), "{}", c[2]);
        assert!(!c[2].contains("--profile"), "{}", c[2]);
        assert!(relaunch_command(1, "/it's/K.app", None)[2].contains(r"'/it'\''s/K.app'"));
    }

    /// Nach dem Beenden startet die CLI die App zehn Minuten nicht neu; ein Neustart der App räumt den Marker weg.
    #[test]
    fn quit_marker_blocks_only_for_the_window_and_clears() {
        let d = TempDir::new();
        let t = 1_789_479_734.25;
        assert!(!quit_recently(d.path(), t));
        mark_quit(d.path(), t);
        assert!(quit_recently(d.path(), t + 5.0));
        assert!(!quit_recently(d.path(), t + QUIT_MARKER_WINDOW + 1.0));
        assert!(!quit_recently(d.path(), t - 1.0), "Marker aus der Zukunft zählt nicht");
        clear_quit_marker(d.path());
        assert!(!quit_recently(d.path(), t + 5.0));
    }

    #[test]
    fn store_opens_inside_the_profile_dir() {
        let d = TempDir::new();
        let p = setup(&d);
        let profile = Profile::open(Some("w"), &p, 1, &|_| true).unwrap();
        let mut store = profile.open_store("/h");
        store.add_session(crate::session::Session::new("a", "/p/x", 1.0, "a", ""), None);
        assert!(profile.sessions_path().exists() && profile.groups_path().exists());
    }
}
