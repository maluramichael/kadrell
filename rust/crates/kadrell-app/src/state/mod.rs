//! Zustand der App ohne Oberfläche: Profil, Sessions und Gruppen, Einstellungen, Auswahl, die Kacheln mit ihren
//! Prozessen, Status und Marke „neu“. `Shell` hält genau einen `AppState`; der Steuerkanal (`handler`) arbeitet auf
//! demselben Wert. Alles hier ist ohne GPUI testbar: Terminals stehen hinter [`Term`].

pub mod agents;
pub mod attention;
pub mod cli_check;
pub mod feedback;
mod handler;
mod poll;
pub mod stats;
pub mod launch;
pub mod mapping;
mod tiles;
mod workspace;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use kadrell_claude::attention::{AttentionSession, AttentionTracker, Effect, UpdateInput};
use kadrell_claude::registry::{Change, SessionRegistry};
use kadrell_control::LayoutMode;
use kadrell_control::user_hooks::{self, HookEvent};
use kadrell_core::profile::InstanceLock;
use kadrell_core::{Group, Paths, Profile, Session, SessionStatus, SidebarSort, Store};
use serde_json::json;

use self::attention::{ProfileAttention, patch_setting, patch_settings};
use self::launch::LaunchEnv;
use self::tiles::Tile;
pub use self::tiles::{Run, Term};
use kadrell_core::Settings;

pub fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

pub fn now_ms() -> f64 {
    now_secs() * 1000.0
}

/// Alles, was `AppState::new` braucht. Das Binary füllt es aus der Umgebung, Tests mit einem Wegwerfordner.
pub struct Parts {
    pub paths: Paths,
    pub profile: Profile,
    pub lock: Option<InstanceLock>,
    pub env: LaunchEnv,
    /// Endpunkt des Steuerkanals (Socket bzw. Endpunktdatei).
    pub endpoint: PathBuf,
}

pub struct AppState {
    pub paths: Paths,
    pub profile: Profile,
    _lock: Option<InstanceLock>,
    pub store: Store,
    pub settings: Settings,
    pub env: LaunchEnv,
    pub endpoint: PathBuf,
    /// Lokale Modelle (Ollama, llama.cpp) für Pi-Sessions, Schnellstart und Sessiontitel.
    pub models: kadrell_models::LocalModels,
    /// Hintergrundarbeit vor dem Start von Pi- und Remote-Kacheln (`agents`).
    prep: agents::Preparations,
    pub registry: Arc<Mutex<SessionRegistry>>,
    pub attention: AttentionTracker<ProfileAttention>,
    tiles: HashMap<String, Tile>,
    /// Erste Nachricht je neuer Session, gebraucht beim ersten Start.
    prompts: HashMap<String, String>,
    /// Sessions, deren Prozess die Oberfläche als Nächstes starten soll.
    wanted: Vec<String>,
    pub selected: Vec<String>,
    pub focused: Option<String>,
    /// Die Kachel, die vor dem letzten Fokuswechsel den Fokus hatte (⌥⇥).
    pub last_focused: Option<String>,
    pub anchor: Option<String>,
    pub layout: LayoutMode,
    pub zoom: bool,
    pub collapsed: HashSet<String>,
    /// Steuerkanal und Oberfläche sind bereit (`STARTING_STATUS` davor).
    pub ready: bool,
    pub quit_requested: bool,
    /// Das Fenster ist vorn (für Marke „neu“ und Klang).
    pub window_active: bool,
    /// Letzter Fehler zum Anzeigen (claude fehlt, Start schlägt fehl).
    pub error: Option<String>,
    /// Wirkungen des Aufmerksamkeits-Trackers (Klang, Benachrichtigung, Hüpfen), die die Plattformschicht noch
    /// ausführen muss (`take_effects`).
    pending_effects: Vec<Effect>,
    /// claude läuft, ist aber älter als getestet: nicht blockierend, nur die Leiste warnt (`cli_check`).
    pub version_warning: Option<String>,
    /// Letzte Ansage („<Titel> wartet“), als Beschreibung am Status-Display der Leiste (`tray_wire::feedback`).
    pub announcement: Option<String>,
    /// Klänge für Öffnen, Schließen und Umschalten, die die Oberfläche noch spielen muss (`feedback`).
    pending_sounds: Vec<kadrell_tray::Sound>,
}

impl AppState {
    pub fn new(parts: Parts) -> AppState {
        let Parts { paths, profile, lock, env, endpoint } = parts;
        let home = paths.home.to_string_lossy().replace('\\', "/");
        let mut store = profile.open_store(&home);
        let settings = profile.settings();
        let records = store.sessions.sessions.iter().map(mapping::record_of).collect();
        let closed = Vec::new();
        let registry = Arc::new(Mutex::new(SessionRegistry::new(records, closed)));
        registry.lock().unwrap_or_else(PoisonError::into_inner).show_last_message = settings.show_last_message();
        store.assign();
        let attention = AttentionTracker::new(ProfileAttention { path: profile.settings_path() });
        let models = kadrell_models::LocalModels::new(
            Arc::new(kadrell_models::UreqHttp),
            kadrell_models::LocalSettings::from_settings(&settings),
            env.base.environment.clone(),
            &home,
            Some(profile.dir.clone()),
        );
        models.sweep_stale();
        let mut state = AppState {
            paths,
            profile,
            _lock: lock,
            store,
            settings,
            env,
            endpoint,
            models,
            prep: Default::default(),
            registry,
            attention,
            tiles: HashMap::new(),
            prompts: HashMap::new(),
            wanted: Vec::new(),
            selected: Vec::new(),
            focused: None,
            last_focused: None,
            anchor: None,
            layout: LayoutMode::Grid,
            zoom: false,
            collapsed: HashSet::new(),
            ready: false,
            quit_requested: false,
            window_active: true,
            error: None,
            pending_effects: Vec::new(),
            version_warning: None,
            announcement: None,
            pending_sounds: Vec::new(),
        };
        state.restore_view();
        state
    }

    pub fn home(&self) -> String {
        self.paths.home.to_string_lossy().replace('\\', "/")
    }

    // MARK: Ansicht aus den Einstellungen

    /// Gespeicherte Auswahl und Layout (`workspace.selected`, `workspace.mode`); ohne Auswahl zeigt die erste Session.
    fn restore_view(&mut self) {
        let known: HashSet<&str> = self.store.sessions.sessions.iter().map(|s| s.id.as_str()).collect();
        let keys: Vec<String> = self.settings.strings_or("workspace.selected").into_iter().filter(|k| known.contains(k.as_str())).collect();
        let focus = self.settings.opt_string("workspace.selected.focus").filter(|f| keys.contains(f));
        self.layout = LayoutMode::from_raw(&self.settings.string_or("workspace.mode", "grid")).unwrap_or_default();
        let keys = if keys.is_empty() { self.ordered_keys().into_iter().take(1).collect() } else { keys };
        let focus = focus.or_else(|| keys.first().cloned());
        self.set_selection(keys, focus);
    }

    fn save_view(&self) {
        let path = self.profile.settings_path();
        patch_setting(&path, "workspace.selected", json!(self.selected));
        patch_setting(&path, "workspace.selected.focus", json!(self.focused));
        self.save_layout();
    }

    pub fn save_layout(&self) {
        patch_setting(&self.profile.settings_path(), "workspace.mode", json!(self.layout.as_str()));
    }

    /// Alle Sessions in Baumreihenfolge (Gruppe für Gruppe, wie `groups.json` sie hält).
    pub fn ordered_keys(&self) -> Vec<String> {
        self.store.groups.ordered_keys().into_iter().filter(|k| self.store.sessions.get(k).is_some()).collect()
    }

    // MARK: Auswahl

    /// Setzt Auswahl und Fokus. Unbekannte Schlüssel fallen weg, der Fokus gehört immer zur Auswahl. Sichtbare
    /// Sessions ohne Prozess werden zum Start vorgemerkt (Lazy Start) und gelten als gesehen.
    pub fn set_selection(&mut self, keys: Vec<String>, focus: Option<String>) {
        let mut seen = HashSet::new();
        let keys: Vec<String> = keys.into_iter().filter(|k| self.store.sessions.get(k).is_some() && seen.insert(k.clone())).collect();
        let focus = focus.filter(|f| keys.contains(f)).or_else(|| keys.last().cloned());
        let changed = self.selected != keys || self.focused != focus;
        self.count_focus_switch(&self.focused.clone(), &focus);
        if self.focused.is_some() && self.focused != focus {
            self.last_focused = self.focused.clone();
        }
        self.selected = keys;
        self.focused = focus;
        if self.anchor.as_ref().is_none_or(|a| !self.selected.contains(a)) {
            self.anchor = self.focused.clone();
        }
        self.queue_visible_starts();
        if let Some(f) = self.focused.clone() {
            self.attention.mark_seen(&f);
        }
        if changed {
            self.save_view();
        }
    }

    /// Nur diese Session zeigen.
    pub fn show_only(&mut self, key: &str) {
        self.set_selection(vec![key.to_string()], Some(key.to_string()));
        self.anchor = Some(key.to_string());
    }

    /// Fokus auf eine sichtbare Kachel, die Auswahl bleibt.
    pub fn focus_key(&mut self, key: &str) {
        if self.selected.iter().any(|k| k == key) {
            self.set_selection(self.selected.clone(), Some(key.to_string()));
        }
    }

    /// Setzt die Auswahl (`add` falsch) oder schaltet jede Session um (`add` wahr): die letzte bleibt immer.
    pub fn select_keys(&mut self, keys: &[String], add: bool) {
        if !add {
            let focus = keys.first().cloned();
            return self.set_selection(keys.to_vec(), focus);
        }
        let mut selected = self.selected.clone();
        let mut focus = self.focused.clone();
        for k in keys {
            if let Some(i) = selected.iter().position(|x| x == k).filter(|_| selected.len() > 1) {
                selected.remove(i);
                if focus.as_deref() == Some(k) {
                    focus = selected.last().cloned();
                }
            } else if !selected.contains(k) {
                selected.push(k.clone());
                focus = Some(k.clone());
            }
        }
        let order = self.ordered_keys();
        selected.sort_by_key(|k| order.iter().position(|o| o == k).unwrap_or(usize::MAX));
        self.set_selection(selected, focus);
    }

    /// Die sichtbare Kachel, die als Nächstes (oder davor) im Baum steht, für Alt+N und Alt+P.
    pub fn step(&self, delta: isize) -> Option<String> {
        let order = self.ordered_keys();
        let at = self.focused.as_ref().and_then(|f| order.iter().position(|k| k == f))? as isize;
        let next = (at + delta).rem_euclid(order.len() as isize) as usize;
        order.get(next).cloned()
    }

    fn queue_visible_starts(&mut self) {
        for key in self.selected.clone() {
            if self.run(&key) == Run::NotStarted && !self.wanted.contains(&key) {
                self.wanted.push(key);
            }
        }
    }

    // MARK: Sessions und Gruppen

    pub fn session(&self, key: &str) -> Option<&Session> {
        self.store.sessions.get(key)
    }

    /// Fügt die Session hinzu (in `group`, sonst die mit gleichem Ordner), meldet sie dem Register und merkt den Start
    /// vor. `show` setzt die Auswahl auf sie. Gibt `false` zurück, wenn es den Schlüssel schon gibt.
    pub fn add_session(&mut self, session: Session, group: Option<&str>, show: bool, prompt: Option<String>) -> bool {
        let key = session.id.clone();
        if !self.store.add_session(session.clone(), group) {
            return false;
        }
        self.registry.lock().unwrap_or_else(PoisonError::into_inner).add(mapping::record_of(&session));
        self.count_opened(session.is_shell());
        self.queue_sound(kadrell_tray::Sound::Open);
        if let Some(prompt) = prompt {
            self.prompts.insert(key.clone(), prompt);
        }
        self.wanted.push(key.clone());
        if show {
            self.show_only(&key);
        }
        self.fire_hook(HookEvent::SessionNew, &key);
        true
    }

    /// Neue Claude-Session im Ordner `cwd`.
    pub fn new_claude_session(&mut self, cwd: &str, group: Option<&str>, show: bool) -> String {
        let session = Session::fresh(cwd, now_ms());
        let key = session.id.clone();
        self.add_session(session, group, show, None);
        key
    }

    /// Neues Terminal ohne Claude im Ordner `cwd`.
    pub fn new_shell_session(&mut self, cwd: &str, show: bool) -> String {
        self.new_shell_session_in(cwd, None, show)
    }

    /// Neues Terminal ohne Claude im Ordner `cwd`, ausdrücklich in `group` (Id): auch wenn zwei Gruppen denselben
    /// Ordner haben, landet es in der gewählten.
    pub fn new_shell_session_in(&mut self, cwd: &str, group: Option<&str>, show: bool) -> String {
        let key = Session::new_shell_id();
        let session = Session::new(&key, cwd, now_ms(), &key, "");
        self.add_session(session, group, show, None);
        key
    }

    /// Beendet Prozesse, entfernt die Sessions und merkt sie in `closed.json`.
    pub fn remove_sessions(&mut self, keys: &[String]) {
        let ids: HashSet<String> = keys.iter().filter(|k| self.store.sessions.get(k).is_some()).cloned().collect();
        if ids.is_empty() {
            return;
        }
        for key in &ids {
            self.fire_hook(HookEvent::SessionRemove, key);
            self.shutdown_tile(key);
            self.tiles.remove(key);
            self.prompts.remove(key);
        }
        self.wanted.retain(|k| !ids.contains(k));
        self.queue_sound(kadrell_tray::Sound::Close);
        self.store.remove_sessions(&ids, now_secs());
        self.registry.lock().unwrap_or_else(PoisonError::into_inner).remove(&ids, now_ms());
        self.store.assign();
        let selected: Vec<String> = self.selected.iter().filter(|k| !ids.contains(*k)).cloned().collect();
        let focus = self.focused.clone().filter(|f| !ids.contains(f));
        self.set_selection(selected, focus);
        if self.selected.is_empty() {
            let first: Vec<String> = self.ordered_keys().into_iter().take(1).collect();
            self.set_selection(first, None);
        }
    }

    pub fn rename_session(&mut self, key: &str, name: &str) {
        self.store.sessions.rename(key, name);
        self.registry.lock().unwrap_or_else(PoisonError::into_inner).rename(key, name);
    }

    pub fn move_sessions(&mut self, keys: &[String], group_id: &str) -> bool {
        self.store.groups.move_sessions(keys, group_id)
    }

    /// Von Hand angelegte Gruppe: bekommt das Herz und bleibt auch leer stehen.
    pub fn add_group(&mut self, mut group: Group) {
        group.favorite = true;
        self.store.groups.add(group);
    }

    /// Beendet und entfernt alle Sessions der Gruppe, dann die Gruppe selbst (auch mit Herz).
    pub fn close_group(&mut self, id: &str) {
        let keys = self.store.groups.group(id).map(|g| g.session_ids.clone()).unwrap_or_default();
        self.remove_sessions(&keys);
        self.store.groups.remove(id);
    }

    pub fn toggle_collapsed(&mut self, group_id: &str) {
        if !self.collapsed.remove(group_id) {
            self.collapsed.insert(group_id.to_string());
        }
    }

    // MARK: Sortierung und Ansicht des Baums

    pub fn sort(&self) -> SidebarSort {
        self.settings.sidebar_sort()
    }

    pub fn grouped(&self) -> bool {
        self.settings.sidebar_grouped()
    }

    /// Übernimmt die Einstellungen aus dem Dialog: nur geänderte und entfernte Schlüssel gehen in die Datei des Profils,
    /// damit Schlüssel, die andere Stellen schreiben (Aufmerksamkeit, Auswahl), nicht überschrieben werden.
    pub fn apply_settings(&mut self, mut new: Settings) {
        stats::keep_live_keys(&mut new, &self.settings);
        let mut changes: Vec<(String, Option<serde_json::Value>)> = new.keys().filter(|k| new.get(k) != self.settings.get(k)).map(|k| (k.clone(), new.get(k).cloned())).collect();
        changes.extend(self.settings.keys().filter(|k| !new.contains(k)).map(|k| (k.clone(), None)));
        self.settings = new;
        self.models.set_settings(kadrell_models::LocalSettings::from_settings(&self.settings));
        if !changes.is_empty() {
            patch_settings(&self.profile.settings_path(), changes);
        }
    }

    /// Schreibt eine Einstellung der Leiste in die Datei (nur diesen Schlüssel) und in den Zustand.
    fn set_sidebar(&mut self, key: &str, value: serde_json::Value) {
        self.settings.set(key, value.clone());
        patch_setting(&self.profile.settings_path(), key, value);
    }

    /// A-Z oder Status ein, noch einmal gedrückt wieder aus (Reihenfolge der Datei).
    pub fn toggle_sort(&mut self, mode: SidebarSort) {
        let next = if self.sort() == mode { SidebarSort::Off } else { mode };
        self.set_sidebar("sidebar.sort", json!(next.raw()));
    }

    /// Von Hand gezogene Reihenfolge der flachen Liste (`sidebar.flatOrder`), getrennt von `groups.json`.
    pub fn set_flat_order(&mut self, order: &[String]) {
        self.set_sidebar("sidebar.flatOrder", json!(order));
    }

    pub fn toggle_grouped(&mut self) {
        let next = !self.grouped();
        self.set_sidebar("sidebar.grouped", json!(next));
    }

    /// Gruppen in Anzeigereihenfolge mit der Sortierung der Leiste angewendet.
    pub fn sorted_groups(&self) -> Vec<Group> {
        let map: HashMap<String, Session> = self.store.sessions.sessions.iter().map(|s| (s.id.clone(), s.clone())).collect();
        self.sort().apply(&self.store.groups.groups, &map)
    }

    // MARK: Status

    /// Zustand für die Anzeige. Ohne laufenden Prozess gilt die Session als getrennt (`None`).
    pub fn display_status(&self, key: &str) -> Option<SessionStatus> {
        if self.run(key) != Run::Running {
            return None;
        }
        self.session(key).map(Session::status)
    }

    pub fn is_unseen(&self, key: &str) -> bool {
        self.attention.unseen().contains(key)
    }

    pub fn running_count(&self) -> usize {
        self.store.sessions.sessions.iter().filter(|s| self.run(&s.id) == Run::Running).count()
    }

    /// Kopiert den Live-Zustand des Registers in die Sessions des Kerns und gleicht die Marke „neu“ ab.
    pub fn apply_registry(&mut self, change: Change) {
        let records = self.registry.lock().unwrap_or_else(PoisonError::into_inner).sessions.clone();
        let mut stored = false;
        for r in &records {
            if let Some(s) = self.store.sessions.get_mut(&r.id) {
                stored |= mapping::apply_record(s, r);
            }
        }
        if stored || change.persist {
            self.store.sessions.save();
        }
        self.update_attention();
    }

    /// Meldung eines Hooks (`kadrell status`) in das Register und von dort in die Session.
    pub fn report(&mut self, key: &str, report: &kadrell_claude::hook::HookReport) {
        let pids = self.pids();
        let change = self.registry.lock().unwrap_or_else(PoisonError::into_inner).report(key, report, &pids, now_ms());
        if let Some(change) = change {
            self.apply_registry(change);
        }
    }

    /// Marke „neu“, Klang und Fokus-Hook nach jeder Änderung. Klang und Benachrichtigung bleiben Aufgabe der Plattform.
    pub fn update_attention(&mut self) {
        let sessions: HashMap<String, AttentionSession> =
            self.store.sessions.sessions.iter().map(|s| (s.id.clone(), AttentionSession { id: s.id.clone(), active_worktree: s.active_worktree.clone() })).collect();
        let statuses: HashMap<String, kadrell_claude::agent::Status> =
            self.store.sessions.sessions.iter().filter(|s| self.run(&s.id) == Run::Running).map(|s| (s.id.clone(), agent_status(s.status()))).collect();
        let waiting: Vec<String> = self.ordered_keys().into_iter().filter(|k| statuses.get(k) == Some(&kadrell_claude::agent::Status::Waiting)).collect();
        let looking: HashSet<String> = self.selected.iter().cloned().collect();
        self.attention.focus_hook_enabled = true;
        let input = UpdateInput { sessions: &sessions, waiting: &waiting, statuses: &statuses, focused: self.focused.as_deref(), looking: Some(&looking), is_key: self.window_active };
        let (_, effects) = self.attention.update(&input);
        self.attention.prune(&sessions);
        for effect in effects {
            match effect {
                Effect::FireFocusHook(key) => self.fire_hook(HookEvent::SessionFocus, &key),
                other => self.pending_effects.push(other),
            }
        }
    }

    /// Die Wirkungen seit dem letzten Abruf (Klang, Benachrichtigung, Hüpfen), in der Reihenfolge des Trackers.
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.pending_effects)
    }

    // MARK: Eigene Hooks des Nutzers

    /// Startet `~/.config/kadrell/hooks/<ereignis>`, falls vorhanden, ohne zu warten.
    pub fn fire_hook(&self, event: HookEvent, key: &str) {
        let Some(session) = self.session(key) else { return };
        let dir = user_hooks::hooks_dir(&self.paths.home);
        let branch = session.branch.clone();
        user_hooks::fire(&dir, event, session, branch.as_deref(), &self.env.tile_env_list());
    }
}

fn agent_status(s: SessionStatus) -> kadrell_claude::agent::Status {
    use kadrell_claude::agent::Status;
    match s {
        SessionStatus::Running => Status::Running,
        SessionStatus::Waiting => Status::Waiting,
        SessionStatus::Idle => Status::Idle,
        SessionStatus::Error => Status::Error,
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod bar_tests;
