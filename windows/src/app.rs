//! Zustand der laufenden App: Gruppen und Sessions (`Store`), je laufender Session ein Terminal (`Pane`), Fokus.
//! Führt die Steuerbefehle aus wie `AppDelegate+Control.swift` auf dem Mac, mit denselben Meldungen.

use crate::command::{self, Command, ControlError, NewSession, Result, Status};
use crate::hook;
use crate::profile::{self, Profile, Settings};
use crate::protocol::{ControlRequest, ControlResponse};
use crate::pty::{Pane, Spawn};
use crate::store::{self, Session, Store};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

/// Umgebungsvariablen einer aufrufenden Claude-Session, die sonst an die Kinder vererbt würden: Claude Code
/// speichert dann kein Transcript und `--resume` findet nichts (wie `ClaudeCLI` auf dem Mac).
const SESSION_MARKERS: [&str; 10] = [
    "CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET", "CLAUDE_CODE_MESSAGING_TOKEN", "CLAUDE_CODE_EXECPATH", "CLAUDE_PID", "CLAUDE_JOB_DIR",
];

pub struct Tile {
    pub pane: Pane,
    pub alive: bool,
}

pub struct App {
    pub profile: Profile,
    pub settings: Settings,
    pub store: Store,
    pub tiles: HashMap<String, Tile>,
    /// Prozess hat sich selbst beendet (`/exit`, Absturz), nicht über `stop` oder `kill`.
    pub ended: HashSet<String>,
    pub focused: Option<String>,
    pub dirty: Arc<AtomicBool>,
    /// Größe der Kachel in Zeichen, neue Terminals starten damit.
    pub tile_size: (u16, u16),
    pending_prompts: HashMap<String, String>,
    exe: String,
    /// `kadrell kill-server`: die Ereignisschleife endet nach der Antwort.
    pub quit_requested: bool,
}

impl App {
    pub fn new(profile: Profile) -> App {
        let store = Store::load(&profile.dir);
        let focused = store.ordered_keys().into_iter().next();
        let exe = std::env::current_exe().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| "kadrell".into());
        App {
            settings: profile.settings(),
            profile,
            store,
            tiles: HashMap::new(),
            ended: HashSet::new(),
            focused,
            dirty: Arc::new(AtomicBool::new(true)),
            tile_size: (30, 100),
            pending_prompts: HashMap::new(),
            exe,
            quit_requested: false,
        }
    }

    pub fn is_running(&self, key: &str) -> bool {
        self.tiles.get(key).is_some_and(|t| t.alive)
    }

    pub fn status_of(&self, s: &Session) -> &'static str {
        if self.is_running(&s.id) {
            s.status()
        } else if self.ended.contains(&s.id) {
            "ended"
        } else {
            "stopped"
        }
    }

    // MARK: Prozesse

    fn claude_binary(&self) -> String {
        if !self.settings.claude_binary.is_empty() {
            return self.settings.claude_binary.clone();
        }
        find_in_path("claude").unwrap_or_else(|| "claude".into())
    }

    pub fn launch(&mut self, key: &str) -> Result<()> {
        let s = self.store.session(key).cloned().ok_or_else(|| ControlError::new(format!("keine Session „{key}“")))?;
        let has_transcript = transcript_exists(&s.session_id);
        let mut args = if has_transcript { vec!["--resume".into(), s.session_id.clone()] } else { vec!["--session-id".into(), s.session_id.clone()] };
        args.extend(self.settings.claude.launch_args(s.claude_model.as_deref(), s.claude_effort.as_deref()));
        args.extend(hook::launch_args());
        if let Some(prompt) = self.pending_prompts.remove(key).filter(|_| !has_transcript) {
            args.extend(["--".into(), prompt]);
        }
        let spec = Spawn {
            program: self.claude_binary(),
            args,
            cwd: s.cwd.clone(),
            env: self.child_env(key),
            remove_env: SESSION_MARKERS.iter().map(|m| m.to_string()).collect(),
            rows: self.tile_size.0,
            cols: self.tile_size.1,
        };
        let pane = Pane::spawn(spec, self.dirty.clone()).map_err(|e| ControlError::new(format!("claude startet nicht: {e}")))?;
        self.tiles.insert(key.to_string(), Tile { pane, alive: true });
        self.ended.remove(key);
        if let Some(s) = self.store.session_mut(key) {
            s.raw_status = None;
        }
        Ok(())
    }

    /// Wie `$TMUX_PANE`: `kadrell` in dieser Session weiß, wo es läuft, und findet den Endpunkt.
    fn child_env(&self, key: &str) -> Vec<(String, String)> {
        let mut env = vec![
            ("KADRELL_SESSION_KEY".to_string(), key.to_string()),
            ("KADRELL_SOCKET".to_string(), self.profile.endpoint_path().to_string_lossy().replace('\\', "/")),
            ("KADRELL".to_string(), self.exe.clone()),
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
        ];
        if self.settings.claude.fullscreen {
            env.push(("CLAUDE_CODE_NO_FLICKER".into(), "1".into()));
        }
        env
    }

    /// Fokus wechseln; eine noch nie gestartete Session startet dabei (wie ein Klick im Baum auf dem Mac).
    pub fn focus(&mut self, key: &str) {
        self.focused = Some(key.to_string());
        // Scheitert der Start, gilt die Session als beendet, sonst versucht die Schleife es bei jedem Bild erneut.
        if !self.tiles.contains_key(key) && !self.ended.contains(key) && self.launch(key).is_err() {
            self.ended.insert(key.to_string());
        }
        self.mark_dirty();
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Beendete Prozesse erkennen; die ConPTY meldet das nicht von selbst.
    pub fn poll_exits(&mut self) {
        let mut changed = false;
        for (key, tile) in self.tiles.iter_mut().filter(|(_, t)| t.alive) {
            if tile.pane.exited().is_some() {
                tile.alive = false;
                self.ended.insert(key.clone());
                changed = true;
            }
        }
        if changed {
            self.mark_dirty();
        }
    }

    pub fn resize_tiles(&mut self, rows: u16, cols: u16) {
        self.tile_size = (rows, cols);
        for tile in self.tiles.values_mut().filter(|t| t.alive) {
            tile.pane.resize(rows, cols);
        }
    }

    pub fn stop(&mut self, key: &str) {
        if let Some(t) = self.tiles.get_mut(key).filter(|t| t.alive) {
            t.pane.kill();
            t.alive = false;
        }
        self.ended.remove(key);
        self.mark_dirty();
    }

    pub fn kill_session(&mut self, key: &str) {
        self.stop(key);
        self.tiles.remove(key);
        let order = self.store.ordered_keys();
        let index = order.iter().position(|k| k == key);
        self.store.remove_session(key);
        self.store.save();
        if self.focused.as_deref() == Some(key) {
            let rest = self.store.ordered_keys();
            self.focused = index.and_then(|i| rest.get(i.min(rest.len().saturating_sub(1))).cloned());
        }
        self.mark_dirty();
    }

    pub fn shutdown(&mut self) {
        for tile in self.tiles.values_mut().filter(|t| t.alive) {
            tile.pane.kill();
        }
        self.store.save();
    }

    pub fn rename(&mut self, key: &str, name: &str) {
        if let Some(s) = self.store.session_mut(key) {
            s.custom_name = (!name.trim().is_empty()).then(|| name.trim().to_string());
        }
        self.store.save();
        self.mark_dirty();
    }

    /// Neue Session im Ordner `cwd`, in `group` oder der Gruppe des Ordners. Gibt den Key zurück.
    pub fn start_session(&mut self, group: Option<usize>, cwd: &str, show: bool, prompt: Option<String>, model: Option<String>, effort: Option<String>) -> Result<String> {
        let s = Session::new(cwd, model, effort);
        let key = s.id.clone();
        self.store.add_session(s, group);
        self.store.save();
        if let Some(p) = prompt {
            self.pending_prompts.insert(key.clone(), p);
        }
        let launched = self.launch(&key);
        if show || self.focused.is_none() {
            self.focused = Some(key.clone());
        }
        self.mark_dirty();
        launched.map(|_| key)
    }

    // MARK: Steuerbefehle

    pub fn handle(&mut self, req: &ControlRequest) -> ControlResponse {
        let result = command::parse(&req.argv).and_then(|cmd| self.run(cmd, req));
        self.mark_dirty();
        match result {
            Ok(out) => ControlResponse::ok(out),
            Err(e) => ControlResponse::fail(&e.0),
        }
    }

    fn run(&mut self, cmd: Command, req: &ControlRequest) -> Result<String> {
        match cmd {
            Command::Help => Ok(command::USAGE.to_string()),
            Command::List { json } => Ok(self.list_output(json)),
            Command::NewGroup { dir, name, color } => self.control_new_group(&dir, name, color, req),
            Command::New(n) => self.control_new_session(n, req),
            Command::Capture { target, all } => self.control_capture(target.as_deref(), all, req),
            other => self.perform(other, req).map(|_| String::new()),
        }
    }

    fn perform(&mut self, cmd: Command, req: &ControlRequest) -> Result<()> {
        match cmd {
            Command::Select { target } => {
                let key = self.session(target.as_deref(), req)?;
                self.focus(&key);
            }
            Command::Rename { target, name } => {
                let key = self.session(target.as_deref(), req)?;
                self.rename(&key, &name);
            }
            Command::Move { target, group } => self.control_move(target.as_deref(), &group, req)?,
            Command::SetGroup { target, name, color, favorite } => self.control_set_group(target.as_deref(), name, color, favorite, req)?,
            Command::Stop { target } => self.control_stop(target.as_deref(), req)?,
            Command::Resume { target } => {
                let key = self.session(target.as_deref(), req)?;
                if !self.is_running(&key) {
                    self.launch(&key)?;
                }
            }
            Command::Kill { target } => {
                let key = self.session(target.as_deref(), req)?;
                self.kill_session(&key);
            }
            Command::KillGroup { target } => self.control_kill_group(target.as_deref(), req)?,
            Command::Send { target, text, enter, keys } => self.control_send(target.as_deref(), &text, enter, keys, req)?,
            Command::Status(status) => self.control_status(status, req)?,
            Command::KillServer => self.quit_requested = true,
            Command::Help | Command::List { .. } | Command::NewGroup { .. } | Command::New(_) | Command::Capture { .. } => {}
        }
        Ok(())
    }

    /// Ziel einer Session-Aktion, aus einer Kachel heraus nur im erlaubten Bereich.
    fn session(&self, t: Option<&str>, req: &ControlRequest) -> Result<String> {
        let key = self.store.find_session(t, req.caller.as_deref(), self.focused.as_deref())?;
        if !self.allowed_session(&key, req.caller.as_deref()) {
            let title = self.store.session(&key).map(Session::title).unwrap_or_default();
            return Err(ControlError::new(format!("„{title}“ gehört nicht zur Gruppe dieser Session (Einstellung „Sessions dürfen andere Sessions steuern“)")));
        }
        Ok(key)
    }

    fn allowed_session(&self, key: &str, caller: Option<&str>) -> bool {
        let Some(caller) = caller.filter(|c| !self.settings.control_other_sessions && *c != key) else { return true };
        self.store.groups.iter().any(|g| g.session_ids.iter().any(|s| s == caller) && g.session_ids.iter().any(|s| s == key))
    }

    fn group(&self, t: Option<&str>, req: &ControlRequest, restricted: bool) -> Result<usize> {
        let index = self.store.find_group(t, req.caller.as_deref(), self.focused.as_deref())?;
        let g = &self.store.groups[index];
        let allowed = !restricted
            || self.settings.control_other_sessions
            || req.caller.as_deref().is_none_or(|c| g.session_ids.iter().any(|s| s == c));
        if !allowed {
            return Err(ControlError::new(format!("Gruppe „{}“ ist nicht die dieser Session (Einstellung „Sessions dürfen andere Sessions steuern“)", g.name)));
        }
        Ok(index)
    }

    fn existing_dir(&self, p: &str, req: &ControlRequest) -> Result<String> {
        let path = store::resolve_path(p, &req.cwd, &profile::home());
        if Path::new(&path).is_dir() {
            Ok(path)
        } else {
            Err(ControlError::new(format!("kein Ordner: {path}")))
        }
    }

    fn control_new_group(&mut self, dir: &str, name: Option<String>, color: Option<String>, req: &ControlRequest) -> Result<String> {
        let mut g = self.store.make_group(&self.existing_dir(dir, req)?, name);
        if let Some(c) = color {
            g.color = c;
        }
        g.favorite = true;
        let id = g.id.clone();
        self.store.groups.push(g);
        self.store.save();
        Ok(id)
    }

    /// Ohne -t und -c landet die Session bei der aufrufenden: gleiche Gruppe, gleicher Ordner (wie tmux new-window).
    fn control_new_session(&mut self, n: NewSession, req: &ControlRequest) -> Result<String> {
        if let Some(r) = &n.resume {
            self.check_resumable(r)?;
        }
        let mut group = n.target.as_deref().map(|t| self.group(Some(t), req, false)).transpose()?;
        let caller = req.caller.as_deref().and_then(|c| self.store.session(c)).cloned();
        if n.target.is_none() && n.dir.is_none() {
            if let Some(c) = &caller {
                group = self.store.group_of(&c.id);
            }
        }
        let cwd = match (&n.dir, &n.target, &caller, group) {
            (Some(d), _, _, _) => self.existing_dir(d, req)?,
            (None, None, Some(c), _) => c.cwd.clone(),
            (None, _, _, Some(g)) => self.store.groups[g].cwd.clone(),
            _ => self.existing_dir(&req.cwd, req)?,
        };
        let key = match n.resume {
            Some(sid) => self.start_resumed(group, &cwd, &sid, n.model, n.effort, !n.detached)?,
            None => self.start_session(group, &cwd, !n.detached, n.prompt, n.model, n.effort)?,
        };
        if let Some(name) = n.name {
            self.rename(&key, &name);
        }
        Ok(key)
    }

    fn start_resumed(&mut self, group: Option<usize>, cwd: &str, sid: &str, model: Option<String>, effort: Option<String>, show: bool) -> Result<String> {
        let mut s = Session::new(cwd, model, effort);
        s.id = sid.to_string();
        s.session_id = sid.to_string();
        self.store.add_session(s, group);
        self.store.save();
        self.launch(sid)?;
        if show || self.focused.is_none() {
            self.focused = Some(sid.to_string());
        }
        Ok(sid.to_string())
    }

    fn check_resumable(&self, sid: &str) -> Result<()> {
        if self.store.sessions.iter().any(|s| s.session_id == sid || s.id == sid) {
            return Err(ControlError::new(format!("{sid} ist schon in Kadrell")));
        }
        if !transcript_exists(sid) {
            return Err(ControlError::new(format!("kein Transcript für {sid}")));
        }
        Ok(())
    }

    fn control_move(&mut self, t: Option<&str>, group: &str, req: &ControlRequest) -> Result<()> {
        let key = self.session(t, req)?;
        let g = self.group(Some(group), req, true)?;
        self.store.move_session(&key, g);
        self.store.save();
        Ok(())
    }

    fn control_set_group(&mut self, t: Option<&str>, name: Option<String>, color: Option<String>, favorite: Option<bool>, req: &ControlRequest) -> Result<()> {
        let index = self.group(t, req, true)?;
        let g = &mut self.store.groups[index];
        g.name = name.unwrap_or(g.name.clone());
        g.color = color.unwrap_or(g.color.clone());
        if let Some(f) = favorite {
            g.favorite = f;
        }
        if g.session_ids.is_empty() && !g.favorite {
            self.store.groups.remove(index);
        }
        self.store.save();
        Ok(())
    }

    fn control_stop(&mut self, t: Option<&str>, req: &ControlRequest) -> Result<()> {
        let key = self.session(t, req)?;
        if !self.is_running(&key) {
            return Err(ControlError::new(format!("„{}“ läuft nicht", self.title(&key))));
        }
        self.stop(&key);
        Ok(())
    }

    fn control_kill_group(&mut self, t: Option<&str>, req: &ControlRequest) -> Result<()> {
        let index = self.group(t, req, true)?;
        let id = self.store.groups[index].id.clone();
        for key in self.store.groups[index].session_ids.clone() {
            self.kill_session(&key);
        }
        self.store.groups.retain(|g| g.id != id);
        self.store.save();
        Ok(())
    }

    /// Text geht als Einfügen (bracketed paste), wenn das Programm es eingeschaltet hat; ⏎ kommt getrennt hinterher,
    /// sonst hält die Eingabe es für einen Zeilenumbruch im eingefügten Text.
    fn control_send(&mut self, t: Option<&str>, text: &str, enter: bool, keys: bool, req: &ControlRequest) -> Result<()> {
        let key = self.session(t, req)?;
        let Some(tile) = self.tiles.get(&key).filter(|t| t.alive) else {
            return Err(ControlError::new(format!("„{}“ läuft nicht, erst kadrell resume -t {}", self.title(&key), &key[..8.min(key.len())])));
        };
        if keys {
            let bytes: String = text.split(' ').filter_map(command::key_bytes).collect();
            tile.pane.write(bytes.as_bytes());
            return Ok(());
        }
        let clean = text.replace("\u{1b}[201~", "");
        let data = if tile.pane.bracketed_paste() { format!("\u{1b}[200~{clean}\u{1b}[201~") } else { clean };
        tile.pane.write(data.as_bytes());
        if enter {
            std::thread::sleep(Duration::from_millis(150));
            tile.pane.write(b"\r");
        }
        Ok(())
    }

    fn control_capture(&self, t: Option<&str>, all: bool, req: &ControlRequest) -> Result<String> {
        let key = self.session(t, req)?;
        Ok(self.tiles.get(&key).map(|tile| tile.pane.capture(all)).unwrap_or_default())
    }

    /// Ohne -t meldet sich die aufrufende Kachel selbst; von außen muss das Ziel genannt sein.
    fn control_status(&mut self, st: Status, req: &ControlRequest) -> Result<()> {
        if st.target.is_none() && req.caller.is_none() {
            return Err(ControlError::new("status braucht -t, wenn es nicht aus einer Session kommt"));
        }
        let key = self.session(st.target.as_deref(), req)?;
        let Some(s) = self.store.session_mut(&key) else { return Ok(()) };
        let persisted_before = (s.session_id.clone(), s.name.clone());
        s.raw_status = Some(st.state.clone());
        s.waiting_for = if st.state == "waiting" { st.waiting_for } else { None };
        if let Some(sid) = st.session_id.filter(|v| !v.is_empty()) {
            s.session_id = sid;
        }
        if let Some(title) = st.title.filter(|v| !v.is_empty()) {
            s.name = title;
        }
        if s.first_prompt.is_none() {
            s.first_prompt = st.first_prompt.filter(|p| !p.is_empty());
        }
        if st.message.is_some() {
            s.message = st.message;
        }
        if persisted_before != (s.session_id.clone(), s.name.clone()) {
            self.store.save();
        }
        Ok(())
    }

    fn title(&self, key: &str) -> String {
        self.store.session(key).map(Session::title).unwrap_or_else(|| key.to_string())
    }

    /// Dieselbe Form wie `kadrell ls` auf dem Mac, damit Skripte (start-my-day) auf beiden Systemen laufen.
    pub fn list_output(&self, as_json: bool) -> String {
        let groups: Vec<serde_json::Value> = self
            .store
            .groups
            .iter()
            .map(|g| {
                let sessions: Vec<serde_json::Value> = g.session_ids.iter().filter_map(|k| self.store.session(k)).map(|s| self.session_json(s)).collect();
                json!({"id": g.id, "name": g.name, "color": g.color, "cwd": g.cwd, "favorite": g.favorite, "sessions": sessions})
            })
            .collect();
        if as_json {
            let out = json!({"layout": "stack", "focused": self.focused, "groups": groups});
            return serde_json::to_string_pretty(&out).unwrap_or_default();
        }
        groups.iter().map(text_group).collect::<Vec<_>>().join("\n")
    }

    fn session_json(&self, s: &Session) -> serde_json::Value {
        let running = self.is_running(&s.id);
        let mut v = json!({
            "key": s.id, "sessionId": s.session_id, "title": s.title(), "cwd": s.cwd, "status": self.status_of(s),
            "running": running, "shown": self.focused.as_deref() == Some(s.id.as_str()), "focused": self.focused.as_deref() == Some(s.id.as_str()),
        });
        if let Some(b) = git_branch(&s.cwd) {
            v["branch"] = json!(b);
        }
        v
    }
}

fn text_group(g: &serde_json::Value) -> String {
    let short = |v: &serde_json::Value| v.as_str().unwrap_or("").chars().take(8).collect::<String>();
    let fav = if g["favorite"].as_bool() == Some(true) { " ★" } else { "" };
    let mut lines = vec![format!("{}  {}{}  {}", short(&g["id"]), g["name"].as_str().unwrap_or(""), fav, g["cwd"].as_str().unwrap_or(""))];
    for s in g["sessions"].as_array().into_iter().flatten() {
        let mark = if s["focused"].as_bool() == Some(true) { "*" } else { " " };
        lines.push(format!("  {mark} {}  {:<8}  {}", short(&s["key"]), s["status"].as_str().unwrap_or(""), s["title"].as_str().unwrap_or("")));
    }
    lines.join("\n")
}

/// Datenordner von Claude Code (`~/.claude`, per `CLAUDE_CONFIG_DIR` verlegbar).
pub fn claude_config_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).unwrap_or_else(|| profile::home().join(".claude"))
}

/// Ohne erste Nachricht gibt es kein Transcript, `--resume` scheiterte dann; deshalb vorher nachsehen.
fn transcript_exists(session_id: &str) -> bool {
    let Ok(dirs) = std::fs::read_dir(claude_config_dir().join("projects")) else { return false };
    !session_id.is_empty() && dirs.flatten().any(|d| d.path().join(format!("{session_id}.jsonl")).is_file())
}

pub fn find_in_path(name: &str) -> Option<String> {
    let exts = std::env::var("PATHEXT").unwrap_or_else(|_| ".EXE;.CMD;.BAT".into());
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        std::iter::once(String::new())
            .chain(exts.split(';').map(str::to_lowercase))
            .map(|ext| dir.join(format!("{name}{ext}")))
            .find(|p| p.is_file() && p.extension().is_some())
            .map(|p| p.to_string_lossy().to_string())
    })
}

/// Branch aus `.git/HEAD`, auch in Worktrees (`.git` ist dort eine Datei mit `gitdir:`).
fn git_branch(cwd: &str) -> Option<String> {
    let dot_git = Path::new(cwd).join(".git");
    let git_dir = if dot_git.is_file() {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        PathBuf::from(text.strip_prefix("gitdir:")?.trim())
    } else {
        dot_git
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    head.trim().strip_prefix("ref: refs/heads/").map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let dir = std::env::temp_dir().join(format!("kadrell-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        App::new(Profile { dir, temporary: true })
    }

    fn req(argv: &[&str]) -> ControlRequest {
        ControlRequest { argv: argv.iter().map(|s| s.to_string()).collect(), cwd: std::env::temp_dir().to_string_lossy().into(), caller: None, token: String::new() }
    }

    /// Schlüssel rekursiv, Arrays über ihr erstes Element: dieselbe Form wie `docs/contract/ls.json` (Mac-Ausgabe).
    fn shape(v: &serde_json::Value) -> serde_json::Value {
        match v {
            serde_json::Value::Object(m) => serde_json::Value::Object(m.iter().map(|(k, v)| (k.clone(), shape(v))).collect()),
            serde_json::Value::Array(a) => serde_json::Value::Array(a.first().map(shape).into_iter().collect()),
            serde_json::Value::Bool(_) => json!("bool"),
            serde_json::Value::Number(_) => json!("number"),
            serde_json::Value::Null | serde_json::Value::String(_) => json!("string?"),
        }
    }

    #[test]
    fn list_json_matches_contract() {
        let mut a = app();
        let dir = std::env::current_dir().unwrap().parent().unwrap().to_string_lossy().replace('\\', "/");
        let mut s = Session::new(&dir, None, None);
        s.custom_name = Some("_MASTER_".into());
        let key = s.id.clone();
        a.store.add_session(s, None);
        a.store.groups[0].favorite = true;
        a.focused = Some(key);
        let ours: serde_json::Value = serde_json::from_str(&a.list_output(true)).unwrap();
        let contract: serde_json::Value = serde_json::from_slice(&std::fs::read("../docs/contract/ls.json").unwrap()).unwrap();
        assert_eq!(shape(&ours), shape(&contract));
        a.profile.close();
    }

    #[test]
    fn group_commands_and_list_json() {
        let mut a = app();
        let tmp = std::env::temp_dir().to_string_lossy().replace('\\', "/");
        let r = a.handle(&req(&["new-group", &tmp, "--name", "Start"]));
        assert_eq!(r.status, 0, "{}", r.stderr);
        let id = r.stdout.trim().to_string();
        assert_eq!(a.handle(&req(&["set-group", "-t", "start", "--favorite", "off"])).status, 0);
        assert!(a.store.groups.is_empty(), "leere Gruppe ohne Herz verschwindet");
        let r = a.handle(&req(&["set-group", "-t", &id[..8]]));
        assert_eq!(r.stderr, format!("kadrell: keine Gruppe „{}“\n", &id[..8]));
        let v: serde_json::Value = serde_json::from_str(&a.list_output(true)).unwrap();
        assert_eq!(v["groups"], json!([]));
        a.profile.close();
    }

    #[test]
    fn status_and_send_need_a_running_session() {
        let mut a = app();
        let tmp = std::env::temp_dir().to_string_lossy().replace('\\', "/");
        let mut s = Session::new(&tmp, None, None);
        s.custom_name = Some("_MASTER_".into());
        a.store.add_session(s, None);
        assert_eq!(a.handle(&req(&["status", "working"])).stderr, "kadrell: status braucht -t, wenn es nicht aus einer Session kommt\n");
        assert_eq!(a.handle(&req(&["status", "waiting", "-t", "_MASTER_", "--waiting-for", "Bash"])).status, 0);
        let v: serde_json::Value = serde_json::from_str(&a.list_output(true)).unwrap();
        let session = &v["groups"][0]["sessions"][0];
        assert_eq!(session["title"], "_MASTER_");
        assert_eq!(session["status"], "stopped", "nicht gestartet");
        assert_eq!(session["running"], false);
        let r = a.handle(&req(&["send", "-t", "_MASTER_", "hallo"]));
        assert!(r.stderr.contains("läuft nicht, erst kadrell resume"), "{}", r.stderr);
        a.profile.close();
    }
}
