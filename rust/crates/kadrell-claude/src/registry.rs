//! Live-Zustand der Sessions: Abgleich mit `claude agents`/den Session-Dateien über die pid des eigenen Prozesses,
//! Status per Hook-Meldung (`report`), die 5-s-Regel (`report_fallback`), erste Eingabe als Ersatztitel, letzte Antwort
//! und aktiver Worktree (Swift: `Services/SessionRegistry.swift`).
//!
//! Reine Zustandsmaschine ohne Threads und ohne Zeitgeber: der Aufrufer ruft `poll_now` alle `poll_interval()`
//! Sekunden und `report` bei jeder Meldung auf. Alles, was Prozesse oder Dateien anfasst, steht hinter `PollEnv`.
//! Nicht hier: `sessions.json`/`closed.json` lesen und schreiben (gehört in den Kern), Stats, Hooks.fire.

use crate::agent::{self, AgentInfo, Status};
use crate::hook::HookReport;
use crate::report_fallback;
use crate::transcript::{self, Entry as TranscriptEntry};
use crate::worktree;
use std::collections::{HashMap, HashSet};

/// Art der Kachel. Nur Claude-Sessions haben ein Transcript unter `<configDir>/projects`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SessionKind {
    #[default]
    Claude,
    Shell,
    Remote,
    Pi,
}

/// Der Teil von `Session` (Kern), den Poll und Hooks lesen und schreiben. Die gespeicherten Felder tragen dieselben
/// Namen wie im Kern (`id`, `cwd`, `startedAt`, `sessionId`, `name`, `customName`, `localTitle`, `model`).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRecord {
    pub id: String,
    pub cwd: String,
    pub started_at: f64,
    pub session_id: String,
    pub name: String,
    pub custom_name: Option<String>,
    pub local_title: Option<String>,
    pub model: Option<String>,
    pub kind: SessionKind,
    pub raw_status: Option<String>,
    pub pid: Option<u32>,
    pub waiting_for: Option<String>,
    pub branch: Option<String>,
    pub active_worktree: Option<String>,
    pub first_prompt: Option<String>,
    pub has_running_shell: bool,
}

impl SessionRecord {
    pub fn new(id: &str, cwd: &str, started_at: f64, session_id: &str, name: &str) -> SessionRecord {
        SessionRecord {
            id: id.into(),
            cwd: cwd.into(),
            started_at,
            session_id: session_id.into(),
            name: name.into(),
            custom_name: None,
            local_title: None,
            model: None,
            kind: SessionKind::Claude,
            raw_status: None,
            pid: None,
            waiting_for: None,
            branch: None,
            active_worktree: None,
            first_prompt: None,
            has_running_shell: false,
        }
    }

    pub fn status(&self) -> Status {
        agent::map_status(None, self.raw_status.as_deref())
    }

    pub fn has_transcript(&self) -> bool {
        self.kind == SessionKind::Claude
    }

    /// Anzeigetitel wie `Session.title` für Claude-Sessions: eigener Name, lokaler Titel, Claudes Titel, erste Eingabe,
    /// sonst `<ordner> · <4 Zeichen der Id>`.
    pub fn title(&self) -> String {
        if let Some(c) = &self.custom_name {
            return c.clone();
        }
        if let Some(l) = &self.local_title {
            return l.clone();
        }
        if !self.name.is_empty() {
            return self.name.clone();
        }
        self.first_prompt.clone().unwrap_or_else(|| format!("{} · {}", crate::paths::last_component(&self.cwd), self.id.chars().take(4).collect::<String>()))
    }

    /// Was `sessions.json` speichert: ändert sich nur Live-Zustand, muss nicht geschrieben werden.
    fn stored(&self) -> [String; 8] {
        [
            self.id.clone(),
            self.cwd.clone(),
            self.started_at.to_string(),
            self.session_id.clone(),
            self.name.clone(),
            self.custom_name.clone().unwrap_or_default(),
            self.local_title.clone().unwrap_or_default(),
            self.model.clone().unwrap_or_default(),
        ]
    }
}

/// Eine geschlossene Session samt Zeitpunkt (`closed.json`).
#[derive(Clone, Debug, PartialEq)]
pub struct ClosedSession {
    pub session: SessionRecord,
    pub closed_at: f64,
}

pub const MAX_CLOSED: usize = 50;

/// Was nach einer Änderung zu tun ist.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    /// Oberfläche neu zeichnen (`onChange`).
    pub notify: bool,
    /// gespeicherte Felder haben sich geändert: `sessions.json` schreiben.
    pub persist: bool,
}

/// Alles, was der Poll von außen braucht. Tests geben Fixtures, die App gibt Dateien und Prozesse (`FsPollEnv`).
pub trait PollEnv {
    /// pid des eigenen Claude-Prozesses je Session-Schlüssel (liefert der AttachManager bzw. die ConPTY-Verwaltung).
    fn pids(&self) -> HashMap<u32, String>;
    fn local_agents(&self, pids: &[u32]) -> Vec<AgentInfo>;
    fn branch(&self, cwd: &str) -> Option<String>;
    /// Aktueller Ordner der Shell mit dieser pid (Terminals ohne Claude folgen dem `cd`).
    fn shell_cwd(&self, pid: u32) -> Option<String>;
    /// pids, unter denen noch ein Bash-Werkzeug-Kommando läuft (`background_shells`).
    fn running_shells(&self, pids: &[u32]) -> HashSet<u32>;
    /// Erste Eingabe der Konversation. nil = es gibt (noch) kein Transcript; leerer String = Transcript da, aber kein
    /// Titel gefunden (wird gemerkt, damit nicht jeder Poll erneut liest).
    fn first_prompt(&self, session_id: &str) -> Option<String>;
    fn refresh_transcripts(&self, session_ids: &[String], cache: &HashMap<String, TranscriptEntry>, want_text: bool) -> HashMap<String, TranscriptEntry>;
    fn worktrees(&self, cwd: &str) -> Vec<worktree::Entry>;
    /// ms seit 1970.
    fn now_ms(&self) -> f64;
}

#[derive(Default)]
pub struct SessionRegistry {
    pub sessions: Vec<SessionRecord>,
    /// Letzte Textantwort von Claude je Session (`id`), nur wenn `show_last_message` an ist.
    pub last_messages: HashMap<String, String>,
    pub show_last_message: bool,
    /// Binary nicht gefunden, `claude agents` schlägt fehl, Speichern schlägt fehl: kein Poll räumt es wieder ab.
    pub last_error: Option<String>,
    /// `last_error` ist speziell das fehlende Binary: der Leerzustand zeigt dann den Installationsbefehl.
    pub last_error_is_missing_binary: bool,
    /// Ob schon ein Poll durchgelaufen ist, für den Lade-Zustand davor.
    pub polled: bool,
    /// Zuletzt geschlossene Sessions (neueste zuerst, höchstens `MAX_CLOSED`).
    pub closed: Vec<ClosedSession>,
    transcripts: HashMap<String, TranscriptEntry>,
    worktree_cache: HashMap<String, Vec<worktree::Entry>>,
    first_prompts: HashMap<String, String>,
    /// Kacheln, die sich per `report` selbst melden: kein Lesen ihrer Session-Datei, kein Transcript-Scan.
    reported: HashSet<String>,
    reported_at: HashMap<String, f64>,
    hook_messages: HashMap<String, String>,
    transcript_due: HashSet<String>,
    background: bool,
}

pub const POLL_INTERVAL_SECS: f64 = 2.0;
pub const BACKGROUND_POLL_INTERVAL_SECS: f64 = 5.0;

impl SessionRegistry {
    pub fn new(sessions: Vec<SessionRecord>, closed: Vec<ClosedSession>) -> SessionRegistry {
        SessionRegistry { sessions: Self::deduped_by_id(sessions), closed, ..SessionRegistry::default() }
    }

    /// Erste gewinnt: eine doppelte Id (Handbearbeitung, Übernahme mit schon vorhandener Kurz-Id) ließe die
    /// Oberfläche sonst bei jedem Start abstürzen.
    fn deduped_by_id(sessions: Vec<SessionRecord>) -> Vec<SessionRecord> {
        let mut seen = HashSet::new();
        sessions.into_iter().filter(|s| seen.insert(s.id.clone())).collect()
    }

    /// `backgroundInterval` bleibt bewusst nah an `interval`: Sound und Marke „neu“ bei Statuswechseln sollen auch im
    /// Hintergrund zeitnah reagieren.
    pub fn poll_interval(&self) -> f64 {
        if self.background { BACKGROUND_POLL_INTERVAL_SECS } else { POLL_INTERVAL_SECS }
    }

    /// Fenster verdeckt/versteckt oder App im Hintergrund: seltener pollen. true = kommt zurück, sofort neu laden.
    pub fn set_background(&mut self, background: bool) -> bool {
        let poll_now = self.background && !background;
        self.background = background;
        poll_now
    }

    pub fn is_background(&self) -> bool {
        self.background
    }

    /// false = die Id gibt es schon, nichts geändert.
    pub fn add(&mut self, session: SessionRecord) -> bool {
        if self.sessions.iter().any(|s| s.id == session.id) {
            return false;
        }
        self.sessions.push(session);
        true
    }

    /// Leer = wieder der Titel von Claude Code.
    pub fn rename(&mut self, id: &str, name: &str) {
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id) {
            let n = name.trim();
            s.custom_name = if n.is_empty() { None } else { Some(n.to_string()) };
        }
    }

    /// Titel vom lokalen Modell. Ein vorhandener bleibt, ein eigener Name geht ohnehin vor.
    pub fn set_local_title(&mut self, id: &str, title: &str) {
        if let Some(s) = self.sessions.iter_mut().find(|s| s.id == id && s.local_title.is_none()) {
            s.local_title = Some(title.to_string());
        }
    }

    /// Entfernt Sessions und merkt sich ihre sessionId (der Schließen-Dialog verspricht „Konversation bleibt erhalten“).
    pub fn remove(&mut self, ids: &HashSet<String>, now_ms: f64) -> Vec<SessionRecord> {
        let removed: Vec<SessionRecord> = self.sessions.iter().filter(|s| ids.contains(&s.id)).cloned().collect();
        self.sessions.retain(|s| !ids.contains(&s.id));
        let archived = removed.iter().map(|s| ClosedSession { session: s.clone(), closed_at: now_ms / 1000.0 });
        self.closed = archived.chain(self.closed.drain(..)).take(MAX_CLOSED).collect();
        removed
    }

    /// Binary fehlt oder `claude agents` schlägt fehl.
    pub fn fail(&mut self, message: &str, missing_binary: bool) {
        self.last_error = Some(message.into());
        self.last_error_is_missing_binary = missing_binary;
    }

    /// Erfolgreiche Neuprüfung: den alten Fehler wegräumen. true = es gab einen.
    pub fn clear_error(&mut self) -> bool {
        let had = self.last_error.take().is_some();
        self.last_error_is_missing_binary = false;
        had
    }

    /// `kadrell status` aus einem Hook der Session: ab jetzt gilt für diese Kachel das Gemeldete, der Poll lässt Status,
    /// sessionId, Name und Transcript in Ruhe, solange ihr Prozess läuft. nil = Schlüssel unbekannt.
    pub fn report(&mut self, key: &str, report: &HookReport, pids: &HashMap<u32, String>, now_ms: f64) -> Option<Change> {
        let i = self.sessions.iter().position(|s| s.id == key)?;
        let mut s = self.sessions[i].clone();
        s.raw_status = Some(report.state.clone());
        s.waiting_for = if report.state == "waiting" { report.waiting_for.clone() } else { None };
        s.pid = pids.iter().filter(|(_, k)| k.as_str() == key).map(|(p, _)| *p).min().or(s.pid);
        if let Some(id) = report.session_id.as_ref().filter(|v| !v.is_empty()) {
            s.session_id = id.clone();
        }
        if let Some(title) = report.title.as_ref().filter(|v| !v.is_empty()) {
            s.name = title.clone();
        }
        if let Some(fp) = &report.first_prompt {
            self.first_prompts.entry(s.session_id.clone()).or_insert_with(|| fp.clone());
        }
        s.first_prompt = self.first_prompts.get(&s.session_id).filter(|p| !p.is_empty()).cloned();
        if let Some(m) = &report.message {
            self.hook_messages.insert(key.to_string(), m.clone());
            if self.show_last_message {
                self.last_messages.insert(key.to_string(), m.clone());
            }
        }
        // Nach jeder Antwort einmal den Transcript-Tail lesen: nur daraus kommt der aktive Worktree.
        if report.state == "idle" {
            self.transcript_due.insert(key.to_string());
        }
        self.reported.insert(key.to_string());
        self.reported_at.insert(key.to_string(), now_ms);
        let changed = s != self.sessions[i];
        let persist = s.stored() != self.sessions[i].stored();
        self.sessions[i] = s;
        Some(Change { notify: changed || report.message.is_some(), persist })
    }

    /// Ein Poll. `Change::notify` ist beim ersten Poll immer gesetzt: Registrierte hören darauf, um den Lade-Zustand zu
    /// verlassen.
    pub fn poll_now(&mut self, env: &dyn PollEnv) -> Change {
        let pids = env.pids();
        let prior = self.sessions.clone();
        // Gemeldete Kacheln, deren Prozess weg ist, fallen zurück auf den Poll.
        let alive: HashSet<&String> = pids.values().collect();
        self.reported.retain(|k| alive.contains(k));
        self.hook_messages.retain(|k, _| self.reported.contains(k));
        self.reported_at.retain(|k, _| self.reported.contains(k));
        // Gemeldete Kacheln, die nicht untätig sind, lesen ihre Datei trotzdem, nur für den Abgleich (`report_fallback`).
        let unsettled: HashSet<&String> = self.sessions.iter().filter(|s| self.reported.contains(&s.id) && s.status() != Status::Idle).map(|s| &s.id).collect();
        let poll_pids: Vec<u32> = pids.iter().filter(|(_, k)| !self.reported.contains(*k) || unsettled.contains(k)).map(|(p, _)| *p).collect();
        let agents = env.local_agents(&poll_pids);
        let mut merged = merge(&prior, &agents, &pids, &self.reported, &|cwd| env.branch(cwd));
        apply_shell_cwd(&mut merged, &pids, env);
        apply_running_shells(&mut merged, &pids, env);
        self.fill_first_prompts(&mut merged, env);
        let messages = self.refresh_transcripts(&merged, env);
        self.apply_active_worktree(&mut merged, env);
        merged = apply_live(&merged, &self.sessions, &self.reported);
        self.settle_stale_reports(&mut merged, &agents, &pids, env.now_ms());
        let first_poll = !self.polled;
        self.polled = true;
        if merged == self.sessions && messages == self.last_messages && !first_poll {
            return Change::default();
        }
        let persist = merged.iter().map(SessionRecord::stored).ne(self.sessions.iter().map(SessionRecord::stored));
        self.sessions = merged;
        self.last_messages = messages;
        Change { notify: true, persist }
    }

    /// Gemeldete Kacheln, deren Meldung die Session-Datei überholt hat (Abbruch ohne `Stop`-Hook), nehmen deren Stand an.
    fn settle_stale_reports(&mut self, merged: &mut [SessionRecord], agents: &[AgentInfo], pids: &HashMap<u32, String>, now: f64) {
        let mut files: HashMap<&String, &AgentInfo> = HashMap::new();
        for a in agents {
            if let Some(key) = a.pid.and_then(|p| pids.get(&p)) {
                files.entry(key).or_insert(a);
            }
        }
        for s in merged.iter_mut() {
            let Some(at) = self.reported_at.get(&s.id).filter(|_| self.reported.contains(&s.id)) else { continue };
            let Some((raw, waiting_for)) = report_fallback::correction(s.status(), *at, files.get(&s.id).copied(), now) else { continue };
            s.raw_status = Some(raw);
            s.waiting_for = waiting_for;
            if s.status() == Status::Idle {
                self.transcript_due.insert(s.id.clone());
            }
        }
    }

    fn fill_first_prompts(&mut self, merged: &mut [SessionRecord], env: &dyn PollEnv) {
        // Shell-, Remote- und Pi-Sessions haben nie ein Transcript, sonst würde jeder Poll erneut vergeblich scannen.
        let missing: Vec<String> = merged
            .iter()
            .filter(|s| s.name.is_empty() && s.has_transcript() && !self.reported.contains(&s.id) && !self.first_prompts.contains_key(&s.session_id))
            .map(|s| s.session_id.clone())
            .collect();
        for id in missing {
            if let Some(found) = env.first_prompt(&id) {
                self.first_prompts.entry(id).or_insert(found);
            }
        }
        for s in merged.iter_mut() {
            s.first_prompt = self.first_prompts.get(&s.session_id).filter(|p| !p.is_empty()).cloned();
        }
    }

    /// Liest nur gewachsene Transcripts neu und leitet daraus die Nachrichtenzeile ab (falls eingeschaltet).
    fn refresh_transcripts(&mut self, merged: &[SessionRecord], env: &dyn PollEnv) -> HashMap<String, String> {
        // Gemeldete Sessions (Hooks) nur nach einer Antwort (`transcript_due`), nicht bei jedem Poll.
        let ids: Vec<String> =
            merged.iter().filter(|s| s.has_transcript() && (!self.reported.contains(&s.id) || self.transcript_due.contains(&s.id))).map(|s| s.session_id.clone()).collect();
        self.transcript_due.retain(|k| !self.reported.contains(k));
        self.transcripts = env.refresh_transcripts(&ids, &self.transcripts, self.show_last_message);
        let mut messages = HashMap::new();
        if self.show_last_message {
            for s in merged {
                if let Some(t) = self.hook_messages.get(&s.id).or_else(|| self.transcripts.get(&s.session_id).and_then(|e| e.text.as_ref())) {
                    messages.insert(s.id.clone(), t.clone());
                }
            }
        }
        messages
    }

    /// Viele Sessions laufen im Repo-Root, arbeiten per absoluten Pfaden aber in einem Geschwister- oder
    /// Unterordner-Worktree: `cwd` und Branch aus `merge` stimmen dann nicht. Ordnet über die zuletzt angefassten
    /// Transcript-Pfade den tatsächlich aktiven Worktree zu.
    fn apply_active_worktree(&mut self, merged: &mut [SessionRecord], env: &dyn PollEnv) {
        for s in merged.iter_mut() {
            let Some(candidates) = self.transcripts.get(&s.session_id).map(|e| e.tool_candidates.clone()) else { continue };
            let Some(top) = candidates.first() else { continue };
            // Nur neu laden, wenn der Kandidat von der gecachten Liste nicht mehr abgedeckt ist: nur dann kann ein
            // Worktree entstanden oder verschwunden sein. Mehrere Sessions im selben `cwd` teilen sich den Cache.
            let covered = self.worktree_cache.get(&s.cwd).is_some_and(|l| worktree::match_candidate(top, l).is_some());
            if !covered {
                self.worktree_cache.insert(s.cwd.clone(), env.worktrees(&s.cwd));
            }
            let Some(list) = self.worktree_cache.get(&s.cwd) else { continue };
            match worktree::active(&candidates, list) {
                None => s.active_worktree = None,
                Some(active) => {
                    s.active_worktree = Some(active.path.clone());
                    s.branch = active.branch.clone().or_else(|| s.branch.clone());
                }
            }
        }
    }
}

/// Live-Werte nur über die pid des eigenen Prozesses: eine fremde Session mit derselben sessionId (z. B. der gestoppte
/// Hintergrund-Eintrag) ist nicht diese Kachel, und `/clear` wechselt die sessionId.
pub fn merge(sessions: &[SessionRecord], agents: &[AgentInfo], pids: &HashMap<u32, String>, reported: &HashSet<String>, branch: &dyn Fn(&str) -> Option<String>) -> Vec<SessionRecord> {
    let mut live: HashMap<&String, &AgentInfo> = HashMap::new();
    for a in agents {
        if let Some(key) = a.pid.and_then(|p| pids.get(&p)) {
            live.insert(key, a);
        }
    }
    let mut pid_by_key: HashMap<&String, u32> = HashMap::new();
    for (pid, key) in pids {
        pid_by_key.entry(key).and_modify(|p| *p = (*p).min(*pid)).or_insert(*pid);
    }
    sessions
        .iter()
        .map(|s| {
            let mut s = s.clone();
            s.branch = branch(&s.cwd);
            // Per Hook gemeldet: Status, sessionId und Name kommen von der Session selbst, nur die pid nachziehen.
            if reported.contains(&s.id) {
                s.pid = pid_by_key.get(&s.id).copied();
                return s;
            }
            let Some(a) = live.get(&s.id) else {
                s.raw_status = None;
                s.pid = None;
                s.waiting_for = None;
                return s;
            };
            s.session_id = a.session_id.clone();
            if !agent::is_auto_name(&a.name, &s.cwd) {
                s.name = a.name.clone();
            }
            s.raw_status = a.status.clone();
            s.pid = a.pid;
            s.waiting_for = a.waiting_for.clone();
            s
        })
        .collect()
}

/// Überträgt die vom Poll ermittelten Felder per `id` auf `current`. Neu hinzugekommene Sessions bleiben unverändert,
/// entfernte kommen nicht zurück, `custom_name` bleibt aus `current`.
pub fn apply_live(polled: &[SessionRecord], current: &[SessionRecord], reported: &HashSet<String>) -> Vec<SessionRecord> {
    let by_id: HashMap<&String, &SessionRecord> = polled.iter().map(|s| (&s.id, s)).collect();
    current
        .iter()
        .map(|s| {
            let Some(p) = by_id.get(&s.id) else { return s.clone() };
            let mut s = s.clone();
            s.cwd = p.cwd.clone();
            s.pid = p.pid;
            s.branch = p.branch.clone();
            s.active_worktree = p.active_worktree.clone();
            s.has_running_shell = p.has_running_shell;
            // Gemeldete Sessions: was `report` inzwischen gesetzt hat, ist neuer als der Stand vom Poll-Beginn.
            if reported.contains(&s.id) {
                return s;
            }
            s.session_id = p.session_id.clone();
            s.name = p.name.clone();
            s.raw_status = p.raw_status.clone();
            s.waiting_for = p.waiting_for.clone();
            s.first_prompt = p.first_prompt.clone();
            s
        })
        .collect()
}

/// Bei Terminals ohne Claude folgt der Ordner dem `cd` der Shell.
fn apply_shell_cwd(merged: &mut [SessionRecord], pids: &HashMap<u32, String>, env: &dyn PollEnv) {
    for (pid, key) in pids {
        let Some(s) = merged.iter_mut().find(|s| &s.id == key && s.kind == SessionKind::Shell) else { continue };
        if let Some(dir) = env.shell_cwd(*pid) {
            s.cwd = dir;
        }
    }
}

/// Kommandos aus dem Bash-Werkzeug, die noch laufen, während Claude selbst schon wieder wartet.
fn apply_running_shells(merged: &mut [SessionRecord], pids: &HashMap<u32, String>, env: &dyn PollEnv) {
    let all: Vec<u32> = pids.keys().copied().collect();
    let keys: HashSet<&String> = env.running_shells(&all).iter().filter_map(|p| pids.get(p)).collect();
    for s in merged.iter_mut() {
        s.has_running_shell = keys.contains(&s.id);
    }
}

type RunningShells<'a> = Box<dyn Fn(&[u32]) -> HashSet<u32> + 'a>;

/// Der Poll-Zustand der Dateien (`<configDir>/sessions/<pid>.json`, Transcripts, `git`) für den Alltag. Pids, Shell-Ordner und
/// laufende Shells liefert der Aufrufer, weil das je Plattform anders geht.
pub struct FsPollEnv<'a> {
    pub config_dir: String,
    pub pids: HashMap<u32, String>,
    pub runner: &'a dyn crate::cli::CommandRunner,
    pub process_start: Box<dyn Fn(u32) -> Option<f64> + 'a>,
    pub shell_cwd: Box<dyn Fn(u32) -> Option<String> + 'a>,
    pub running_shells: RunningShells<'a>,
    pub reader: std::sync::Mutex<transcript::TranscriptReader>,
    /// Feste Uhrzeit (ms seit 1970) statt der Systemuhr, für Tests.
    pub clock_ms: Option<f64>,
}

impl<'a> FsPollEnv<'a> {
    pub fn new(config_dir: &str, pids: HashMap<u32, String>, runner: &'a dyn crate::cli::CommandRunner) -> FsPollEnv<'a> {
        FsPollEnv {
            config_dir: config_dir.into(),
            pids,
            runner,
            process_start: Box::new(|_| None),
            shell_cwd: Box::new(|_| None),
            running_shells: Box::new(|_| HashSet::new()),
            reader: Default::default(),
            clock_ms: None,
        }
    }
}

impl PollEnv for FsPollEnv<'_> {
    fn pids(&self) -> HashMap<u32, String> {
        self.pids.clone()
    }
    fn local_agents(&self, pids: &[u32]) -> Vec<AgentInfo> {
        agent::local_agents(pids, &self.config_dir, &*self.process_start)
    }
    fn branch(&self, cwd: &str) -> Option<String> {
        crate::git::branch(cwd)
    }
    fn shell_cwd(&self, pid: u32) -> Option<String> {
        (self.shell_cwd)(pid)
    }
    fn running_shells(&self, pids: &[u32]) -> HashSet<u32> {
        (self.running_shells)(pids)
    }
    fn first_prompt(&self, session_id: &str) -> Option<String> {
        let path = transcript::path(session_id, &self.config_dir)?;
        Some(transcript::first_prompt_default(&path).unwrap_or_default())
    }
    fn refresh_transcripts(&self, ids: &[String], cache: &HashMap<String, TranscriptEntry>, want_text: bool) -> HashMap<String, TranscriptEntry> {
        self.reader.lock().map(|mut r| r.refresh(ids, &self.config_dir, cache, want_text, self.now_ms() as u64)).unwrap_or_default()
    }
    fn worktrees(&self, cwd: &str) -> Vec<worktree::Entry> {
        worktree::list(self.runner, cwd)
    }
    fn now_ms(&self) -> f64 {
        self.clock_ms.unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{CommandRunner, RunOutput};
    use crate::paths::testdir::TempDir;
    use std::collections::BTreeMap;
    use std::time::Duration;

    struct NoRunner;
    impl CommandRunner for NoRunner {
        fn run(&self, _: &str, _: &[String], _: Option<&BTreeMap<String, String>>, _: Option<&str>, _: Duration) -> Result<RunOutput, String> {
            Err("kein Prozess im Test".into())
        }
    }

    fn agents(json: &str) -> Vec<AgentInfo> {
        agent::decode_list(json).unwrap()
    }

    fn session(id: &str, cwd: &str, session_id: &str, name: &str) -> SessionRecord {
        SessionRecord::new(id, cwd, 1.0, session_id, name)
    }

    fn no_branch(_: &str) -> Option<String> {
        None
    }

    fn pids(pairs: &[(u32, &str)]) -> HashMap<u32, String> {
        pairs.iter().map(|(p, k)| (*p, k.to_string())).collect()
    }

    /// Live-Werte nur über die eigene pid: der gestoppte Hintergrund-Eintrag mit derselben sessionId zählt nicht, eine
    /// neue sessionId (nach `/clear`) wird übernommen, Auto-Namen überschreiben keinen Titel.
    #[test]
    fn merge_by_own_pid() {
        let stored = vec![session("a7adb9af", "/p/proj", "old", "Fix"), session("k2", "/p/proj", "k2", "")];
        let list = agents(
            r#"[{ "id": "a7adb9af", "cwd": "/p/proj", "kind": "background", "startedAt": 1, "sessionId": "old", "name": "Fix", "state": "done" },
               { "pid": 11, "cwd": "/p/proj", "kind": "interactive", "startedAt": 5, "sessionId": "new", "name": "proj-c6", "status": "busy" },
               { "pid": 99, "cwd": "/p/proj", "kind": "interactive", "startedAt": 5, "sessionId": "k2", "name": "Fremd", "status": "busy" }]"#,
        );
        let p = pids(&[(11, "a7adb9af")]);
        let merged = merge(&stored, &list, &p, &HashSet::new(), &no_branch);
        assert_eq!(merged[0].session_id, "new");
        assert_eq!(merged[0].name, "Fix");
        assert_eq!(merged[0].status(), Status::Running);
        assert_eq!(merged[0].pid, Some(11));
        assert_eq!(merged[1].pid, None, "pid 99 gehört nicht Kadrell");
        assert_eq!(merged[1].status(), Status::Idle);
        assert_eq!(merged[1].name, "");

        let titled_agents = agents(r#"[{"pid":11,"cwd":"/p/proj","kind":"interactive","startedAt":5,"sessionId":"new","name":"Neuer Titel","status":"idle"}]"#);
        let titled = merge(&stored, &titled_agents, &p, &HashSet::new(), &no_branch);
        assert_eq!(titled[0].name, "Neuer Titel");

        // Von Hand vergebener Name bleibt Titel, auch wenn Claude Code später umbenennt.
        let mut manual = stored.clone();
        manual[0].custom_name = Some("Meins".into());
        let kept = merge(&manual, &titled_agents, &p, &HashSet::new(), &no_branch);
        assert_eq!(kept[0].title(), "Meins");
    }

    #[test]
    fn title_order_custom_local_claude_first_prompt_folder() {
        let mut s = session("abcdef12", "/p/homelab", "x", "");
        assert_eq!(s.title(), "homelab · abcd");
        s.first_prompt = Some("check mal".into());
        assert_eq!(s.title(), "check mal");
        s.name = "Claude Titel".into();
        assert_eq!(s.title(), "Claude Titel");
        s.local_title = Some("Lokaler Titel".into());
        assert_eq!(s.title(), "Lokaler Titel");
        s.custom_name = Some("Von Hand".into());
        assert_eq!(s.title(), "Von Hand");
    }

    #[test]
    fn duplicate_id_is_deduped_on_load_and_rejected_on_add() {
        let mut r = SessionRegistry::new(vec![session("s1", "/p", "s1", "eins"), session("s1", "/q", "s1", "zwei")], vec![]);
        assert_eq!(r.sessions.len(), 1);
        assert_eq!(r.sessions[0].name, "eins");
        assert!(!r.add(session("s1", "/x", "s1", "drei")));
        assert_eq!(r.sessions[0].name, "eins");
        assert!(r.add(session("s2", "/x", "s2", "vier")));
    }

    /// Der Schließen-Dialog verspricht „Konversation bleibt erhalten“: die sessionId bleibt nach `remove` auffindbar.
    #[test]
    fn remove_archives_closed_sessions_capped_newest_first() {
        let mut r = SessionRegistry::new(vec![], vec![]);
        r.add(session("s1", "/p", "conv-1", "Eins"));
        let removed = r.remove(&HashSet::from(["s1".to_string()]), 5000.0);
        assert_eq!(removed.len(), 1);
        assert_eq!(r.closed[0].session.session_id, "conv-1");
        assert_eq!(r.closed[0].closed_at, 5.0);
        for i in 2..=61 {
            r.add(session(&format!("s{i}"), "/p", &format!("conv-{i}"), ""));
            r.remove(&HashSet::from([format!("s{i}")]), 6000.0);
        }
        assert_eq!(r.closed.len(), 50);
        assert_eq!(r.closed[0].session.session_id, "conv-61");
    }

    #[test]
    fn rename_and_local_title() {
        let mut r = SessionRegistry::new(vec![session("a", "/p", "a", "")], vec![]);
        r.rename("a", "  Neu  ");
        assert_eq!(r.sessions[0].custom_name.as_deref(), Some("Neu"));
        r.rename("a", "   ");
        assert_eq!(r.sessions[0].custom_name, None);
        r.set_local_title("a", "Eins");
        r.set_local_title("a", "Zwei");
        assert_eq!(r.sessions[0].local_title.as_deref(), Some("Eins"));
    }

    #[test]
    fn background_switch_asks_for_an_immediate_poll_when_returning() {
        let mut r = SessionRegistry::default();
        assert_eq!(r.poll_interval(), 2.0);
        assert!(!r.set_background(true));
        assert_eq!(r.poll_interval(), 5.0);
        assert!(r.set_background(false));
        assert!(!r.set_background(false));
    }

    // RegistryReportTests (gegen echte Dateien, mit der pid des Testprozesses als Schlüssel)

    fn report(state: &str) -> HookReport {
        HookReport { state: state.into(), ..HookReport::default() }
    }

    #[test]
    fn reported_session_is_not_polled_until_process_ends() {
        let dir = TempDir::new("report");
        let pid = std::process::id();
        dir.write(
            &format!("sessions/{pid}.json"),
            &format!(r#"{{"pid":{pid},"sessionId":"alt","cwd":"/p","startedAt":9999999999999,"kind":"interactive","name":"Aus Datei","status":"busy"}}"#),
        );
        let mut r = SessionRegistry::new(vec![session("s1", &dir.path(), "s1", "")], vec![]);
        r.show_last_message = true;
        let poll = |r: &mut SessionRegistry, running: bool| {
            let p = if running { pids(&[(pid, "s1")]) } else { HashMap::new() };
            r.poll_now(&FsPollEnv::new(&dir.path(), p, &NoRunner))
        };

        assert!(poll(&mut r, true).notify);
        assert_eq!(r.sessions[0].status(), Status::Running);
        assert_eq!(r.sessions[0].session_id, "alt");

        let hook = HookReport {
            state: "waiting".into(),
            session_id: Some("neu".into()),
            title: Some("Vom Hook".into()),
            waiting_for: Some("Bash".into()),
            message: Some("Antwort".into()),
            first_prompt: Some("Erste Frage".into()),
        };
        let change = r.report("s1", &hook, &pids(&[(pid, "s1")]), 1000.0).unwrap();
        assert!(change.notify && change.persist);
        let s = &r.sessions[0];
        assert_eq!((s.status(), s.waiting_for.as_deref(), s.session_id.as_str(), s.name.as_str()), (Status::Waiting, Some("Bash"), "neu", "Vom Hook"));
        assert_eq!(s.first_prompt.as_deref(), Some("Erste Frage"));
        assert_eq!(s.pid, Some(pid));
        assert_eq!(r.last_messages["s1"], "Antwort");

        poll(&mut r, true);
        assert_eq!(r.sessions[0].status(), Status::Waiting, "die Datei (busy) darf die Meldung nicht überschreiben");
        assert_eq!(r.sessions[0].session_id, "neu");
        assert_eq!(r.sessions[0].name, "Vom Hook");

        r.report("s1", &report("idle"), &pids(&[(pid, "s1")]), 2000.0);
        assert_eq!(r.sessions[0].status(), Status::Idle);
        assert_eq!(r.sessions[0].waiting_for, None);

        poll(&mut r, false);
        assert_eq!(r.sessions[0].status(), Status::Idle);
        assert_eq!(r.sessions[0].pid, None);
        poll(&mut r, true);
        assert_eq!(r.sessions[0].status(), Status::Running, "nach dem Prozessende gilt wieder die Datei");
    }

    /// Esc/Strg+C feuert keinen `Stop`-Hook: die Datei springt auf `idle` mit neuerem `statusUpdatedAt`, der Poll übernimmt das.
    #[test]
    fn interrupt_without_stop_hook_falls_back_to_idle() {
        let dir = TempDir::new("report");
        let pid = std::process::id();
        let write = |status: &str, ms: f64| {
            dir.write(
                &format!("sessions/{pid}.json"),
                &format!(r#"{{"pid":{pid},"sessionId":"s1","cwd":"/p","startedAt":9999999999999,"kind":"interactive","name":"n","status":"{status}","statusUpdatedAt":{ms}}}"#),
            );
        };
        let mut r = SessionRegistry::new(vec![session("s1", &dir.path(), "s1", "")], vec![]);
        let p = pids(&[(pid, "s1")]);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64() * 1000.0;
        let poll_at = |r: &mut SessionRegistry, t: f64| {
            let mut env = FsPollEnv::new(&dir.path(), p.clone(), &NoRunner);
            env.clock_ms = Some(t);
            r.poll_now(&env)
        };

        write("idle", now - 60_000.0);
        r.report("s1", &report("working"), &p, now);
        poll_at(&mut r, now + 100.0);
        assert_eq!(r.sessions[0].status(), Status::Running, "altes idle von vor der Meldung zählt nicht (Schonfrist)");

        write("busy", now - 1.0);
        poll_at(&mut r, now + 200.0);
        assert_eq!(r.sessions[0].status(), Status::Running);

        write("idle", now + 300.0);
        poll_at(&mut r, now + 400.0);
        assert_eq!(r.sessions[0].status(), Status::Idle, "Abbruch: Datei ist neuer als die Meldung");

        let hook = HookReport { state: "waiting".into(), waiting_for: Some("Bash".into()), ..HookReport::default() };
        r.report("s1", &hook, &p, now + 500.0);
        poll_at(&mut r, now + 600.0);
        assert_eq!(r.sessions[0].status(), Status::Waiting, "neue Meldung schlägt den älteren Dateistand");
        assert_eq!(r.sessions[0].waiting_for.as_deref(), Some("Bash"));
        write("busy", now + 700.0);
        poll_at(&mut r, now + 800.0);
        assert_eq!(r.sessions[0].status(), Status::Running, "Freigabe erteilt: kein Hook, die Datei sagt busy");
        assert_eq!(r.sessions[0].waiting_for, None);
    }

    #[test]
    fn report_for_unknown_key_is_ignored() {
        let mut r = SessionRegistry::default();
        assert_eq!(r.report("weg", &report("idle"), &HashMap::new(), 0.0), None);
        assert!(r.sessions.is_empty());
    }

    #[test]
    fn errors_are_sticky_until_cleared() {
        let mut r = SessionRegistry::default();
        r.fail("claude fehlt", true);
        assert!(r.last_error_is_missing_binary);
        assert!(r.clear_error());
        assert!(!r.clear_error());
        assert!(!r.last_error_is_missing_binary);
    }

    #[test]
    fn first_poll_always_notifies_then_only_changes() {
        let dir = TempDir::new("firstpoll");
        let mut r = SessionRegistry::new(vec![session("s1", &dir.path(), "s1", "")], vec![]);
        let run = |r: &mut SessionRegistry| r.poll_now(&FsPollEnv::new(&dir.path(), HashMap::new(), &NoRunner));
        assert!(run(&mut r).notify);
        assert!(!run(&mut r).notify);
        assert!(r.polled);
    }

    #[test]
    fn first_prompt_becomes_title_when_claude_gave_none() {
        let dir = TempDir::new("prompt");
        dir.write("projects/-p/conv.jsonl", r#"{"type":"user","message":{"content":"Bitte den Build reparieren"}}"#);
        let mut r = SessionRegistry::new(vec![session("s1", &dir.path(), "conv", "")], vec![]);
        r.poll_now(&FsPollEnv::new(&dir.path(), HashMap::new(), &NoRunner));
        assert_eq!(r.sessions[0].first_prompt.as_deref(), Some("Bitte den Build reparieren"));
        assert_eq!(r.sessions[0].title(), "Bitte den Build reparieren");
    }

    /// Hat ein Poll Änderungen überholt (Session hinzugefügt oder entfernt, umbenannt, Hook gemeldet), gelten dessen
    /// Live-Werte nur für das, was noch da ist: neue Sessions bleiben, entfernte kommen nicht zurück, `custom_name` bleibt.
    #[test]
    fn apply_live_keeps_changes_made_while_polling() {
        let at_start = vec![session("s1", "/p", "s1", "Eins"), session("s3", "/p", "s3", "Drei")];
        let mut polled = at_start.clone();
        polled[0].pid = Some(7);
        polled[0].raw_status = Some("busy".into());
        polled[0].name = "Vom Poll".into();
        polled[1].pid = Some(8);
        // Währenddessen: s2 kommt dazu, s3 geht, s1 bekommt einen eigenen Namen.
        let mut current = vec![session("s1", "/p", "s1", "Eins"), session("s2", "/p", "s2", "Zwei")];
        current[0].custom_name = Some("Meins".into());
        let merged = apply_live(&polled, &current, &HashSet::new());
        assert_eq!(merged.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["s1", "s2"]);
        assert_eq!((merged[0].pid, merged[0].status(), merged[0].name.as_str()), (Some(7), Status::Running, "Vom Poll"));
        assert_eq!(merged[0].custom_name.as_deref(), Some("Meins"));
        assert_eq!(merged[1], current[1], "neu hinzugekommen: unverändert");

        // Gemeldete Sessions behalten, was `report` zwischenzeitlich gesetzt hat.
        let mut hooked = current.clone();
        hooked[0].raw_status = Some("waiting".into());
        let kept = apply_live(&polled, &hooked, &HashSet::from(["s1".to_string()]));
        assert_eq!(kept[0].status(), Status::Waiting);
        assert_eq!(kept[0].name, "Eins");
        assert_eq!(kept[0].pid, Some(7), "die pid kommt immer vom Poll");
    }

    /// Verdrahtung: was `background_shells` findet, steht nach dem Poll an der Session.
    #[test]
    fn poll_marks_session_with_running_shell() {
        let dir = TempDir::new("shells");
        let pid = std::process::id();
        let mut r = SessionRegistry::new(vec![session("s1", &dir.path(), "s1", "")], vec![]);
        let running = std::cell::Cell::new(true);
        let poll = |r: &mut SessionRegistry| {
            let mut env = FsPollEnv::new(&dir.path(), pids(&[(pid, "s1")]), &NoRunner);
            let on = running.get();
            env.running_shells = Box::new(move |p: &[u32]| if on { p.iter().copied().collect() } else { HashSet::new() });
            r.poll_now(&env)
        };
        poll(&mut r);
        assert!(r.sessions[0].has_running_shell);
        running.set(false);
        poll(&mut r);
        assert!(!r.sessions[0].has_running_shell);
    }

    #[test]
    fn shell_tiles_follow_the_cd_of_their_shell() {
        let dir = TempDir::new("shellcwd");
        let mut shell = session("shell-1", "/alt", "", "");
        shell.kind = SessionKind::Shell;
        let mut r = SessionRegistry::new(vec![shell], vec![]);
        let mut env = FsPollEnv::new(&dir.path(), pids(&[(42, "shell-1")]), &NoRunner);
        env.shell_cwd = Box::new(|pid| (pid == 42).then(|| "/neu".to_string()));
        r.poll_now(&env);
        assert_eq!(r.sessions[0].cwd, "/neu");
    }
}
