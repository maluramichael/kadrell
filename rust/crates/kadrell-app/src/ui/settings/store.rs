//! Die Einstellungen der laufenden Instanz als GPUI-Global. Sie leben im Speicher; geschrieben wird nur im
//! Wegwerfprofil (`--profile tmp`), nie in das Standardprofil und nie in ein benanntes Profil. Wer die App mit
//! Profilen verdrahtet, ersetzt `AppSettings::load` durch das echte Profil.

use std::path::PathBuf;

use gpui::{App, Global};
use kadrell_core::profile::TEMP_NAME;
use kadrell_core::{Paths, Profile, Settings};

pub struct AppSettings {
    pub settings: Settings,
    /// Datei, in die jede Änderung geschrieben wird. `None`: nur im Speicher.
    pub path: Option<PathBuf>,
}

impl Global for AppSettings {}

impl AppSettings {
    /// Startzustand: im Wegwerfprofil dessen Datei (sonst die Einstellungen des Standardprofils ohne Auswahl),
    /// in allen anderen Fällen leer und nur im Speicher.
    pub fn load() -> AppSettings {
        let name = Profile::name_from_process();
        Self::for_profile(name.as_deref(), &Paths::from_env(), std::process::id())
    }

    pub fn for_profile(name: Option<&str>, paths: &Paths, pid: u32) -> AppSettings {
        if name != Some(TEMP_NAME) {
            return AppSettings { settings: Settings::default(), path: None };
        }
        let file = paths.temp_profile_dir(pid).join("settings.json");
        let settings = if file.exists() {
            Settings::load(&file)
        } else {
            Settings::load(&paths.default_profile_dir().join("settings.json")).for_temp_profile()
        };
        AppSettings { settings, path: Some(file) }
    }

    /// Schreibt die Datei, falls es eine gibt. Fehler gehen auf stderr, die Änderung gilt trotzdem.
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = self.settings.save(path) {
            eprintln!("Einstellungen nicht gespeichert: {e}");
        }
    }
}

/// Die Einstellungen aus dem Global; beim ersten Zugriff geladen.
pub fn ensure(cx: &mut App) {
    if !cx.has_global::<AppSettings>() {
        cx.set_global(AppSettings::load());
    }
}

/// Kopie der aktuellen Einstellungen.
pub fn snapshot(cx: &App) -> Settings {
    cx.global::<AppSettings>().settings.clone()
}

/// Ändert die Einstellungen und speichert sie.
pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    let store = cx.global_mut::<AppSettings>();
    change(&mut store.settings);
    store.save();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Frischer Ordner unter dem Temp-Ordner, verschwindet beim Drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> TempDir {
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("kadrell-settings-test-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn paths_in(dir: &TempDir) -> Paths {
        Paths::new(dir.path().join("home"), dir.path().join("support"), dir.path().join("tmp"))
    }

    #[test]
    fn only_the_temp_profile_gets_a_file() {
        let dir = TempDir::new();
        let paths = paths_in(&dir);
        assert!(AppSettings::for_profile(None, &paths, 7).path.is_none());
        assert!(AppSettings::for_profile(Some("work"), &paths, 7).path.is_none());
        let tmp = AppSettings::for_profile(Some("tmp"), &paths, 7);
        assert_eq!(tmp.path.as_deref(), Some(paths.temp_profile_dir(7).join("settings.json").as_path()));
    }

    #[test]
    fn temp_profile_starts_from_the_default_profile_and_saves_into_its_own_folder() {
        let dir = TempDir::new();
        let paths = paths_in(&dir);
        std::fs::create_dir_all(paths.default_profile_dir()).unwrap();
        let mut default = Settings::default();
        default.set_claude_model("opus");
        default.save(&paths.default_profile_dir().join("settings.json")).unwrap();

        let mut store = AppSettings::for_profile(Some("tmp"), &paths, 7);
        assert_eq!(store.settings.claude_model(), "opus");
        store.settings.set_claude_model("haiku");
        store.save();
        let reread = Settings::load(&paths.temp_profile_dir(7).join("settings.json"));
        assert_eq!(reread.claude_model(), "haiku");
        // Das Standardprofil bleibt unberührt.
        assert_eq!(Settings::load(&paths.default_profile_dir().join("settings.json")).claude_model(), "opus");
    }
}
