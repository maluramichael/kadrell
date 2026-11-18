//! Die Extension-Schicht in der App: `ExtensionManager` aus `kadrell-ext`, verdrahtet mit dem Steuerkanal, dem
//! Schlüsselbund und der Einstellungsdatei des Profils. Panels und Fußzeile folgen mit der Oberfläche; bis dahin
//! schreibt der Manager nur, was Extensions setzen, ins Log (stderr).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use kadrell_accounts::SecretStore as KeychainStore;
use kadrell_control::protocol::ControlRequest;
use kadrell_control::Service;
use kadrell_core::Settings;
use kadrell_ext::bridge::{ControlResponse, ControlRunner, SecretStore, SettingsStore};
use kadrell_ext::manager::{ExtensionManager, ManagerConfig, SessionsProvider};
use kadrell_ext::process::ProcessConfig;
use serde_json::Value;

use crate::state::AppState;

/// `SettingsStore` auf der `settings.json` des Profils. Jeder Zugriff liest und schreibt die Datei neu (nur der eine
/// Schlüssel), damit Änderungen der Oberfläche nicht verloren gehen.
pub struct ProfileSettingsStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl ProfileSettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: Mutex::new(()) }
    }
}

impl SettingsStore for ProfileSettingsStore {
    fn get(&self, key: &str) -> Option<Value> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        Settings::load(&self.path).get(key).cloned()
    }

    fn set(&self, key: &str, value: Option<Value>) {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut settings = Settings::load(&self.path);
        match value {
            Some(v) => settings.set(key, v),
            None => settings.remove(key),
        }
        if let Err(e) = settings.save(&self.path) {
            eprintln!("kadrell: Extension-Einstellung nicht gespeichert: {e}");
        }
    }
}

/// Schlüsselbund (macOS) oder Credential Manager (Windows) aus `kadrell-accounts`.
pub struct Keychain(Box<dyn KeychainStore>);

impl Keychain {
    pub fn system() -> Self {
        Self(kadrell_accounts::default_secret_store())
    }
}

impl SecretStore for Keychain {
    fn read(&self, service: &str, account: &str) -> Option<String> {
        self.0.read(service, Some(account))
    }

    fn write(&self, service: &str, account: &str, value: &str) -> bool {
        self.0.write(service, account, value)
    }

    fn delete(&self, service: &str, account: &str) {
        self.0.delete(service, account);
    }
}

/// `kadrell.run`: der Befehl läuft durch denselben Dienst wie der Steuerkanal, also auf dem GPUI-Thread.
struct Runner {
    service: Service,
    cwd: String,
}

impl ControlRunner for Runner {
    fn run(&self, argv: &[String]) -> ControlResponse {
        let req = ControlRequest { argv: argv.to_vec(), cwd: self.cwd.clone(), caller: None, token: String::new() };
        let r = (self.service)(req, None);
        ControlResponse { status: r.status, stdout: r.stdout, stderr: r.stderr }
    }
}

/// Die Session-Objekte für `hello`: Ausgabe von `ls --json`.
fn sessions(runner: Arc<Runner>) -> SessionsProvider {
    Arc::new(move || {
        let r = runner.run(&["ls".to_string(), "--json".to_string()]);
        match serde_json::from_str::<Value>(&r.stdout) {
            Ok(Value::Array(list)) => list,
            _ => Vec::new(),
        }
    })
}

/// Ordner der Extensions: `--extensions-dir` oder `KADRELL_EXTENSIONS_DIR`, sonst `~/.config/kadrell/extensions`.
fn catalog_dir() -> PathBuf {
    kadrell_ext::manifest::default_dir()
}

/// Baut den Manager für das Profil des Zustands. Gestartet wird er mit `ExtensionManager::start`.
pub fn build(state: &AppState, service: Service) -> ExtensionManager {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("kadrell"));
    let mut process = ProcessConfig::new(exe);
    // Der Helper findet den Steuerkanal dieser Instanz, nicht den eines anderen Profils.
    process.extra_environment = vec![("KADRELL_SOCKET".to_string(), state.endpoint.to_string_lossy().into_owned())];
    let mut config = ManagerConfig::new(catalog_dir(), process, state.profile.dir.join("extensions"));
    config.profile = state.profile.name.clone();
    let runner = Arc::new(Runner { service, cwd: state.paths.home.to_string_lossy().into_owned() });
    let store = Arc::new(ProfileSettingsStore::new(state.profile.settings_path()));
    let manager = ExtensionManager::new(config, runner.clone(), store, Arc::new(Keychain::system()), sessions(runner));
    let cell = Arc::new(OnceLock::new());
    manager.set_on_change(change_logger(Arc::clone(&cell)));
    let _ = cell.set(manager.clone());
    manager
}

/// Schreibt neue Logzeilen und geänderte Statuseinträge nach stderr, solange es keine Oberfläche dafür gibt.
fn change_logger(cell: Arc<OnceLock<ExtensionManager>>) -> Arc<dyn Fn() + Send + Sync> {
    let seen: Mutex<(HashMap<String, usize>, HashMap<String, String>)> = Mutex::default();
    Arc::new(move || {
        let Some(manager) = cell.get() else { return };
        let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
        for (name, item) in manager.status_items() {
            if seen.1.insert(name.clone(), item.text.clone()).as_deref() != Some(item.text.as_str()) {
                eprintln!("kadrell: extension {name}: status \"{}\"", item.text);
            }
        }
        for name in manager.found().into_iter().map(|f| f.name) {
            let log = manager.log(&name);
            let from = seen.0.insert(name.clone(), log.len()).unwrap_or(0).min(log.len());
            for line in &log[from..] {
                eprintln!("kadrell: extension {name}: {line}");
            }
        }
    })
}
