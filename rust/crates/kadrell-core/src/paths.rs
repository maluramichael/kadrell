//! Pfade: Profilordner je Plattform und kleine Pfadhelfer (Ordnername, Vergleich, `~` und `..` auflösen).
//!
//! Der einzige plattformabhängige Teil des Kerns. Alles andere bekommt Ordner von hier oder vom Aufrufer, damit
//! Tests mit eigenen Ordnern laufen.

use std::path::{Component, Path, PathBuf};

/// Ordner, aus denen sich alle Profilpfade ableiten. `from_env` liest die echte Umgebung, `new` nimmt feste Ordner
/// (Tests, Sonderfälle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Persönlicher Ordner des Nutzers.
    pub home: PathBuf,
    /// Standardprofil: macOS `~/Library/Application Support/de.malura.kadrell`, Windows `%APPDATA%\Kadrell`,
    /// sonst `$XDG_DATA_HOME/kadrell` (nur für Entwicklung und Tests).
    pub app_support: PathBuf,
    /// Ordner für Wegwerfprofile (`kadrell-tmp-<pid>`).
    pub temp: PathBuf,
}

impl Paths {
    /// Pfade der laufenden Umgebung.
    pub fn from_env() -> Paths {
        let home = home_dir();
        Paths { app_support: app_support_dir(&home), temp: std::env::temp_dir(), home }
    }

    /// Feste Ordner, ohne die Umgebung zu lesen.
    pub fn new(home: impl Into<PathBuf>, app_support: impl Into<PathBuf>, temp: impl Into<PathBuf>) -> Paths {
        Paths { home: home.into(), app_support: app_support.into(), temp: temp.into() }
    }

    /// Ordner des Standardprofils.
    pub fn default_profile_dir(&self) -> PathBuf {
        self.app_support.clone()
    }

    /// Ordner eines benannten Profils (`--profile <name>`): `<Standardprofil>/profiles/<name>`.
    pub fn named_profile_dir(&self, name: &str) -> PathBuf {
        self.app_support.join("profiles").join(name)
    }

    /// Ordner des Wegwerfprofils (`--profile tmp`) der Instanz mit dieser pid.
    pub fn temp_profile_dir(&self, pid: u32) -> PathBuf {
        self.temp.join(format!("{TEMP_PREFIX}{pid}"))
    }
}

/// Namensanfang der Wegwerfprofile im Temp-Ordner.
pub const TEMP_PREFIX: &str = "kadrell-tmp-";

/// Persönlicher Ordner: unter Windows `USERPROFILE`, sonst `HOME`, notfalls der aktuelle Ordner.
pub fn home_dir() -> PathBuf {
    let (first, second) = if cfg!(windows) { ("USERPROFILE", "HOME") } else { ("HOME", "USERPROFILE") };
    std::env::var_os(first).or_else(|| std::env::var_os(second)).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Ordner des Standardprofils für `home`, je nach Betriebssystem.
pub fn app_support_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/de.malura.kadrell")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData/Roaming")).join("Kadrell")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share")).join("kadrell")
    }
}

/// Letzter Pfadbestandteil, wie `URL.lastPathComponent` mit `\` und `/`. Ein Pfad ohne Namen (`/`, `C:\`) bleibt
/// ganz stehen.
pub fn base_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let base = trimmed.rsplit(['/', '\\']).next().unwrap_or("");
    if base.is_empty() { path.to_string() } else { base.to_string() }
}

/// Sind das dieselben Ordner? Auf Windows ohne Rücksicht auf Groß-/Kleinschreibung, Schrägstrich-Richtung und
/// abschließenden Schrägstrich, sonst Zeichen für Zeichen wie auf dem Mac.
pub fn same_folder(a: &str, b: &str) -> bool {
    if !cfg!(windows) {
        return a == b;
    }
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

/// `~`, relative Pfade gegen den Ordner des Aufrufers (`cwd`), `.` und `..` aufgelöst, immer mit Schrägstrichen
/// (wie `ControlTarget.path` auf dem Mac).
pub fn resolve_path(p: &str, cwd: &str, home: &Path) -> String {
    let expanded = match p.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => home.join(rest.trim_start_matches(['/', '\\'])),
        _ => PathBuf::from(p),
    };
    let abs = if expanded.is_absolute() { expanded } else { Path::new(cwd).join(expanded) };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().replace('\\', "/")
}

/// Hilfen für Tests: eigener, frischer Ordner unter dem Temp-Ordner, der beim Drop verschwindet.
#[cfg(test)]
pub(crate) mod testutil {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new() -> TempDir {
            static N: AtomicU32 = AtomicU32::new(0);
            let n = N.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("kadrell-core-test-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_name_handles_both_separators() {
        assert_eq!(base_name("C:/development/projects/kadrell/"), "kadrell");
        assert_eq!(base_name("/p/homelab"), "homelab");
        assert_eq!(base_name("C:\\p\\beta"), "beta");
        assert_eq!(base_name("/"), "/");
    }

    #[test]
    fn profile_dirs_derive_from_app_support() {
        let p = Paths::new("/h", "/h/support", "/tmp");
        assert_eq!(p.default_profile_dir(), PathBuf::from("/h/support"));
        assert_eq!(p.named_profile_dir("work"), PathBuf::from("/h/support").join("profiles").join("work"));
        assert_eq!(p.temp_profile_dir(42), PathBuf::from("/tmp").join("kadrell-tmp-42"));
    }

    #[test]
    fn app_support_follows_platform() {
        let dir = app_support_dir(Path::new("/h"));
        let last = dir.file_name().unwrap().to_string_lossy().to_string();
        if cfg!(target_os = "macos") {
            assert_eq!(last, "de.malura.kadrell");
        } else if cfg!(windows) {
            assert_eq!(last, "Kadrell");
        } else {
            assert_eq!(last, "kadrell");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn resolve_path_expands_home_and_dots() {
        let home = Path::new("/home/m");
        assert_eq!(resolve_path("~/d/x", "/w", home), "/home/m/d/x");
        assert_eq!(resolve_path("../y/./z", "/w/v", home), "/w/y/z");
        assert_eq!(resolve_path("rel", "/w", home), "/w/rel");
    }

    #[cfg(windows)]
    #[test]
    fn resolve_path_windows_drives() {
        let home = Path::new("C:/Users/m");
        assert_eq!(resolve_path("~/d/x", "C:/w", home), "C:/Users/m/d/x");
        assert_eq!(resolve_path("../y/./z", "C:/w/v", home), "C:/w/y/z");
        assert_eq!(resolve_path("D:\\a", "C:/w", home), "D:/a");
    }

    #[cfg(windows)]
    #[test]
    fn same_folder_ignores_case_and_separators_on_windows() {
        assert!(same_folder("c:\\p\\Alpha\\", "C:/p/alpha"));
        assert!(!same_folder("C:/p/a", "C:/p/b"));
    }

    #[cfg(not(windows))]
    #[test]
    fn same_folder_is_exact_elsewhere() {
        assert!(same_folder("/p/a", "/p/a"));
        assert!(!same_folder("/p/a", "/p/A"));
    }
}
