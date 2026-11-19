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
use kadrell_ext::events::{self, SessionState};
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

/// Meldet der Oberfläche, dass sich bei den Extensions etwas geändert hat. Der Manager ruft `on_change` aus
/// Helfer-Threads, bis zu 50-mal pro Sekunde; ein Kanal mit Platz für eine Meldung bündelt das (eine offene Meldung
/// genügt, die Oberfläche liest danach den neuesten Stand).
fn ping() -> &'static (async_channel::Sender<()>, async_channel::Receiver<()>) {
    static PING: OnceLock<(async_channel::Sender<()>, async_channel::Receiver<()>)> = OnceLock::new();
    PING.get_or_init(|| async_channel::bounded(1))
}

/// Die Meldungen des Managers für die Oberfläche (eine Anwendung je Prozess).
pub fn changes() -> async_channel::Receiver<()> {
    ping().1.clone()
}

/// Schreibt neue Logzeilen und geänderte Statuseinträge nach stderr (Diagnose) und weckt die Oberfläche.
fn change_logger(cell: Arc<OnceLock<ExtensionManager>>) -> Arc<dyn Fn() + Send + Sync> {
    let seen: Mutex<(HashMap<String, usize>, HashMap<String, String>)> = Mutex::default();
    Arc::new(move || {
        let _ = ping().0.try_send(());
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

// MARK: Ereignisse an die Extensions

type Known = HashMap<String, (SessionState, Value)>;

/// Die Session als Event-Daten (`state` wie in `kadrell ls --json`, `group` der Gruppenname oder `null`).
pub fn session_payload(state: &AppState, s: &kadrell_core::Session) -> Value {
    let group = state.store.groups.group_for_session(&s.id).map_or(Value::Null, |g| Value::String(g.name.clone()));
    serde_json::json!({
        "key": s.id,
        "title": s.title(),
        "cwd": s.cwd,
        "branch": s.branch,
        "sessionId": s.session_id,
        "state": s.status().as_str(),
        "group": group,
    })
}

/// Merkt sich den letzten Stand der Sessions und leitet daraus `session.new`, `session.status` und `session.remove` ab.
/// Läuft auch ohne gestartete Extensions mit, sonst kämen beim Start alle Sessions als neu.
#[derive(Default)]
pub struct SessionTracker {
    known: Known,
}

impl SessionTracker {
    /// Events seit dem letzten Aufruf als `(Name, Daten)`. `session.status` trägt zusätzlich `state`; `session.remove`
    /// trägt die Daten der Session vom letzten bekannten Stand.
    pub fn diff(&mut self, now: Vec<(SessionState, Value)>) -> Vec<(&'static str, Value)> {
        let old: HashMap<String, SessionState> = self.known.iter().map(|(k, (s, _))| (k.clone(), s.clone())).collect();
        let states: Vec<SessionState> = now.iter().map(|(s, _)| s.clone()).collect();
        let events = events::session_events(&old, &states);
        let fresh: Known = now.into_iter().map(|(s, v)| (s.id.clone(), (s, v))).collect();
        let out = events.into_iter().map(|e| (e.event, event_data(e.event, &e.id, &self.known, &fresh))).collect();
        self.known = fresh;
        out
    }
}

fn event_data(event: &str, id: &str, old: &Known, fresh: &Known) -> Value {
    let from = if event == "session.remove" { old } else { fresh };
    let session = from.get(id).map_or(Value::Null, |(_, v)| v.clone());
    let mut data = serde_json::json!({ "session": session });
    if event == "session.status" {
        let state = fresh.get(id).map_or("", |(s, _)| s.status.as_str());
        data["state"] = Value::String(state.to_string());
    }
    data
}

impl crate::Shell {
    /// Nach jeder Änderung: `app.locale` bei neuer Sprache, `session.focus` für gefeuerte Fokus-Hooks, dann
    /// `session.new`/`status`/`remove` aus dem Vergleich mit dem letzten Stand.
    pub(crate) fn emit_ext_events(&mut self) {
        let code = kadrell_i18n::global().lang().code();
        kadrell_ext::i18n::set_locale(code);
        self.ext.set_locale(code);
        for key in self.state.take_focus_fired() {
            if let Some(s) = self.state.session(&key) {
                self.ext.emit("session.focus", serde_json::json!({ "session": session_payload(&self.state, s) }));
            }
        }
        for (event, data) in self.ext_sessions.diff(self.state.extension_sessions()) {
            self.ext.emit(event, data);
        }
    }
}

impl AppState {
    /// Die Sessions im Format, das `SessionTracker::diff` braucht.
    pub fn extension_sessions(&self) -> Vec<(SessionState, Value)> {
        let sessions = &self.store.sessions.sessions;
        sessions.iter().map(|s| (SessionState { id: s.id.clone(), status: s.status().as_str().to_string() }, session_payload(self, s))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, status: &str, title: &str) -> (SessionState, Value) {
        (SessionState { id: id.into(), status: status.into() }, serde_json::json!({ "key": id, "title": title, "state": status }))
    }

    fn names(events: &[(&'static str, Value)]) -> Vec<String> {
        events.iter().map(|(e, d)| format!("{e} {}", d["session"]["key"].as_str().unwrap_or("?"))).collect()
    }

    #[test]
    fn first_diff_reports_everything_as_new() {
        let mut t = SessionTracker::default();
        let events = t.diff(vec![entry("a", "idle", "A")]);
        assert_eq!(names(&events), ["session.new a"]);
    }

    #[test]
    fn status_event_only_on_state_change_and_carries_state() {
        let mut t = SessionTracker::default();
        t.diff(vec![entry("a", "idle", "A")]);
        assert!(t.diff(vec![entry("a", "idle", "Neuer Titel")]).is_empty());
        let events = t.diff(vec![entry("a", "waiting", "Neuer Titel")]);
        assert_eq!(names(&events), ["session.status a"]);
        assert_eq!(events[0].1["state"], "waiting");
        assert_eq!(events[0].1["session"]["title"], "Neuer Titel");
    }

    #[test]
    fn remove_events_are_sorted_and_carry_the_last_known_data() {
        let mut t = SessionTracker::default();
        t.diff(vec![entry("b", "idle", "B"), entry("a", "idle", "A"), entry("c", "idle", "C")]);
        let events = t.diff(vec![entry("c", "idle", "C")]);
        assert_eq!(names(&events), ["session.remove a", "session.remove b"]);
        assert_eq!(events[1].1["session"]["title"], "B");
        assert!(events[0].1.get("state").is_none());
    }
}
