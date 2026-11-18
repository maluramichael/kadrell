//! Alle Extensions eines Profils: welche an sind, ihre Prozesse, Panels, Statuseinträge und Logs. Je Extension entscheidet
//! ein `ExtensionSupervisor`, hier werden seine Aktionen ausgeführt und Events, Steuerbefehle (`kadrell.run`) und
//! Dateiänderungen weitergereicht.
//!
//! Der Manager ist ein billig kopierbares Handle und von jedem Thread aus benutzbar. Der Zustand liegt hinter einem Lock;
//! Aufrufe in die App (`ControlRunner`, `SecretStore`, `SettingsStore`, Sitzungsliste) laufen nie unter diesem Lock,
//! sondern auf Arbeitsthreads, und `on_change` wird erst nach dem Loslassen des Locks gerufen. Die App darf deshalb
//! aus ihren Callbacks heraus den Manager befragen.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use crate::bridge::{self, ControlResponse, ControlRunner, SecretStore, SettingsStore};
use crate::footer::{self, FooterItem};
use crate::i18n::{self, tr_args};
use crate::manifest::{self, FoundExtension};
use crate::message::{ExtensionMessage, HostMessage};
use crate::panel::{self, Dock, PaletteItem, PanelError, PanelTree, StatusItem};
use crate::process::{EventHandler, ExtensionProcess, ProcessConfig, ProcessEvent};
use crate::settings::{ExtensionSettings, KEYCHAIN_SERVICE};
use crate::supervisor::{ExtensionState, ExtensionSupervisor, SupervisorAction, SupervisorInput};
use crate::watcher::FolderWatcher;

/// Höchstens so viele Logzeilen je Extension.
pub const MAX_LOG: usize = 200;
/// So lange sammelt der Beobachter Dateiänderungen, bevor er neu einliest.
pub const RELOAD_DELAY: Duration = Duration::from_millis(300);

/// Liefert die Sessions für `hello` (Form von `kadrell ls --json`: die Session-Objekte).
pub type SessionsProvider = Arc<dyn Fn() -> Vec<Value> + Send + Sync>;
pub type ChangeCallback = Arc<dyn Fn() + Send + Sync>;

/// Was der Manager zum Start braucht.
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// Ordner mit den Extensions (`~/.config/kadrell/extensions` oder `--extensions-dir`).
    pub catalog_dir: PathBuf,
    /// So startet der Helper (`kadrell ext-host <ordner>`).
    pub process: ProcessConfig,
    /// Wurzel der Persistenz (`kadrell.storage`), pro Extension ein Unterordner, normalerweise `<profil>/extensions`.
    pub storage_root: PathBuf,
    /// Profilname (`None` heißt Standardprofil), trennt die Schlüsselbund-Einträge.
    pub profile: Option<String>,
    /// Sprache, die laufende Extensions kennen (`hello`, danach `app.locale`): `de` oder `en`.
    pub locale: String,
    /// Wartezeit des Dateibeobachters; die Voreinstellung ist `RELOAD_DELAY`.
    pub reload_delay: Duration,
}

impl ManagerConfig {
    pub fn new(
        catalog_dir: impl Into<PathBuf>,
        process: ProcessConfig,
        storage_root: impl Into<PathBuf>,
    ) -> ManagerConfig {
        ManagerConfig {
            catalog_dir: catalog_dir.into(),
            process,
            storage_root: storage_root.into(),
            profile: None,
            locale: i18n::locale().to_string(),
            reload_delay: RELOAD_DELAY,
        }
    }
}

struct ProcEntry {
    id: u64,
    process: Arc<ExtensionProcess>,
}

#[derive(Default)]
struct State {
    found: Vec<FoundExtension>,
    supervisors: HashMap<String, ExtensionSupervisor>,
    /// Aktueller Prozess je Extension, eingetragen bis zu seinem Ende, auch während `stop()`.
    procs: HashMap<String, ProcEntry>,
    /// Abgelöste Prozesse, die noch sauber enden dürfen; was sie melden, zählt nicht mehr.
    retired: Vec<ProcEntry>,
    /// Zähler der geplanten Neustarts: ein Neustart gilt nur, solange sein Zähler noch der aktuelle ist.
    restarts: HashMap<String, u64>,
    /// Zählt Starts: ein Start, der noch auf seine Einstellungen wartet, verfällt, sobald ein neuerer begonnen hat.
    launches: HashMap<String, u64>,
    trees: HashMap<String, PanelTree>,
    docks: HashMap<String, Dock>,
    statuses: HashMap<String, StatusItem>,
    palettes: HashMap<String, Vec<PaletteItem>>,
    footers: HashMap<String, Vec<FooterItem>>,
    logs: HashMap<String, Vec<String>>,
    /// Änderungszeiten der Lua-Dateien und des Manifests je Extension.
    stamps: HashMap<String, HashMap<PathBuf, Option<SystemTime>>>,
    /// `app.ready` ist schon gelaufen: jeder später gestartete Prozess bekommt es direkt nach `hello`.
    app_ready: bool,
    /// Letztes `session.focus`: ein später gestarteter Prozess bekommt es nach `app.ready`.
    focus: Option<Value>,
    locale: String,
    watcher: Option<FolderWatcher>,
    counter: u64,
    dirty: bool,
}

struct Inner {
    config: ManagerConfig,
    control: Arc<dyn ControlRunner>,
    store: Arc<dyn SettingsStore>,
    secrets: Arc<dyn SecretStore>,
    settings: ExtensionSettings,
    sessions: SessionsProvider,
    on_change: Mutex<ChangeCallback>,
    state: Mutex<State>,
}

/// Handle auf den Manager; `Clone` teilt denselben Zustand.
#[derive(Clone)]
pub struct ExtensionManager {
    inner: Arc<Inner>,
}

impl ExtensionManager {
    pub fn new(
        config: ManagerConfig,
        control: Arc<dyn ControlRunner>,
        store: Arc<dyn SettingsStore>,
        secrets: Arc<dyn SecretStore>,
        sessions: SessionsProvider,
    ) -> ExtensionManager {
        let settings = ExtensionSettings::new(
            Arc::clone(&store),
            Arc::clone(&secrets),
            config.profile.clone(),
        );
        let state = State {
            locale: config.locale.clone(),
            ..State::default()
        };
        let inner = Inner {
            config,
            control,
            store,
            secrets,
            settings,
            sessions,
            on_change: Mutex::new(Arc::new(|| {})),
            state: Mutex::new(state),
        };
        ExtensionManager {
            inner: Arc::new(inner),
        }
    }

    /// Wird nach jeder Änderung gerufen (bis zu 50-mal pro Sekunde, jede Logzeile); der Aufrufer bündelt selbst.
    pub fn set_on_change(&self, callback: ChangeCallback) {
        *self
            .inner
            .on_change
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = callback;
    }

    pub fn catalog_dir(&self) -> &Path {
        &self.inner.config.catalog_dir
    }

    /// Zugriff auf die Einstellungswerte (für den Einstellungsdialog der App).
    pub fn settings(&self) -> &ExtensionSettings {
        &self.inner.settings
    }

    // MARK: Lesen

    pub fn found(&self) -> Vec<FoundExtension> {
        self.inner.read(|s| s.found.clone())
    }

    /// Alle Panels, Dock-übergreifend, in Fundreihenfolge.
    pub fn panels(&self) -> Vec<(String, PanelTree)> {
        self.inner.read(|s| {
            s.found
                .iter()
                .filter_map(|f| s.trees.get(&f.name).map(|t| (f.name.clone(), t.clone())))
                .collect()
        })
    }

    /// Panels eines Docks, in Fundreihenfolge. Ohne gesetztes Dock gilt rechts.
    pub fn panels_in(&self, dock: Dock) -> Vec<(String, PanelTree)> {
        self.inner.read(|s| {
            s.found
                .iter()
                .filter(|f| s.docks.get(&f.name).copied().unwrap_or_default() == dock)
                .filter_map(|f| s.trees.get(&f.name).map(|t| (f.name.clone(), t.clone())))
                .collect()
        })
    }

    pub fn status_items(&self) -> Vec<(String, StatusItem)> {
        self.inner.read(|s| {
            s.found
                .iter()
                .filter_map(|f| s.statuses.get(&f.name).map(|i| (f.name.clone(), i.clone())))
                .collect()
        })
    }

    /// Alle Einträge der Statusleiste unten, in Fundreihenfolge der Extensions und je Extension in ihrer Reihenfolge.
    pub fn footer_items(&self) -> Vec<(String, FooterItem)> {
        self.inner.read(|s| {
            s.found
                .iter()
                .flat_map(|f| {
                    s.footers
                        .get(&f.name)
                        .into_iter()
                        .flatten()
                        .map(|i| (f.name.clone(), i.clone()))
                })
                .collect()
        })
    }

    /// Alle von Extensions gelieferten ⌘N-Einträge, mit ihrer Extension.
    pub fn palette_entries(&self) -> Vec<(String, PaletteItem)> {
        self.inner.read(|s| {
            s.found
                .iter()
                .flat_map(|f| {
                    s.palettes
                        .get(&f.name)
                        .into_iter()
                        .flatten()
                        .map(|i| (f.name.clone(), i.clone()))
                })
                .collect()
        })
    }

    pub fn state(&self, name: &str) -> ExtensionState {
        self.inner.read(|s| s.state(name))
    }

    pub fn log(&self, name: &str) -> Vec<String> {
        self.inner
            .read(|s| s.logs.get(name).cloned().unwrap_or_default())
    }

    pub fn is_enabled(&self, name: &str) -> bool {
        bridge::enabled_extensions(&*self.inner.store)
            .iter()
            .any(|n| n == name)
    }

    /// Die Sprache, die laufende Extensions kennen.
    pub fn locale(&self) -> String {
        self.inner.read(|s| s.locale.clone())
    }

    /// Prozess-ids aller Helper, die noch nicht zu Ende gemeldet sind (auch abgelöste), für Diagnose und Tests.
    pub fn pids(&self) -> Vec<u32> {
        self.inner.read(|s| {
            s.procs
                .values()
                .chain(s.retired.iter())
                .map(|e| e.process.pid())
                .collect()
        })
    }

    // MARK: Steuern

    /// Beobachtet den Katalog und liest ihn zum ersten Mal ein.
    pub fn start(&self) {
        let weak = Arc::downgrade(&self.inner);
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(inner) = weak.upgrade() {
                inner.rescan();
            }
        });
        let watcher = FolderWatcher::new(
            self.inner.config.catalog_dir.clone(),
            self.inner.config.reload_delay,
            callback,
        );
        self.inner.locked(|_, s| s.watcher = Some(watcher));
        self.inner.rescan();
    }

    pub fn set_enabled(&self, name: &str, on: bool) {
        let store = &*self.inner.store;
        let mut list: Vec<String> = bridge::enabled_extensions(store)
            .into_iter()
            .filter(|n| n != name)
            .collect();
        if on {
            list.push(name.to_string());
        }
        bridge::set_enabled_extensions(store, &list);
        self.inner.locked(|i, s| {
            if on
                && !s
                    .found
                    .iter()
                    .any(|f| f.name == name && f.problem.is_none())
            {
                return;
            }
            i.feed(
                s,
                name,
                if on {
                    SupervisorInput::Enable
                } else {
                    SupervisorInput::Disable
                },
            );
        });
    }

    pub fn reload(&self, name: &str) {
        self.inner
            .locked(|i, s| i.feed(s, name, SupervisorInput::Reload));
    }

    /// Nach einer Installation aus dem Marktplatz sofort neu einlesen, statt auf den Beobachter zu warten.
    pub fn refresh(&self) {
        self.inner.rescan();
    }

    /// Schickt ein Event an alle laufenden Extensions.
    pub fn emit(&self, event: &str, data: Value) {
        self.inner.locked(|_, s| {
            if event == "app.ready" {
                s.app_ready = true;
            }
            if event == "session.focus" {
                s.focus = Some(data.clone());
            }
            for name in s.live_names() {
                s.send_event(&name, event, data.clone());
            }
        });
    }

    /// Neue Sprache: als Event `app.locale` an alle laufenden, nur wenn sie sich wirklich geändert hat.
    pub fn set_locale(&self, new: &str) {
        let changed = self.inner.locked(|_, s| {
            let changed = s.locale != new;
            if changed {
                s.locale = new.to_string();
            }
            changed
        });
        if changed {
            self.emit("app.locale", json!({ "locale": new }));
        }
    }

    /// Klick auf einen Panel-Button, Statuseintrag oder Eintrag der Statusleiste: Event `ui.action`.
    pub fn action(&self, name: &str, id: &str) {
        self.inner
            .locked(|_, s| s.send_event(name, "ui.action", json!({ "id": id })));
    }

    /// Auswahl eines Palette-Eintrags: geht als Event `palette.select` an die liefernde Extension.
    pub fn palette_select(&self, name: &str, id: &str) {
        self.inner
            .locked(|_, s| s.send_event(name, "palette.select", json!({ "id": id })));
    }

    /// Beim Beenden: alle stoppen wie beim Abschalten, ohne die Auswahl in den Einstellungen anzufassen.
    pub fn shutdown_all(&self) {
        let watcher = self.inner.locked(|i, s| {
            let watcher = s.watcher.take();
            let names: Vec<String> = s.supervisors.keys().cloned().collect();
            for n in names {
                i.feed(s, &n, SupervisorInput::Disable);
            }
            watcher
        });
        // Außerhalb des Locks: der Beobachter-Thread könnte gerade auf ihn warten.
        if let Some(mut w) = watcher {
            w.stop();
        }
    }
}

impl State {
    fn state(&self, name: &str) -> ExtensionState {
        self.supervisors
            .get(name)
            .map(|s| s.state().clone())
            .unwrap_or_default()
    }

    /// Ein Prozess, der gerade stoppt, ist nicht mehr live: sein spätes ready darf keinen neuen Start überholen.
    fn is_live(&self, name: &str, id: Option<u64>) -> bool {
        let Some(entry) = self.procs.get(name) else {
            return false;
        };
        id.is_none_or(|i| i == entry.id)
            && !entry.process.is_stopping()
            && matches!(
                self.state(name),
                ExtensionState::Starting | ExtensionState::Running
            )
    }

    fn live_names(&self) -> Vec<String> {
        self.procs
            .keys()
            .filter(|n| self.is_live(n, None))
            .cloned()
            .collect()
    }

    fn send_event(&self, name: &str, event: &str, data: Value) {
        if let Some(e) = self.procs.get(name).filter(|_| self.is_live(name, None)) {
            e.process.send(HostMessage::Event {
                name: event.to_string(),
                data,
            });
        }
    }

    /// Ringpuffer: höchstens `MAX_LOG` Zeilen zu je höchstens 500 Zeichen.
    fn append(&mut self, name: &str, line: &str) {
        let log = self.logs.entry(name.to_string()).or_default();
        log.push(crate::json::clip(line, panel::MAX_TEXT));
        if log.len() > MAX_LOG {
            log.drain(..log.len() - MAX_LOG);
        }
    }

    fn next_id(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    fn clear_ui(&mut self, name: &str) {
        self.trees.remove(name);
        self.docks.remove(name);
        self.statuses.remove(name);
        self.palettes.remove(name);
        self.footers.remove(name);
    }
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn read<R>(&self, f: impl FnOnce(&State) -> R) -> R {
        f(&self.lock())
    }

    /// Zustand ändern; `on_change` kommt erst nach dem Loslassen des Locks.
    fn locked<R>(self: &Arc<Self>, f: impl FnOnce(&Arc<Inner>, &mut State) -> R) -> R {
        let (result, dirty) = {
            let mut guard = self.lock();
            let result = f(self, &mut guard);
            (result, std::mem::take(&mut guard.dirty))
        };
        if dirty {
            let callback = self
                .on_change
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            callback();
        }
        result
    }

    // MARK: Katalog

    /// Neu einlesen: verschwundene stoppen und vergessen, geänderte eingeschaltete neu laden, fehlende starten.
    fn rescan(self: &Arc<Self>) {
        let scanned = manifest::scan(&self.config.catalog_dir);
        self.locked(|i, s| {
            let names: HashSet<&str> = scanned.iter().map(|f| f.name.as_str()).collect();
            let gone: Vec<String> = s
                .found
                .iter()
                .filter(|f| !names.contains(f.name.as_str()))
                .map(|f| f.name.clone())
                .collect();
            for name in gone {
                i.vanish(s, &name);
            }
            s.found = scanned.clone();
            for f in &scanned {
                let stamp = stamp(&f.dir);
                let changed = s.stamps.get(&f.name).is_some_and(|old| *old != stamp);
                s.stamps.insert(f.name.clone(), stamp);
                i.reconcile(s, f, changed);
            }
            s.dirty = true;
        });
    }

    fn reconcile(self: &Arc<Self>, s: &mut State, f: &FoundExtension, changed: bool) {
        let wanted =
            f.problem.is_none() && bridge::enabled_extensions(&*self.store).contains(&f.name);
        let off = s.state(&f.name) == ExtensionState::Off;
        match (wanted, off) {
            (false, true) => {}
            (false, false) => self.feed(s, &f.name, SupervisorInput::Disable),
            (true, true) => self.feed(s, &f.name, SupervisorInput::Enable),
            (true, false) => {
                if changed {
                    self.feed(s, &f.name, SupervisorInput::Reload);
                }
            }
        }
    }

    fn vanish(self: &Arc<Self>, s: &mut State, name: &str) {
        self.feed(s, name, SupervisorInput::Disable);
        s.supervisors.remove(name);
        s.logs.remove(name);
        s.stamps.remove(name);
        s.launches.remove(name);
    }

    // MARK: Zustandsmaschine

    fn feed(self: &Arc<Self>, s: &mut State, name: &str, input: SupervisorInput) {
        if matches!(
            input,
            SupervisorInput::Enable | SupervisorInput::Reload | SupervisorInput::Disable
        ) {
            s.restarts.remove(name);
        }
        let mut sup = s.supervisors.remove(name).unwrap_or_default();
        let before = sup.state().clone();
        let actions = sup.handle(input, Instant::now());
        let after = sup.state().clone();
        s.supervisors.insert(name.to_string(), sup);
        if after != before {
            s.append(name, &after.label());
        }
        for action in actions {
            self.perform(s, action, name);
        }
        s.dirty = true;
    }

    fn perform(self: &Arc<Self>, s: &mut State, action: SupervisorAction, name: &str) {
        match action {
            SupervisorAction::Spawn => self.spawn(s, name),
            SupervisorAction::Terminate => {
                // Kein Prozess (Start wartet noch auf Einstellungen oder ist gescheitert): gilt als schon beendet.
                match s.procs.get(name) {
                    Some(e) => e.process.stop(),
                    None => self.feed(
                        s,
                        name,
                        SupervisorInput::Exited {
                            expected: true,
                            reason: String::new(),
                        },
                    ),
                }
            }
            SupervisorAction::ScheduleRestart(delay) => self.schedule_restart(s, name, delay),
            SupervisorAction::ClearUi => s.clear_ui(name),
        }
    }

    fn schedule_restart(self: &Arc<Self>, s: &mut State, name: &str, delay: Duration) {
        let generation = s.next_id();
        s.restarts.insert(name.to_string(), generation);
        let (weak, name) = (Arc::downgrade(self), name.to_string());
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            let Some(inner) = weak.upgrade() else { return };
            inner.locked(|i, s| {
                if s.restarts.get(&name) == Some(&generation) {
                    s.restarts.remove(&name);
                    i.feed(s, &name, SupervisorInput::RestartDue);
                }
            });
        });
    }

    /// Start: Einstellungen und Sitzungen holen (der Schlüsselbund darf blockieren) auf einem Arbeitsthread, dann unter dem
    /// Lock prüfen, ob der Start noch gilt, und den Prozess anlegen.
    fn spawn(self: &Arc<Self>, s: &mut State, name: &str) {
        let Some(ext) = s.found.iter().find(|f| f.name == name).cloned() else {
            return;
        };
        let n = s.launches.get(name).copied().unwrap_or(0) + 1;
        s.launches.insert(name.to_string(), n);
        let weak: Weak<Inner> = Arc::downgrade(self);
        std::thread::spawn(move || {
            let Some(inner) = weak.upgrade() else { return };
            let config = inner.settings.values(&ext);
            let sessions = (inner.sessions)();
            inner.locked(|i, s| {
                if s.launches.get(&ext.name) == Some(&n)
                    && s.state(&ext.name) == ExtensionState::Starting
                {
                    i.launch(s, &ext, config, sessions);
                }
            });
        });
    }

    fn launch(
        self: &Arc<Self>,
        s: &mut State,
        ext: &FoundExtension,
        config: serde_json::Map<String, Value>,
        sessions: Vec<Value>,
    ) {
        let name = ext.name.clone();
        self.retire(s, &name, false);
        let id = s.next_id();
        let handler = self.handler(&name, id);
        let storage = self.storage_dir(&name);
        let hello = HostMessage::Hello {
            api: manifest::SUPPORTED_API,
            name: name.clone(),
            dir: ext.dir.to_string_lossy().into_owned(),
            storage_dir: storage,
            config,
            locale: s.locale.clone(),
            sessions,
        };
        match ExtensionProcess::spawn(&self.config.process, &ext.dir, hello, handler) {
            Ok(process) => {
                let process = Arc::new(process);
                if s.app_ready {
                    process.send(HostMessage::Event {
                        name: "app.ready".into(),
                        data: json!({}),
                    });
                    if let Some(focus) = &s.focus {
                        process.send(HostMessage::Event {
                            name: "session.focus".into(),
                            data: focus.clone(),
                        });
                    }
                }
                s.procs.insert(name, ProcEntry { id, process });
            }
            Err(e) => self.feed(
                s,
                &name,
                SupervisorInput::Exited {
                    expected: false,
                    reason: e.to_string(),
                },
            ),
        }
    }

    fn handler(self: &Arc<Self>, name: &str, id: u64) -> EventHandler {
        let (weak, name) = (Arc::downgrade(self), name.to_string());
        Arc::new(move |event| {
            if let Some(inner) = weak.upgrade() {
                inner.on_event(&name, id, event);
            }
        })
    }

    /// Persistenz der Extension (`kadrell.storage`), pro Profil, überlebt Reload und Neustart.
    fn storage_dir(&self, name: &str) -> String {
        let dir = self.config.storage_root.join(name);
        let _ = std::fs::create_dir_all(&dir);
        dir.to_string_lossy().into_owned()
    }

    /// Den aktuellen Prozess ablösen: er stoppt sanft (`hard`: sofort), seine Meldungen zählen nicht mehr.
    fn retire(self: &Arc<Self>, s: &mut State, name: &str, hard: bool) {
        let Some(entry) = s.procs.remove(name) else {
            return;
        };
        if hard {
            entry.process.kill();
        } else {
            entry.process.stop();
        }
        s.retired.push(entry);
    }

    // MARK: Nachrichten

    fn on_event(self: &Arc<Self>, name: &str, id: u64, event: ProcessEvent) {
        self.locked(|i, s| match event {
            ProcessEvent::Message(m) => i.received(s, m, name, id),
            ProcessEvent::Stderr(line) => {
                s.append(name, &line);
                s.dirty = true;
            }
            ProcessEvent::Exit { expected, reason } => i.exited(s, name, id, expected, reason),
        });
    }

    fn exited(
        self: &Arc<Self>,
        s: &mut State,
        name: &str,
        id: u64,
        expected: bool,
        reason: String,
    ) {
        s.retired.retain(|e| e.id != id);
        if s.procs.get(name).is_none_or(|e| e.id != id) {
            return;
        }
        s.procs.remove(name);
        // Ist die Extension inzwischen verschwunden (Ordner weg), keinen Supervisor-Eintrag wieder anlegen.
        if !s.found.iter().any(|f| f.name == name) {
            return;
        }
        self.feed(s, name, SupervisorInput::Exited { expected, reason });
    }

    fn received(self: &Arc<Self>, s: &mut State, m: ExtensionMessage, name: &str, id: u64) {
        let live = s.is_live(name, Some(id));
        match m {
            ExtensionMessage::Ready => {
                if live {
                    self.feed(s, name, SupervisorInput::Ready);
                }
            }
            ExtensionMessage::Pong(_) => {}
            ExtensionMessage::Log { level, text } => {
                let line = if level == "info" {
                    text
                } else {
                    format!("[{level}] {text}")
                };
                s.append(name, &line);
                s.dirty = true;
            }
            ExtensionMessage::Run { id: request, argv } => {
                self.answer_run(s, name, id, request, argv, live)
            }
            ExtensionMessage::Panel { tree, dock } => {
                if live {
                    self.set_panel(s, name, tree, dock);
                }
            }
            ExtensionMessage::Status(v) => {
                if live {
                    set_status(s, name, v);
                }
            }
            ExtensionMessage::Palette(v) => {
                if live {
                    s.palettes
                        .insert(name.to_string(), panel::palette(v.as_ref()));
                    s.dirty = true;
                }
            }
            ExtensionMessage::Footer(v) => {
                if live {
                    set_footer(s, name, v);
                }
            }
            ExtensionMessage::Secret {
                id: request,
                op,
                key,
                value,
            } => self.answer_secret(s, name, id, request, (op, key, value), live),
        }
    }

    /// Der Prozess zu `id`, ob aktuell oder abgelöst.
    fn process_of(s: &State, name: &str, id: u64) -> Option<Arc<ExtensionProcess>> {
        s.procs
            .get(name)
            .filter(|e| e.id == id)
            .or_else(|| s.retired.iter().find(|e| e.id == id))
            .map(|e| Arc::clone(&e.process))
    }

    /// Steuerbefehl aus `kadrell.run`. Jede Anfrage bekommt eine Antwort, sonst wartet die Coroutine in Lua ewig; ein
    /// stoppender Prozess führt nichts mehr aus.
    fn answer_run(
        self: &Arc<Self>,
        s: &mut State,
        name: &str,
        id: u64,
        request: i64,
        argv: Vec<String>,
        live: bool,
    ) {
        let Some(process) = Self::process_of(s, name, id) else {
            return;
        };
        let control = Arc::clone(&self.control);
        std::thread::spawn(move || {
            let r = if live {
                control.run(&argv)
            } else {
                ControlResponse::fail("extension is stopping")
            };
            process.send(HostMessage::Result {
                id: request,
                status: r.status,
                stdout: r.stdout,
                stderr: r.stderr,
            });
        });
    }

    /// Schlüsselbund-Zugriff aus `kadrell.secret`, wie `answer_run` mit garantierter Antwort.
    fn answer_secret(
        self: &Arc<Self>,
        s: &mut State,
        name: &str,
        id: u64,
        request: i64,
        op: (String, String, Option<String>),
        live: bool,
    ) {
        let Some(process) = Self::process_of(s, name, id) else {
            return;
        };
        let secrets = Arc::clone(&self.secrets);
        let account = self.settings.account(name, &format!("ext-secret/{}", op.1));
        std::thread::spawn(move || {
            let (status, stdout) = if live {
                secret_op(&*secrets, &account, &op.0, op.2.as_deref())
            } else {
                (1, String::new())
            };
            process.send(HostMessage::Result {
                id: request,
                status,
                stdout,
                stderr: String::new(),
            });
        });
    }

    fn set_panel(
        self: &Arc<Self>,
        s: &mut State,
        name: &str,
        tree: Option<Value>,
        dock: Option<String>,
    ) {
        s.dirty = true;
        let Some(tree) = tree else {
            s.trees.remove(name);
            s.docks.remove(name);
            return;
        };
        match panel::tree(&tree) {
            Ok((t, warnings)) => {
                let mut seen = HashSet::new();
                for w in warnings.iter().filter(|w| seen.insert((*w).clone())) {
                    s.append(name, w);
                }
                s.trees.insert(name.to_string(), t);
                s.docks
                    .insert(name.to_string(), Dock::placed(dock.as_deref()));
            }
            Err(PanelError::TooLarge(count)) => {
                // Wie ein Absturz: Prozess sofort weg, Neustart nach Backoff.
                self.retire(s, name, true);
                let (count, max) = (count.to_string(), panel::MAX_NODES.to_string());
                let reason = tr_args(
                    "Panel zu groß ({0} Knoten, höchstens {1})",
                    "Panel too large ({0} nodes, at most {1})",
                    &[&count, &max],
                );
                self.feed(
                    s,
                    name,
                    SupervisorInput::Exited {
                        expected: false,
                        reason,
                    },
                );
            }
        }
    }
}

fn set_status(s: &mut State, name: &str, v: Option<Value>) {
    let item = v.as_ref().and_then(panel::status);
    if v.is_some() && item.is_none() {
        s.append(name, &panel::missing_field_warning("status", "text"));
    }
    match item {
        Some(i) => s.statuses.insert(name.to_string(), i),
        None => s.statuses.remove(name),
    };
    s.dirty = true;
}

/// Wie beim Panel: Warnungen einmal je Text ins Log, die Liste ersetzt immer die ganze vorige.
fn set_footer(s: &mut State, name: &str, v: Option<Value>) {
    let result = footer::footer(v.as_ref());
    let mut warnings: Vec<String> = result
        .warnings
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    warnings.sort();
    for w in warnings {
        s.append(name, &w);
    }
    if result.items.is_empty() {
        s.footers.remove(name);
    } else {
        s.footers.insert(name.to_string(), result.items);
    }
    s.dirty = true;
}

/// Schlüsselbund-Zugriff für `kadrell.secret`, pro Profil und Extension genamespaced (eigenes Konto-Präfix, kollidiert nicht
/// mit den `secret`-Einstellungen). `get`: Status 0 und Wert, 1 wenn der Eintrag fehlt.
fn secret_op(
    store: &dyn SecretStore,
    account: &str,
    op: &str,
    value: Option<&str>,
) -> (i32, String) {
    match op {
        "get" => store
            .read(KEYCHAIN_SERVICE, account)
            .map_or((1, String::new()), |v| (0, v)),
        "set" => (
            if store.write(KEYCHAIN_SERVICE, account, value.unwrap_or("")) {
                0
            } else {
                1
            },
            String::new(),
        ),
        "delete" => {
            store.delete(KEYCHAIN_SERVICE, account);
            (0, String::new())
        }
        _ => (1, String::new()),
    }
}

/// Nur Lua-Dateien und Manifest: schreibt eine Extension eigene Dateien in ihren Ordner, lädt sie nicht endlos neu.
fn stamp(dir: &Path) -> HashMap<PathBuf, Option<SystemTime>> {
    let mut files = manifest::lua_files(dir);
    files.push(dir.join("kadrell.json"));
    files
        .into_iter()
        .map(|f| {
            (
                f.clone(),
                std::fs::metadata(&f).and_then(|m| m.modified()).ok(),
            )
        })
        .collect()
}
