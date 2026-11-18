//! Hilfen für Tests (auch die in `tests/`): Temp-Ordner, Warten auf Bedingungen.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Ein frischer Ordner unter dem Temp-Verzeichnis, beim Fallenlassen gelöscht.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> TempDir {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("kadrell-ext-{label}-{}-{n}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
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

/// Legt `<root>/<name>/{kadrell.json,init.lua}` an und gibt den Ordner der Extension zurück.
pub fn make_extension(root: &Path, name: &str, init_lua: &str, manifest: Option<serde_json::Value>) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).expect("extension dir");
    let manifest = manifest.unwrap_or_else(|| serde_json::json!({"name": name, "version": "0.1.0", "apiVersion": 1}));
    std::fs::write(dir.join("kadrell.json"), manifest.to_string()).expect("manifest");
    std::fs::write(dir.join("init.lua"), init_lua).expect("init.lua");
    dir
}

/// Wartet bis zu `seconds`, bis `condition` gilt.
pub fn wait_until(seconds: f64, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs_f64(seconds);
    while !condition() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    condition()
}
