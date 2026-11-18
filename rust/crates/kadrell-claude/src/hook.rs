//! `kadrell hook claude`: Claude Code ruft seine Hooks mit JSON auf stdin auf und vererbt ihnen die Umgebung der Kachel
//! (`KADRELL_SOCKET`, `KADRELL_SESSION_KEY`, `CLAUDE_PID`). Das Ereignis wird auf `kadrell status …` abgebildet,
//! die Session meldet sich damit selbst (Swift: `Services/ClaudeHook.swift`).

use crate::{agent, transcript};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Ereignisse, die Kadrell jedem Claude-Prozess als Hooks mitgibt.
pub const EVENTS: [&str; 5] = ["SessionStart", "UserPromptSubmit", "PermissionRequest", "Notification", "Stop"];

/// Die Hook-Zeile: außerhalb von Kadrell (kein Socket) stumm, sonst dieses Binary. Claude Code führt Hooks unter
/// Windows in Git Bash aus, deshalb dieselbe sh-Zeile wie auf dem Mac.
pub const COMMAND: &str = r#"[ -n "$KADRELL_SOCKET" ] && exec "$KADRELL" hook claude || exit 0"#;

/// `notification_type`-Werte, bei denen Claude blockiert auf den Nutzer wartet. `idle_prompt` gehört bewusst nicht
/// dazu: das feuert ~60 s nachdem Claude fertig ist, die Session ist dann untätig, nicht blockiert.
pub const WAITING_NOTIFICATIONS: [&str; 3] = ["permission_prompt", "agent_needs_input", "elicitation_dialog"];

/// `--settings <json>` für den Start jedes Claude-Prozesses: die Hooks gelten nur für diesen Prozess, ohne Eintrag in
/// `~/.claude/settings.json`. serde_json sortiert die Schlüssel (BTreeMap), wie `.sortedKeys` in Swift.
pub fn launch_args() -> Vec<String> {
    let hook = json!({"type": "command", "command": COMMAND, "timeout": 5});
    let hooks: Map<String, Value> = EVENTS.iter().map(|e| (e.to_string(), json!([{"matcher": "", "hooks": [hook.clone()]}]))).collect();
    vec!["--settings".into(), json!({ "hooks": hooks }).to_string()]
}

/// Was ein Hook der Kachel meldet. Entspricht den Argumenten von `kadrell status` und, im Kern, dem Aufruf
/// `SessionRegistry.report` (state, sessionId, title, waitingFor, message, firstPrompt).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HookReport {
    /// `working`, `waiting` oder `idle`.
    pub state: String,
    pub session_id: Option<String>,
    pub title: Option<String>,
    pub waiting_for: Option<String>,
    pub message: Option<String>,
    pub first_prompt: Option<String>,
}

impl HookReport {
    fn new(state: &str) -> HookReport {
        HookReport { state: state.into(), ..HookReport::default() }
    }

    /// `["status", <state>, --title …, --waiting-for …, --first-prompt …, --message …, --session-id …]`.
    pub fn to_argv(&self) -> Vec<String> {
        let mut args = vec!["status".to_string(), self.state.clone()];
        for (flag, value) in [
            ("--title", &self.title),
            ("--waiting-for", &self.waiting_for),
            ("--first-prompt", &self.first_prompt),
            ("--message", &self.message),
            ("--session-id", &self.session_id),
        ] {
            if let Some(v) = value.as_ref().filter(|v| !v.is_empty()) {
                args.push(flag.to_string());
                args.push(v.clone());
            }
        }
        args
    }
}

/// Hook-JSON von stdin als Meldung; nil = das Ereignis ist für den Status uninteressant (Subagent, anderes Ereignis,
/// kein JSON). `env` ist die Umgebung des Hook-Prozesses (`CLAUDE_PID`, `CLAUDE_CONFIG_DIR`), `home` das Home-Verzeichnis.
pub fn report_from_json(json: &[u8], env: &HashMap<String, String>, home: &str) -> Option<HookReport> {
    let obj: Map<String, Value> = serde_json::from_slice(json).ok()?;
    if obj.contains_key("agent_id") {
        return None;
    }
    let text = |key: &str| obj.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let cwd = text("cwd").unwrap_or_default();
    let title = || title_from_session_file(env, &cwd, home);
    let mut report = match obj.get("hook_event_name")?.as_str()? {
        "SessionStart" => HookReport { title: title(), ..HookReport::new("idle") },
        "UserPromptSubmit" => {
            let prompt = obj.get("prompt").or_else(|| obj.get("user_prompt")).and_then(Value::as_str);
            HookReport { first_prompt: prompt.and_then(|p| transcript::flat_prompt(p, 50)), ..HookReport::new("working") }
        }
        "PermissionRequest" => HookReport { waiting_for: text("tool_name"), ..HookReport::new("waiting") },
        "Notification" => notification(text("notification_type")?.as_str(), text("message"), title)?,
        "Stop" => HookReport {
            title: title(),
            message: text("last_assistant_message").and_then(|m| transcript::flat_answer(&m)),
            ..HookReport::new("idle")
        },
        _ => return None,
    };
    report.session_id = text("session_id");
    Some(report)
}

/// Wie `report_from_json`, als Argumente für `kadrell status` (`ClaudeHook.statusArgv`).
pub fn status_argv(json: &[u8], env: &HashMap<String, String>, home: &str) -> Option<Vec<String>> {
    report_from_json(json, env, home).map(|r| r.to_argv())
}

fn notification(kind: &str, message: Option<String>, title: impl Fn() -> Option<String>) -> Option<HookReport> {
    if kind == "idle_prompt" {
        return Some(HookReport { title: title(), ..HookReport::new("idle") });
    }
    WAITING_NOTIFICATIONS.contains(&kind).then(|| HookReport { waiting_for: message, ..HookReport::new("waiting") })
}

/// Titel aus `<configDir>/sessions/<CLAUDE_PID>.json`, nur ein echter: `nameSource` „derived“ (bzw. ohne das Feld das
/// Muster `<ordner>-<2 hex>`) ist der automatische Name, den Kadrell wie bisher verwirft.
pub fn title_from_session_file(env: &HashMap<String, String>, cwd: &str, home: &str) -> Option<String> {
    let pid = env.get("CLAUDE_PID")?;
    let dir = env.get("CLAUDE_CONFIG_DIR").cloned().unwrap_or_else(|| format!("{home}/.claude"));
    let obj: Map<String, Value> = serde_json::from_slice(&std::fs::read(format!("{dir}/sessions/{pid}.json")).ok()?).ok()?;
    let name = obj.get("name").and_then(Value::as_str).filter(|n| !n.is_empty())?;
    let derived = match obj.get("nameSource").and_then(Value::as_str) {
        Some(source) => source == "derived",
        None => agent::is_auto_name(name, cwd),
    };
    if derived { None } else { Some(name.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;

    fn argv(json: &str) -> Option<Vec<String>> {
        status_argv(json.as_bytes(), &HashMap::new(), "/nonexistent")
    }

    #[test]
    fn events_map_to_status() {
        assert_eq!(argv(r#"{"hook_event_name":"SessionStart","session_id":"s1","session_source":"startup"}"#).unwrap(), ["status", "idle", "--session-id", "s1"]);
        assert_eq!(
            argv(r#"{"hook_event_name":"UserPromptSubmit","session_id":"s1","prompt":"Bitte  die\nTests fixen"}"#).unwrap(),
            ["status", "working", "--first-prompt", "Bitte die Tests fixen", "--session-id", "s1"]
        );
        assert_eq!(
            argv(r#"{"hook_event_name":"PermissionRequest","session_id":"s1","tool_name":"Bash"}"#).unwrap(),
            ["status", "waiting", "--waiting-for", "Bash", "--session-id", "s1"]
        );
        assert_eq!(
            argv(r#"{"hook_event_name":"Notification","session_id":"s1","notification_type":"permission_prompt","message":"Darf ich?"}"#).unwrap(),
            ["status", "waiting", "--waiting-for", "Darf ich?", "--session-id", "s1"]
        );
        // idle_prompt (Claude ~60 s untätig): idle, nicht waiting, sonst kippt eine fertige Session zurück auf "wartet".
        assert_eq!(
            argv(r#"{"hook_event_name":"Notification","session_id":"s1","notification_type":"idle_prompt","message":"Claude is waiting for your input"}"#).unwrap(),
            ["status", "idle", "--session-id", "s1"]
        );
        assert_eq!(
            argv(r#"{"hook_event_name":"Stop","session_id":"s1","last_assistant_message":"**Fertig**, `x` gebaut."}"#).unwrap(),
            ["status", "idle", "--message", "Fertig, x gebaut.", "--session-id", "s1"]
        );
    }

    #[test]
    fn ignores_subagents_and_other_events() {
        assert_eq!(argv(r#"{"hook_event_name":"Stop","session_id":"s1","agent_id":"sub"}"#), None);
        assert_eq!(argv(r#"{"hook_event_name":"PreToolUse","session_id":"s1"}"#), None);
        assert_eq!(argv(r#"{"hook_event_name":"Notification","session_id":"s1","notification_type":"auth_success"}"#), None);
        assert_eq!(argv("{}"), None);
        assert_eq!(argv("kein json"), None);
    }

    /// Ein Slash-Command als erste Eingabe ist kein Ersatztitel, der Status kommt trotzdem an.
    #[test]
    fn prompt_without_title_value() {
        assert_eq!(
            argv(r#"{"hook_event_name":"UserPromptSubmit","session_id":"s1","user_prompt":"<command-name>/clear</command-name>"}"#).unwrap(),
            ["status", "working", "--session-id", "s1"]
        );
    }

    #[test]
    fn title_from_session_file_rules() {
        let dir = TempDir::new("hook");
        let env = HashMap::from([("CLAUDE_PID".to_string(), "777".to_string()), ("CLAUDE_CONFIG_DIR".to_string(), dir.path())]);
        let run = |event: &str| status_argv(format!(r#"{{"hook_event_name":"{event}","session_id":"s1","cwd":"/p/repo"}}"#).as_bytes(), &env, "/nonexistent").unwrap();
        dir.write("sessions/777.json", r#"{"pid":777,"name":"Tests grün machen","nameSource":"haiku"}"#);
        assert_eq!(run("SessionStart"), ["status", "idle", "--title", "Tests grün machen", "--session-id", "s1"]);
        dir.write("sessions/777.json", r#"{"pid":777,"name":"repo-2f","nameSource":"derived"}"#);
        assert_eq!(run("Stop"), ["status", "idle", "--session-id", "s1"]);
        // Ohne `nameSource` (ältere CLI) entscheidet das Namensmuster.
        dir.write("sessions/777.json", r#"{"pid":777,"name":"repo-2f"}"#);
        assert_eq!(run("Stop"), ["status", "idle", "--session-id", "s1"]);
        dir.write("sessions/777.json", r#"{"pid":777,"name":"Echter Titel"}"#);
        assert_eq!(run("Stop"), ["status", "idle", "--title", "Echter Titel", "--session-id", "s1"]);
    }

    #[test]
    fn launch_args_carry_all_events_as_hooks() {
        let args = launch_args();
        assert_eq!(args[0], "--settings");
        let obj: Value = serde_json::from_str(&args[1]).unwrap();
        let hooks = obj["hooks"].as_object().unwrap();
        let mut keys: Vec<&str> = hooks.keys().map(String::as_str).collect();
        keys.sort_unstable();
        let mut expected = EVENTS.to_vec();
        expected.sort_unstable();
        assert_eq!(keys, expected);
        assert!(hooks["Stop"][0]["hooks"][0]["command"].as_str().unwrap().contains("\"$KADRELL\" hook claude"));
        // Schlüssel sortiert wie in Swift (.sortedKeys), Schrägstriche unmaskiert.
        assert!(args[1].starts_with(r#"{"hooks":{"Notification":[{"hooks":[{"command":"#));
    }
}
