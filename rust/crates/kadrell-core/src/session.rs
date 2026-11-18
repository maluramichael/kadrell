//! Sessions: das gespeicherte Modell (`sessions.json`, `closed.json`), Status, Titel und die Einträge von
//! `claude agents --json`. Quelle: `Model/Session.swift` und `Model/AgentKind.swift`.

use crate::paths::base_name;
use crate::settings::{Settings, claude_launch_args};
use serde::{Deserialize, Serialize, Serializer};
use std::path::Path;

/// Programm, das in einer neuen Session läuft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentKind {
    Claude,
    Pi,
}

impl AgentKind {
    pub const ALL: [AgentKind; 2] = [AgentKind::Claude, AgentKind::Pi];

    /// Eigenname, bewusst nicht übersetzt.
    pub fn display_name(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude",
            AgentKind::Pi => "Pi",
        }
    }

    /// Wert in den Einstellungen (`lastAgent`).
    pub fn raw(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Pi => "pi",
        }
    }

    pub fn from_raw(raw: &str) -> Option<AgentKind> {
        AgentKind::ALL.into_iter().find(|k| k.raw() == raw)
    }
}

/// Zustand einer laufenden Session, abgeleitet aus dem Rohwert von Claude Code oder den Hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionStatus {
    Running,
    Waiting,
    Idle,
    Error,
}

impl SessionStatus {
    pub const ALL: [SessionStatus; 4] = [SessionStatus::Running, SessionStatus::Waiting, SessionStatus::Idle, SessionStatus::Error];

    /// Wie `ls --json` und die Mac-App ihn schreiben: `running`, `waiting`, `idle`, `error`.
    pub fn as_str(self) -> &'static str {
        match self {
            SessionStatus::Running => "running",
            SessionStatus::Waiting => "waiting",
            SessionStatus::Idle => "idle",
            SessionStatus::Error => "error",
        }
    }
}

/// Eine Session, die Kadrell als Prozess in einer Kachel startet. Gespeichert werden nur Schlüssel, Ordner, Start,
/// sessionId, Namen und Startoptionen; Status und pid kommen zur Laufzeit (`#[serde(skip)]`).
///
/// Die Felder stehen alphabetisch, wie Swift sie mit sortierten Schlüsseln schreibt. Fehlende optionale
/// Schlüssel in älteren Dateien sind erlaubt, `id`, `cwd`, `startedAt`, `sessionId` und `name` sind Pflicht.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Modell und Effort nur dieser Claude-Session (`kadrell new --model --effort`), `None` = Einstellung der App.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_model: Option<String>,
    /// Von Hand vergebener Name (F2), geht immer vor `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_name: Option<String>,
    /// Bei Terminals ohne Claude folgt der Ordner dem `cd` der Shell.
    pub cwd: String,
    /// Remote-Session: ssh-Host aus der ssh-Konfiguration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Stabiler Schlüssel für Gruppen und Auswahl: bei neuen Sessions die erste sessionId.
    pub id: String,
    /// Kurzer Titel von einem lokalen Modell, geht vor `name`, aber nach `custom_name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_title: Option<String>,
    /// Pi-Session: lokales Modell als `provider|name`, `None` = Pis Standard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Titel von Claude Code, leer bis es einen vergibt.
    pub name: String,
    /// Aktuelle Konversation. `/clear` oder `/resume` wechselt sie, `--resume` braucht die neueste.
    pub session_id: String,
    /// Start in Millisekunden seit 1970.
    #[serde(serialize_with = "ser_number")]
    pub started_at: f64,
    /// tmux-Session auf dem Host; `None` hängt sich an die zuletzt benutzte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmux_session: Option<String>,

    /// Laufzeit: Rohstatus (`busy`, `waiting` ...) aus Hook oder Session-Datei.
    #[serde(skip)]
    pub raw_status: Option<String>,
    /// Laufzeit: pid des eigenen Prozesses.
    #[serde(skip)]
    pub pid: Option<i64>,
    #[serde(skip)]
    pub waiting_for: Option<String>,
    /// Laufzeit: Git-Branch des Projektordners (oder des aktiven Worktrees).
    #[serde(skip)]
    pub branch: Option<String>,
    /// Laufzeit: Worktree, in dem laut Transcript zuletzt gearbeitet wurde, `None` = Hauptverzeichnis.
    #[serde(skip)]
    pub active_worktree: Option<String>,
    /// Laufzeit: erste Nachricht aus dem Transcript, Ersatztitel.
    #[serde(skip)]
    pub first_prompt: Option<String>,
    /// Laufzeit: noch läuft ein Kommando aus dem Bash-Tool, obwohl Claude selbst schon wieder wartet.
    #[serde(skip)]
    pub has_running_shell: bool,
    /// Laufzeit: letzte Antwort von Claude (kommt mit dem Stop-Hook), für die zweite Zeile im Baum.
    #[serde(skip)]
    pub message: Option<String>,
}

/// Schreibt ganzzahlige Werte ohne `.0` (wie Swift), damit die Datei für beide Seiten gleich aussieht.
fn ser_number<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if v.fract() == 0.0 && v.abs() < 9.0e15 { s.serialize_i64(*v as i64) } else { s.serialize_f64(*v) }
}

impl Session {
    /// Schlüssel-Präfix eines Terminals ohne Claude.
    pub const SHELL_PREFIX: &'static str = "shell-";
    /// Schlüssel-Präfix einer Pi-Session.
    pub const PI_PREFIX: &'static str = "pi-";
    /// Schlüssel-Präfix einer Remote-Session (macht nur den Schlüssel lesbar, erkannt wird sie am `host`).
    pub const REMOTE_PREFIX: &'static str = "ssh-";

    /// Wie `Session(id:cwd:startedAt:sessionId:name:)` auf dem Mac.
    pub fn new(id: &str, cwd: &str, started_at: f64, session_id: &str, name: &str) -> Session {
        Session { id: id.into(), cwd: cwd.into(), started_at, session_id: session_id.into(), name: name.into(), ..Session::default() }
    }

    /// Neue Claude-Session: Schlüssel und sessionId sind dieselbe frische UUID.
    pub fn fresh(cwd: &str, started_at_ms: f64) -> Session {
        let id = uuid::Uuid::new_v4().to_string();
        Session::new(&id, cwd, started_at_ms, &id, "")
    }

    /// Anzeigename: eigener Name vor automatischem Titel.
    pub fn title(&self) -> String {
        self.custom_name.clone().unwrap_or_else(|| self.auto_title())
    }

    /// Titel ohne eigenen Namen: Remote tmux-Session oder Host, Terminal Ordnername, lokaler Titel, Titel von Claude
    /// Code, erste Nachricht, zuletzt `<ordner> · <4 Zeichen des Schlüssels>`.
    pub fn auto_title(&self) -> String {
        if let Some(host) = &self.host {
            return self.tmux_session.clone().unwrap_or_else(|| host.clone());
        }
        if self.is_shell() {
            return base_name(&self.cwd);
        }
        if let Some(t) = &self.local_title {
            return t.clone();
        }
        if !self.name.is_empty() {
            return self.name.clone();
        }
        self.first_prompt.clone().unwrap_or_else(|| format!("{} · {}", base_name(&self.cwd), self.id.chars().take(4).collect::<String>()))
    }

    /// Terminal ohne Claude, erkennbar am Schlüssel.
    pub fn is_shell(&self) -> bool {
        self.id.starts_with(Self::SHELL_PREFIX)
    }

    pub fn new_shell_id() -> String {
        format!("{}{}", Self::SHELL_PREFIX, uuid::Uuid::new_v4())
    }

    /// Pi mit lokalem Modell statt `claude`, erkennbar am Schlüssel.
    pub fn is_pi(&self) -> bool {
        self.id.starts_with(Self::PI_PREFIX)
    }

    pub fn new_pi_id() -> String {
        format!("{}{}", Self::PI_PREFIX, uuid::Uuid::new_v4())
    }

    /// Hinter dem Präfix steht die `--session-id` von Pi.
    pub fn pi_session_id(&self) -> &str {
        self.id.strip_prefix(Self::PI_PREFIX).unwrap_or(&self.id)
    }

    /// Remote-Session, erkannt an `host`.
    pub fn is_remote(&self) -> bool {
        self.host.is_some()
    }

    /// Nur Claude-Sessions haben ein Transcript unter `<configDir>/projects`.
    pub fn has_transcript(&self) -> bool {
        !self.is_shell() && !self.is_remote() && !self.is_pi()
    }

    /// Zustand aus dem Rohwert.
    pub fn status(&self) -> SessionStatus {
        Session::map_status(None, self.raw_status.as_deref())
    }

    /// Rohwerte von Claude Code auf vier Zustände abbilden. Wie auf dem Mac zählt der erste vorhandene Wert,
    /// `status` vor `state`; Unbekanntes gilt als `Idle`.
    pub fn map_status(state: Option<&str>, status: Option<&str>) -> SessionStatus {
        let Some(raw) = status.or(state) else { return SessionStatus::Idle };
        let v = raw.to_lowercase();
        match v.as_str() {
            "busy" | "working" | "running" | "active" | "starting" => SessionStatus::Running,
            "waiting" | "blocked" => SessionStatus::Waiting,
            "idle" | "done" | "stopped" => SessionStatus::Idle,
            _ if v.contains("error") || v.contains("fail") => SessionStatus::Error,
            _ if v.contains("run") || v.contains("work") || v.contains("active") => SessionStatus::Running,
            _ => SessionStatus::Idle,
        }
    }

    /// Ohne Titel benennt Claude Code interaktive Sessions `<ordner>-<2 hex>`, bei jedem Start neu.
    pub fn is_auto_name(name: &str, cwd: &str) -> bool {
        let base = base_name(cwd);
        let Some(rest) = name.strip_prefix(&format!("{base}-")) else { return false };
        rest.len() == 2 && rest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    }

    /// Laufzeit seit dem Start als `42m`, `2h` oder `2d`; `now` in Sekunden seit 1970.
    pub fn elapsed(&self, now: f64) -> String {
        let m = ((now - self.started_at / 1000.0) / 60.0).round() as i64;
        if m < 60 {
            return format!("{}m", m.max(0));
        }
        if m < 1440 {
            return format!("{}h", (m as f64 / 60.0).round() as i64);
        }
        format!("{}d", (m as f64 / 1440.0).round() as i64)
    }

    /// Start-Flags für den Claude-Prozess: Modell und Effort der Session gehen den Einstellungen vor.
    pub fn launch_args(&self, settings: &Settings) -> Vec<String> {
        claude_launch_args(
            settings.claude_allow_bypass(),
            &settings.claude_mode(),
            self.claude_model.as_deref().unwrap_or(&settings.claude_model()),
            self.claude_effort.as_deref().unwrap_or(&settings.claude_effort()),
        )
    }
}

/// Eintrag in `closed.json`: die Session, wie sie beim Schließen zuletzt aussah, plus Zeitpunkt (Sekunden seit 1970).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedSession {
    #[serde(serialize_with = "ser_number")]
    pub closed_at: f64,
    pub session: Session,
}

/// Ein Eintrag aus `claude agents --json --all` oder `<configDir>/sessions/<pid>.json`. Tolerant: fehlende
/// Felder bekommen Ersatzwerte, damit ein halber Eintrag nicht die ganze Liste kippt.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(from = "RawAgent")]
pub struct Agent {
    /// Kurze Id (8 Hex), nur bei Hintergrund-Sessions aus `claude --bg`.
    pub short_id: Option<String>,
    pub cwd: String,
    pub kind: String,
    pub started_at: f64,
    pub session_id: String,
    pub name: String,
    pub state: Option<String>,
    pub status: Option<String>,
    pub pid: Option<i64>,
    pub waiting_for: Option<String>,
    /// Wann Claude Code `status` zuletzt geschrieben hat (ms seit 1970), nur in `sessions/<pid>.json`.
    pub status_updated_at: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAgent {
    id: Option<String>,
    cwd: Option<String>,
    kind: Option<String>,
    started_at: Option<f64>,
    session_id: Option<String>,
    name: Option<String>,
    state: Option<String>,
    status: Option<String>,
    pid: Option<i64>,
    waiting_for: Option<String>,
    status_updated_at: Option<f64>,
}

impl From<RawAgent> for Agent {
    fn from(r: RawAgent) -> Agent {
        let session_id = r.session_id.or_else(|| r.id.clone()).unwrap_or_default();
        let name = r.name.or_else(|| r.id.clone()).unwrap_or_else(|| session_id.clone());
        Agent {
            short_id: r.id,
            cwd: r.cwd.unwrap_or_default(),
            kind: r.kind.unwrap_or_else(|| "background".into()),
            started_at: r.started_at.unwrap_or(0.0),
            session_id,
            name,
            state: r.state,
            status: r.status,
            pid: r.pid,
            waiting_for: r.waiting_for,
            status_updated_at: r.status_updated_at,
        }
    }
}

impl Agent {
    /// Laufende Hintergrund-Session aus `claude --bg` (gestoppte haben keine pid).
    pub fn is_running_background(&self) -> bool {
        self.kind == "background" && self.short_id.is_some() && self.pid.is_some()
    }

    pub fn decode_list(data: &[u8]) -> serde_json::Result<Vec<Agent>> {
        serde_json::from_slice(data)
    }

    /// Claude Code schreibt je laufendem Prozess `<configDir>/sessions/<pid>.json`. Eine liegengebliebene Datei
    /// eines früheren Prozesses mit derselben pid hat ein `startedAt` vor dem Start des jetzigen Prozesses und wird
    /// ignoriert. `process_start` liefert die Startzeit eines Prozesses in Sekunden seit 1970 (OS-abhängig, kommt
    /// vom Aufrufer), `None` = unbekannt, dann zählt die Datei.
    pub fn local(pids: &[i64], config_dir: &Path, process_start: impl Fn(i64) -> Option<f64>) -> Vec<Agent> {
        pids.iter()
            .filter_map(|pid| {
                let data = std::fs::read(config_dir.join("sessions").join(format!("{pid}.json"))).ok()?;
                let agent: Agent = serde_json::from_slice(&data).ok()?;
                match process_start(*pid) {
                    Some(start) if agent.started_at / 1000.0 < start - 1.0 => None,
                    _ => Some(agent),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testutil::TempDir;

    const AGENTS: &str = r#"[
      { "id": "0ace1aab", "cwd": "/Users/dev/development/projects/homelab", "kind": "background",
        "startedAt": 1786719895859, "sessionId": "0ace1aab-c1a7-45e2-9856-dea5d34eda81",
        "name": "Weltport für Raspberry Pi Display Dashboard prüfen", "state": "blocked" },
      { "pid": 5395, "cwd": "/Users/dev/development/acme/intern/demoapp", "kind": "interactive",
        "startedAt": 1789040585856, "sessionId": "b746c5bb-ecfe-45d8-b268-7df9b23b5e44", "name": "demoapp-78", "status": "idle" },
      { "pid": 18859, "id": "86f99758", "cwd": "/private/tmp/kadrell-spike", "kind": "background", "startedAt": 1789479734977,
        "sessionId": "86f99758-a2e1-4923-b27e-1374ddea59b9", "name": "kadrell-spike", "status": "busy", "state": "working" },
      { "pid": 76489, "cwd": "/x", "kind": "interactive", "startedAt": 1789452498737, "sessionId": "c6994699-de7e-4a0a-86dc-e79a90802d67",
        "name": "export", "status": "waiting", "waitingFor": "input needed" }
    ]"#;

    fn s(id: &str, cwd: &str, name: &str) -> Session {
        Session::new(id, cwd, 0.0, "x", name)
    }

    #[test]
    fn decode_agents_json() {
        let list = Agent::decode_list(AGENTS.as_bytes()).unwrap();
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].short_id.as_deref(), Some("0ace1aab"));
        assert!(!list[0].is_running_background(), "ohne pid: gestoppt oder weg");
        assert_eq!(list[1].short_id, None);
        assert!(!list[1].is_running_background(), "interaktiv");
        assert_eq!(list[1].pid, Some(5395));
        assert!(list[2].is_running_background());
        assert_eq!(list[3].waiting_for.as_deref(), Some("input needed"));
    }

    #[test]
    fn entry_without_name_does_not_break_the_list() {
        let json = r#"[{"id":"79a85a2c","cwd":"/p","kind":"background","startedAt":1,"sessionId":"79a85a2c-x","state":"blocked"},{"id":"aa","cwd":"/p","kind":"background","startedAt":2,"sessionId":"aa-x","name":"fix","pid":5,"status":"idle"}]"#;
        let l = Agent::decode_list(json.as_bytes()).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name, "79a85a2c");
        assert_eq!(l[1].name, "fix");
    }

    #[test]
    fn agent_defaults_for_missing_fields() {
        let l = Agent::decode_list(br#"[{"id":"abc"}]"#).unwrap();
        assert_eq!((l[0].session_id.as_str(), l[0].name.as_str(), l[0].kind.as_str()), ("abc", "abc", "background"));
        assert_eq!((l[0].cwd.as_str(), l[0].started_at), ("", 0.0));
        assert!(Agent::decode_list(br#"[{"name":5}]"#).is_err());
    }

    #[test]
    fn local_ignores_stale_files_of_reused_pids() {
        let d = TempDir::new();
        std::fs::create_dir_all(d.path().join("sessions")).unwrap();
        let body = |started: u64| format!(r#"{{"pid":7,"cwd":"/p","kind":"interactive","startedAt":{started},"sessionId":"s"}}"#);
        std::fs::write(d.path().join("sessions/7.json"), body(1_000_000)).unwrap();
        assert!(Agent::local(&[7], d.path(), |_| Some(2_000.0)).is_empty(), "Datei älter als der Prozess");
        assert_eq!(Agent::local(&[7], d.path(), |_| Some(1_000.0)).len(), 1);
        assert_eq!(Agent::local(&[7], d.path(), |_| None).len(), 1);
        assert!(Agent::local(&[8], d.path(), |_| None).is_empty());
    }

    #[test]
    fn status_mapping() {
        use SessionStatus::*;
        assert_eq!(Session::map_status(None, Some("busy")), Running);
        assert_eq!(Session::map_status(None, Some("waiting")), Waiting);
        assert_eq!(Session::map_status(None, Some("idle")), Idle);
        assert_eq!(Session::map_status(Some("working"), None), Running);
        assert_eq!(Session::map_status(Some("failed"), None), Error);
        assert_eq!(Session::map_status(Some("somethingnew"), None), Idle);
        assert_eq!(Session::map_status(None, None), Idle);
        assert_eq!(Session::map_status(None, Some("BLOCKED")), Waiting);
        assert_eq!(Session::map_status(Some("running"), Some("idle")), Idle, "status vor state");
    }

    #[test]
    fn auto_name_and_title() {
        assert!(Session::is_auto_name("claude-agent-overview-ad", "/p/claude-agent-overview"));
        assert!(Session::is_auto_name("homelab-72", "/p/homelab"));
        assert!(!Session::is_auto_name("homelab-fix", "/p/homelab"));
        assert!(!Session::is_auto_name("homelab-7", "/p/homelab"));
        assert!(!Session::is_auto_name("kadrell-adopt-spike", "/tmp/kadrell-spike"));
        assert_eq!(s("abcdef12", "/p/homelab", "").title(), "homelab · abcd");
        assert_eq!(s("abcdef12", "/p/homelab", "Fix").title(), "Fix");
    }

    #[test]
    fn title_order() {
        let mut x = s("abcdef12", "/p/homelab", "Claude");
        x.first_prompt = Some("Erste".into());
        x.local_title = Some("Lokal".into());
        x.custom_name = Some("Meins".into());
        assert_eq!(x.title(), "Meins");
        x.custom_name = None;
        assert_eq!(x.title(), "Lokal");
        x.local_title = None;
        assert_eq!(x.title(), "Claude");
        x.name.clear();
        assert_eq!(x.title(), "Erste");
        x.host = Some("srv".into());
        assert_eq!(x.title(), "srv");
        x.tmux_session = Some("test".into());
        assert_eq!(x.title(), "test");
        let shell = s("shell-1", "/p/homelab", "");
        assert_eq!(shell.title(), "homelab");
    }

    #[test]
    fn kinds_by_key_prefix() {
        let pi = s(&Session::new_pi_id(), "/p", "");
        assert!(pi.is_pi() && !pi.has_transcript());
        assert_eq!(pi.pi_session_id().len(), 36);
        assert!(s(&Session::new_shell_id(), "/p", "").is_shell());
        let mut r = s("ssh-1", "/p", "");
        r.host = Some("h".into());
        assert!(r.is_remote() && !r.has_transcript());
        assert!(s("abc", "/p", "").has_transcript());
        assert_eq!(AgentKind::from_raw("pi"), Some(AgentKind::Pi));
        assert_eq!(AgentKind::Claude.display_name(), "Claude");
    }

    #[test]
    fn elapsed_formats() {
        let now = 1_800_000_000.0;
        let at = |ago: f64| Session::new("s", "/", (now - ago) * 1000.0, "s", "n");
        assert_eq!(at(42.0 * 60.0).elapsed(now), "42m");
        assert_eq!(at(130.0 * 60.0).elapsed(now), "2h");
        assert_eq!(at(2.0 * 86400.0).elapsed(now), "2d");
        assert_eq!(at(-30.0).elapsed(now), "0m");
    }

    #[test]
    fn launch_args_prefer_session_overrides() {
        let mut settings = Settings::default();
        let mut x = s("a", "/", "");
        assert!(x.launch_args(&settings).is_empty());
        settings.set_claude_allow_bypass(true);
        settings.set_claude_mode("bypassPermissions");
        settings.set_claude_model("opus");
        let base = x.launch_args(&settings);
        assert_eq!(base, ["--allow-dangerously-skip-permissions", "--permission-mode", "bypassPermissions", "--model", "opus"]);
        x.claude_model = Some("claude-opus-5-5".into());
        x.claude_effort = Some("medium".into());
        assert!(x.launch_args(&settings).join(" ").ends_with("--model claude-opus-5-5 --effort medium"));
    }

    /// Datei so, wie die Mac-App sie schreibt (sortierte Schlüssel, ganzzahlige Zeiten, optionale Schlüssel fehlen).
    const SWIFT_SESSIONS: &str = r#"[
  {
    "claudeEffort" : "high",
    "claudeModel" : "sonnet",
    "customName" : "Meins",
    "cwd" : "/Users/dev/p/kadrell",
    "id" : "f45edac5-957e-4453-9dea-ff5728623427",
    "localTitle" : "Kurz",
    "name" : "kadrell-ab",
    "sessionId" : "f45edac5-957e-4453-9dea-ff5728623427",
    "startedAt" : 1789479734977
  },
  {
    "cwd" : "/Users/dev",
    "host" : "examplehost",
    "id" : "ssh-1",
    "model" : "ollama|qwen",
    "name" : "",
    "sessionId" : "ssh-1",
    "startedAt" : 1789479734978.5,
    "tmuxSession" : "test"
  }
]"#;

    #[test]
    fn swift_shaped_json_decodes_and_roundtrips() {
        let list: Vec<Session> = serde_json::from_str(SWIFT_SESSIONS).unwrap();
        assert_eq!(list[0].claude_model.as_deref(), Some("sonnet"));
        assert_eq!(list[0].title(), "Meins");
        assert_eq!(list[1].tmux_session.as_deref(), Some("test"));
        assert_eq!(list[1].model.as_deref(), Some("ollama|qwen"));
        let again: Vec<Session> = serde_json::from_slice(&serde_json::to_vec_pretty(&list).unwrap()).unwrap();
        assert_eq!(again, list);
    }

    #[test]
    fn written_json_has_sorted_keys_integer_times_and_no_runtime_fields() {
        let mut x = s("a", "/p", "n");
        x.started_at = 1_789_479_734_977.0;
        x.raw_status = Some("busy".into());
        x.pid = Some(3);
        let text = serde_json::to_string(&x).unwrap();
        assert_eq!(text, r#"{"cwd":"/p","id":"a","name":"n","sessionId":"x","startedAt":1789479734977}"#);
        x.custom_name = Some("M".into());
        assert!(serde_json::to_string(&x).unwrap().contains(r#""customName":"M""#));
    }

    #[test]
    fn old_files_without_optional_keys_load() {
        let x: Session = serde_json::from_str(r#"{"id":"a","cwd":"/","startedAt":0,"sessionId":"a","name":""}"#).unwrap();
        assert!(x.custom_name.is_none() && x.claude_model.is_none() && x.host.is_none());
        assert!(serde_json::from_str::<Session>(r#"{"id":"a","cwd":"/","startedAt":0,"sessionId":"a"}"#).is_err(), "name ist Pflicht");
    }

    #[test]
    fn closed_session_roundtrip() {
        let c = ClosedSession { closed_at: 1_789_479_734.5, session: s("s1", "/p", "Eins") };
        let text = serde_json::to_string(&c).unwrap();
        assert!(text.starts_with(r#"{"closedAt":1789479734.5,"session":{"#), "{text}");
        assert_eq!(serde_json::from_str::<ClosedSession>(&text).unwrap(), c);
    }
}
