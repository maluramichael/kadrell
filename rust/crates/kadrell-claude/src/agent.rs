//! Einträge aus `claude agents --json --all` und aus `<configDir>/sessions/<pid>.json`
//! (Swift: `Agent` in `Model/Session.swift`, plus `Session.mapStatus` und `Session.isAutoName`).

use crate::paths;
use serde_json::Value;

/// Kopie von `SessionStatus` aus dem Kern (running, waiting, idle, error).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Running,
    Waiting,
    Idle,
    Error,
}

/// `status` hat Vorrang vor `state`; Unbekanntes zählt als untätig (der Aufrufer darf es loggen, siehe `unknown`).
pub fn map_status(state: Option<&str>, status: Option<&str>) -> Status {
    map_status_logged(state, status, &mut |_| {})
}

/// Wie `map_status`, meldet einen unbekannten Wert über `unknown` (Swift loggt ihn einmal je Wert).
pub fn map_status_logged(state: Option<&str>, status: Option<&str>, unknown: &mut dyn FnMut(&str)) -> Status {
    let Some(raw) = [status, state].into_iter().flatten().next() else { return Status::Idle };
    let v = raw.to_lowercase();
    match v.as_str() {
        "busy" | "working" | "running" | "active" | "starting" => Status::Running,
        "waiting" | "blocked" => Status::Waiting,
        "idle" | "done" | "stopped" => Status::Idle,
        _ if v.contains("error") || v.contains("fail") => Status::Error,
        _ if v.contains("run") || v.contains("work") || v.contains("active") => Status::Running,
        _ => {
            unknown(&v);
            Status::Idle
        }
    }
}

/// Ohne Titel benennt Claude Code interaktive Sessions `<ordner>-<2 hex>`, bei jedem Start neu.
pub fn is_auto_name(name: &str, cwd: &str) -> bool {
    let base = paths::last_component(cwd);
    let Some(rest) = name.strip_prefix(base).and_then(|r| r.strip_prefix('-')) else { return false };
    rest.len() == 2 && rest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Ein Eintrag aus `claude agents --json --all` bzw. einer Session-Datei.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentInfo {
    /// Kurze Id (8 Hex), nur bei Hintergrund-Sessions aus `claude --bg` (JSON-Feld `id`).
    pub short_id: Option<String>,
    pub cwd: String,
    pub kind: String,
    pub started_at: f64,
    pub session_id: String,
    pub name: String,
    pub state: Option<String>,
    pub status: Option<String>,
    pub pid: Option<u32>,
    pub waiting_for: Option<String>,
    /// Wann Claude Code `status` zuletzt geschrieben hat (ms seit 1970), nur in `sessions/<pid>.json`.
    pub status_updated_at: Option<f64>,
}

impl AgentInfo {
    /// Tolerant: ein Eintrag ohne `name` kippt nicht die ganze Liste.
    pub fn from_value(v: &Value) -> Option<AgentInfo> {
        let o = v.as_object()?;
        let text = |k: &str| o.get(k).and_then(Value::as_str).map(str::to_string);
        let short_id = text("id");
        let session_id = text("sessionId").or_else(|| short_id.clone()).unwrap_or_default();
        let name = text("name").or_else(|| short_id.clone()).unwrap_or_else(|| session_id.clone());
        Some(AgentInfo {
            cwd: text("cwd").unwrap_or_default(),
            kind: text("kind").unwrap_or_else(|| "background".into()),
            started_at: o.get("startedAt").and_then(Value::as_f64).unwrap_or(0.0),
            state: text("state"),
            status: text("status"),
            pid: o.get("pid").and_then(Value::as_u64).map(|p| p as u32),
            waiting_for: text("waitingFor"),
            status_updated_at: o.get("statusUpdatedAt").and_then(Value::as_f64),
            short_id,
            session_id,
            name,
        })
    }

    /// Laufende Hintergrund-Session aus `claude --bg` (gestoppte haben keine pid).
    pub fn is_running_background(&self) -> bool {
        self.kind == "background" && self.short_id.is_some() && self.pid.is_some()
    }

    pub fn mapped_status(&self) -> Status {
        map_status(self.state.as_deref(), self.status.as_deref())
    }
}

/// Liste als JSON-Array (`Agent.decodeList`).
pub fn decode_list(json: &str) -> Result<Vec<AgentInfo>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let items = v.as_array().ok_or("agents: JSON-Array erwartet")?;
    items.iter().map(|i| AgentInfo::from_value(i).ok_or_else(|| "agents: Eintrag ist kein Objekt".to_string())).collect()
}

/// Ausgabe von `claude agents --json --all`: alles vor dem ersten `[` (Warnungen) wird verworfen.
pub fn parse_agents_output(out: &str) -> Result<Vec<AgentInfo>, String> {
    match out.find('[') {
        Some(start) => decode_list(&out[start..]),
        None => Ok(Vec::new()),
    }
}

/// Claude Code schreibt je laufendem Prozess `<configDir>/sessions/<pid>.json` mit denselben Feldern, die
/// `claude agents --json` liefert. Eine liegengebliebene Datei eines früheren Prozesses mit derselben pid hat ein
/// `startedAt` vor dem Start des jetzigen Prozesses und wird ignoriert. `process_start` liefert die Startzeit eines
/// Prozesses in Sekunden seit 1970 (nil: unbekannt, dann gilt die Datei).
pub fn local_agents(pids: &[u32], config_dir: &str, process_start: &dyn Fn(u32) -> Option<f64>) -> Vec<AgentInfo> {
    pids.iter()
        .filter_map(|pid| {
            let text = std::fs::read_to_string(format!("{config_dir}/sessions/{pid}.json")).ok()?;
            let agent = AgentInfo::from_value(&serde_json::from_str(&text).ok()?)?;
            match process_start(*pid) {
                Some(start) if agent.started_at / 1000.0 < start - 1.0 => None,
                _ => Some(agent),
            }
        })
        .collect()
}

/// Läuft die Konversation `session_id` noch in einem Claude-Prozess? Liefert dessen pid (Swift: `resumeBlocker`):
/// alle `<configDir>/sessions/<pid>.json`, deren Prozess lebt (`alive`), über [`local_agents`] gelesen.
pub fn running_pid(session_id: &str, config_dir: &str, alive: &dyn Fn(u32) -> bool, process_start: &dyn Fn(u32) -> Option<f64>) -> Option<u32> {
    let entries = std::fs::read_dir(format!("{config_dir}/sessions")).ok()?;
    let pids: Vec<u32> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().replace(".json", "").parse::<u32>().ok())
        .filter(|pid| alive(*pid))
        .collect();
    local_agents(&pids, config_dir, process_start).into_iter().find(|a| a.session_id == session_id).map(|a| a.pid.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;

    const JSON: &str = r#"[
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

    #[test]
    fn decode_agents_json() {
        let list = decode_list(JSON).unwrap();
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].short_id.as_deref(), Some("0ace1aab"));
        assert!(!list[0].is_running_background(), "ohne pid: gestoppt oder weg");
        assert!(list[1].short_id.is_none());
        assert!(!list[1].is_running_background(), "interaktiv");
        assert_eq!(list[1].pid, Some(5395));
        assert!(list[2].is_running_background());
        assert_eq!(list[3].waiting_for.as_deref(), Some("input needed"));
    }

    #[test]
    fn entry_without_name_does_not_break_the_list() {
        let json = r#"[{"id":"79a85a2c","cwd":"/p","kind":"background","startedAt":1,"sessionId":"79a85a2c-x","state":"blocked"},{"id":"aa","cwd":"/p","kind":"background","startedAt":2,"sessionId":"aa-x","name":"fix","pid":5,"status":"idle"}]"#;
        let l = decode_list(json).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].name, "79a85a2c");
        assert_eq!(l[1].name, "fix");
    }

    #[test]
    fn output_with_warnings_before_the_array() {
        let out = format!("warning: x\n{JSON}");
        assert_eq!(parse_agents_output(&out).unwrap().len(), 4);
        assert!(parse_agents_output("keine Liste").unwrap().is_empty());
        assert!(parse_agents_output("[{").is_err());
    }

    #[test]
    fn status_mapping() {
        assert_eq!(map_status(None, Some("busy")), Status::Running);
        assert_eq!(map_status(None, Some("waiting")), Status::Waiting);
        assert_eq!(map_status(None, Some("idle")), Status::Idle);
        assert_eq!(map_status(Some("working"), None), Status::Running);
        assert_eq!(map_status(Some("failed"), None), Status::Error);
        assert_eq!(map_status(Some("somethingnew"), None), Status::Idle);
        assert_eq!(map_status(None, None), Status::Idle);
        let mut seen = Vec::new();
        map_status_logged(Some("seltsam"), None, &mut |v| seen.push(v.to_string()));
        assert_eq!(seen, ["seltsam"]);
    }

    /// Verifiziert 16.09.2026 (2.1.273): ohne Titel heißen interaktive Sessions `<ordner>-<2 hex>`.
    #[test]
    fn auto_name() {
        assert!(is_auto_name("claude-agent-overview-ad", "/p/claude-agent-overview"));
        assert!(is_auto_name("homelab-72", "/p/homelab"));
        assert!(!is_auto_name("homelab-fix", "/p/homelab"));
        assert!(!is_auto_name("kadrell-adopt-spike", "/tmp/kadrell-spike"));
        assert!(!is_auto_name("homelab-722", "/p/homelab"));
    }

    #[test]
    fn local_reads_own_pids_from_session_files() {
        let dir = TempDir::new("agents");
        dir.write("sessions/123.json", r#"{"pid":123,"sessionId":"s1","cwd":"/tmp/p","startedAt":9999999999999,"kind":"interactive","name":"p-1a","status":"busy","updatedAt":2}"#);
        dir.write("sessions/456.json", "kaputt");
        let agents = local_agents(&[123, 456, 999], &dir.path(), &|_| None);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].pid, Some(123));
        assert_eq!(agents[0].status.as_deref(), Some("busy"));
        assert_eq!(agents[0].session_id, "s1");
        assert!(agents[0].short_id.is_none());
    }

    /// pid wiederverwendet: die liegengebliebene Datei ist älter als der laufende Prozess und zählt nicht.
    #[test]
    fn ignores_session_file_older_than_process() {
        let dir = TempDir::new("agents");
        dir.write("sessions/7.json", r#"{"pid":7,"sessionId":"alt","cwd":"/p","startedAt":1000,"kind":"interactive","name":"p"}"#);
        let start = &|_: u32| Some(1_700_000_000.0);
        assert!(local_agents(&[7], &dir.path(), start).is_empty());
        dir.write("sessions/7.json", r#"{"pid":7,"sessionId":"neu","cwd":"/p","startedAt":1700000100000,"kind":"interactive","name":"p"}"#);
        assert_eq!(local_agents(&[7], &dir.path(), start)[0].session_id, "neu");
    }

    #[test]
    fn running_pid_needs_a_live_process_with_that_session() {
        let dir = TempDir::new("agents");
        dir.write("sessions/11.json", r#"{"pid":11,"sessionId":"s-live","cwd":"/p","startedAt":9999999999999,"kind":"interactive","name":"p"}"#);
        dir.write("sessions/12.json", r#"{"pid":12,"sessionId":"s-dead","cwd":"/p","startedAt":9999999999999,"kind":"interactive","name":"p"}"#);
        let alive = &|pid: u32| pid == 11;
        let none = &|_: u32| None;
        assert_eq!(running_pid("s-live", &dir.path(), alive, none), Some(11));
        assert_eq!(running_pid("s-dead", &dir.path(), alive, none), None, "Prozess lebt nicht mehr");
        assert_eq!(running_pid("andere", &dir.path(), alive, none), None);
        assert_eq!(running_pid("s-live", "/gibt/es/nicht", alive, none), None);
    }
}
