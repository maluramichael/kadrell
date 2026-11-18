//! Testhelfer: ein [`MockHandler`], der den Zustand einer App mit echtem `Store` (in einem Temp-Ordner) nachstellt und
//! jeden Handgriff protokolliert, und ein [`TempDir`]. Für die Tests dieses Crates und für die der App.

use crate::command::{LayoutMode, Result, Status};
use crate::handler::{Capture, ControlHandler, NewRequest, ResumeBlocker, RunState, SshHosts};
use kadrell_core::{Group, Session, Store};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

/// Frischer Ordner unter dem Temp-Ordner, verschwindet beim Fallenlassen.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("kadrell-control-test-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("Temp-Ordner");
        TempDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Default for TempDir {
    fn default() -> TempDir {
        TempDir::new()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Nachgestellte App. Die Felder sind offen: Tests setzen den Zustand direkt und lesen `calls` und `writes`.
pub struct MockHandler {
    pub dir: TempDir,
    pub store: Store,
    pub ready: bool,
    pub focused: Option<String>,
    pub selected: Vec<String>,
    pub layout: LayoutMode,
    pub others_allowed: bool,
    pub run: HashMap<String, RunState>,
    pub tiles: Vec<(String, u32)>,
    /// Sessions mit Terminal und ob dort bracketed paste an ist.
    pub paste: HashMap<String, bool>,
    pub captures: HashMap<String, Capture>,
    pub hosts: SshHosts,
    pub blockers: HashMap<String, ResumeBlocker>,
    pub pi: bool,
    pub home: PathBuf,
    /// Jeder Handgriff als Text, in der Reihenfolge des Aufrufs.
    pub calls: Vec<String>,
    /// Bytes an Terminals: Key und Daten.
    pub writes: Vec<(String, String)>,
    pub reported: Vec<(String, Status)>,
    pub new_requests: Vec<NewRequest>,
    pub server_killed: bool,
}

impl Default for MockHandler {
    fn default() -> MockHandler {
        MockHandler::new()
    }
}

impl MockHandler {
    pub fn new() -> MockHandler {
        let dir = TempDir::new();
        let store = Store::open(dir.path(), &dir.path().to_string_lossy());
        MockHandler {
            home: dir.path().join("home"),
            dir,
            store,
            ready: true,
            focused: None,
            selected: Vec::new(),
            layout: LayoutMode::Grid,
            others_allowed: true,
            run: HashMap::new(),
            tiles: Vec::new(),
            paste: HashMap::new(),
            captures: HashMap::new(),
            hosts: SshHosts::default(),
            blockers: HashMap::new(),
            pi: false,
            calls: Vec::new(),
            writes: Vec::new(),
            reported: Vec::new(),
            new_requests: Vec::new(),
            server_killed: false,
        }
    }

    /// Legt eine Session an (Key = sessionId = `key`, Name `name` als Titel), gruppiert nach Ordner, und gibt die
    /// Id ihrer Gruppe zurück.
    pub fn add_session(&mut self, key: &str, cwd: &str, name: &str) -> String {
        self.store.add_session(Session::new(key, cwd, 0.0, key, name), None);
        self.store.groups.group_for_session(key).map(|g| g.id.clone()).expect("Gruppe der Session")
    }

    /// Eine Session mit laufendem Terminal.
    pub fn add_running(&mut self, key: &str, cwd: &str, name: &str, pid: u32) -> String {
        let group = self.add_session(key, cwd, name);
        self.run.insert(key.to_string(), RunState::Running);
        self.paste.insert(key.to_string(), false);
        self.tiles.push((key.to_string(), pid));
        group
    }

    fn log(&mut self, text: String) {
        self.calls.push(text);
    }
}

impl ControlHandler for MockHandler {
    fn ready(&self) -> bool {
        self.ready
    }
    fn store(&self) -> &Store {
        &self.store
    }
    fn tile_pids(&self) -> Vec<(String, u32)> {
        self.tiles.clone()
    }
    fn focused(&self) -> Option<String> {
        self.focused.clone()
    }
    fn selected(&self) -> Vec<String> {
        self.selected.clone()
    }
    fn layout(&self) -> LayoutMode {
        self.layout
    }
    fn others_allowed(&self) -> bool {
        self.others_allowed
    }
    fn run_state(&self, key: &str) -> RunState {
        self.run.get(key).copied().unwrap_or(RunState::Stopped)
    }
    fn home(&self) -> PathBuf {
        self.home.clone()
    }
    fn ssh_hosts(&self) -> SshHosts {
        self.hosts.clone()
    }
    fn resume_blocker(&self, session_id: &str) -> Option<ResumeBlocker> {
        self.blockers.get(session_id).copied()
    }
    fn pi_available(&self) -> bool {
        self.pi
    }
    fn bracketed_paste(&self, key: &str) -> Option<bool> {
        self.paste.get(key).copied()
    }
    fn capture(&self, key: &str, _all: bool) -> Capture {
        self.captures.get(key).cloned().unwrap_or(Capture::Stopped(Vec::new()))
    }

    fn add_group(&mut self, group: Group) {
        self.log(format!("add_group {}", group.name));
        self.store.groups.add(group);
    }
    fn update_group(&mut self, group: Group) {
        self.log(format!("update_group {} {} {} {}", group.id, group.name, group.color, group.favorite));
        self.store.groups.update(group);
    }
    fn move_session(&mut self, key: &str, group_id: &str) {
        self.log(format!("move {key} {group_id}"));
        self.store.groups.move_sessions(&[key.to_string()], group_id);
    }
    fn rename(&mut self, key: &str, name: &str) {
        self.log(format!("rename {key} {name}"));
        self.store.sessions.rename(key, name);
    }
    fn new_session(&mut self, request: NewRequest) -> Result<String> {
        self.log(format!("new {request:?}"));
        self.new_requests.push(request.clone());
        let session = match &request.host {
            Some(host) => Session { host: Some(host.clone()), tmux_session: request.tmux.clone(), ..Session::new(&format!("ssh-{host}"), "", 0.0, &format!("ssh-{host}"), "") },
            None => Session::fresh(&request.cwd, 0.0),
        };
        let key = session.id.clone();
        self.store.add_session(session, request.group.as_deref());
        Ok(key)
    }
    fn select(&mut self, keys: &[String], add: bool) {
        self.log(format!("select {} add={add}", keys.join(",")));
        self.selected = if add { keys.iter().cloned().chain(self.selected.clone()).collect() } else { keys.to_vec() };
    }
    fn focus(&mut self, key: &str) {
        self.log(format!("focus {key}"));
        self.focused = Some(key.to_string());
    }
    fn set_layout(&mut self, mode: LayoutMode) {
        self.log(format!("layout {}", mode.as_str()));
        self.layout = mode;
    }
    fn toggle_zoom(&mut self) {
        self.log("zoom".into());
    }
    fn stop(&mut self, key: &str) {
        self.log(format!("stop {key}"));
        self.run.insert(key.to_string(), RunState::Stopped);
    }
    fn resume(&mut self, key: &str) -> Result<()> {
        self.log(format!("resume {key}"));
        self.run.insert(key.to_string(), RunState::Running);
        Ok(())
    }
    fn kill(&mut self, key: &str) {
        self.log(format!("kill {key}"));
        self.store.remove_sessions(&std::iter::once(key.to_string()).collect(), 0.0);
    }
    fn kill_group(&mut self, group_id: &str) {
        self.log(format!("kill_group {group_id}"));
    }
    fn terminal_write(&mut self, key: &str, data: &str) {
        self.writes.push((key.to_string(), data.to_string()));
    }
    fn report_status(&mut self, key: &str, status: &Status) {
        self.reported.push((key.to_string(), status.clone()));
    }
    fn kill_server(&mut self) {
        self.server_killed = true;
    }
}
