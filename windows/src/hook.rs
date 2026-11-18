//! `kadrell hook claude`: Claude Code ruft seine Hooks mit JSON auf stdin auf und vererbt ihnen die Umgebung der
//! Kachel (`KADRELL_SOCKET`, `KADRELL_SESSION_KEY`, `CLAUDE_PID`). Das Ereignis wird auf `kadrell status …`
//! abgebildet, genau wie `Services/ClaudeHook.swift` auf dem Mac.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;

pub const EVENTS: [&str; 5] = ["SessionStart", "UserPromptSubmit", "PermissionRequest", "Notification", "Stop"];
/// Außerhalb von Kadrell (kein Endpunkt) stumm, sonst dieses Binary. Claude Code führt Hooks unter Windows in
/// Git Bash aus, deshalb dieselbe sh-Zeile wie auf dem Mac.
pub const COMMAND: &str = r#"[ -n "$KADRELL_SOCKET" ] && exec "$KADRELL" hook claude || exit 0"#;
/// `notification_type`-Werte, bei denen Claude blockiert auf den Nutzer wartet. `idle_prompt` heißt untätig.
const WAITING_NOTIFICATIONS: [&str; 3] = ["permission_prompt", "agent_needs_input", "elicitation_dialog"];

/// `--settings <json>` für jeden Claude-Prozess: die Hooks gelten nur für ihn, ohne Eintrag in `~/.claude/settings.json`.
pub fn launch_args() -> Vec<String> {
    let hook = json!({"type": "command", "command": COMMAND, "timeout": 5});
    let hooks: Map<String, Value> =
        EVENTS.iter().map(|e| (e.to_string(), json!([{"matcher": "", "hooks": [hook.clone()]}]))).collect();
    vec!["--settings".into(), json!({ "hooks": hooks }).to_string()]
}

pub fn status_argv(input: &[u8], env: &HashMap<String, String>, home: &Path) -> Option<Vec<String>> {
    let obj: Map<String, Value> = serde_json::from_slice(input).ok()?;
    if obj.contains_key("agent_id") {
        return None;
    }
    let text = |key: &str| obj.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let cwd = text("cwd").unwrap_or_default();
    let title = || title_args(env, &cwd, home);
    let mut args: Vec<String> = match obj.get("hook_event_name")?.as_str()? {
        "SessionStart" => [vec!["idle".into()], title()].concat(),
        "UserPromptSubmit" => {
            let prompt = text("prompt").or_else(|| text("user_prompt")).and_then(|p| flat_prompt(&p, 50));
            [vec!["working".into()], flag("--first-prompt", prompt)].concat()
        }
        "PermissionRequest" => [vec!["waiting".into()], flag("--waiting-for", text("tool_name"))].concat(),
        "Notification" => notification(text("notification_type")?.as_str(), text("message"), title)?,
        "Stop" => [vec!["idle".into()], title(), flag("--message", text("last_assistant_message").and_then(|m| flat_answer(&m)))].concat(),
        _ => return None,
    };
    args.splice(0..0, ["status".to_string()]);
    args.extend(flag("--session-id", text("session_id")));
    Some(args)
}

fn notification(kind: &str, message: Option<String>, title: impl Fn() -> Vec<String>) -> Option<Vec<String>> {
    if kind == "idle_prompt" {
        return Some([vec!["idle".into()], title()].concat());
    }
    WAITING_NOTIFICATIONS.contains(&kind).then(|| [vec!["waiting".into()], flag("--waiting-for", message)].concat())
}

fn flag(name: &str, value: Option<String>) -> Vec<String> {
    value.map(|v| vec![name.to_string(), v]).unwrap_or_default()
}

/// Titel aus `<configDir>/sessions/<CLAUDE_PID>.json`, nur ein echter (nicht der automatische `<ordner>-<2 hex>`).
fn title_args(env: &HashMap<String, String>, cwd: &str, home: &Path) -> Vec<String> {
    let Some(pid) = env.get("CLAUDE_PID") else { return vec![] };
    let dir = env.get("CLAUDE_CONFIG_DIR").map(Into::into).unwrap_or_else(|| home.join(".claude"));
    let Some(obj) = std::fs::read(dir.join("sessions").join(format!("{pid}.json")))
        .ok()
        .and_then(|d| serde_json::from_slice::<Map<String, Value>>(&d).ok())
    else {
        return vec![];
    };
    let Some(name) = obj.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()) else { return vec![] };
    let derived = match obj.get("nameSource").and_then(Value::as_str) {
        Some(source) => source == "derived",
        None => is_auto_name(name, cwd),
    };
    if derived { vec![] } else { vec!["--title".into(), name.into()] }
}

pub fn is_auto_name(name: &str, cwd: &str) -> bool {
    let base = crate::store::base_name(cwd);
    name.strip_prefix(&format!("{base}-"))
        .is_some_and(|rest| rest.len() == 2 && rest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))
}

/// Erste Nachricht als Baumzeile: `[Image #n]` raus, eine Zeile, höchstens `limit` Zeichen.
pub fn flat_prompt(text: &str, limit: usize) -> Option<String> {
    let mut cleaned = text.to_string();
    while let Some(start) = cleaned.find("[Image #") {
        let Some(len) = cleaned[start..].find(']') else { break };
        cleaned.replace_range(start..start + len + 1, "");
    }
    let flat = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() || flat.starts_with('<') {
        return None;
    }
    if flat.chars().count() <= limit {
        return Some(flat);
    }
    Some(flat.chars().take(limit).collect::<String>().trim_end().to_string() + "…")
}

fn flat_answer(text: &str) -> Option<String> {
    let flat = text.replace(['`', '*', '#'], "").split_whitespace().collect::<Vec<_>>().join(" ");
    (!flat.is_empty()).then_some(flat)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(json: &str) -> Option<Vec<String>> {
        status_argv(json.as_bytes(), &HashMap::new(), Path::new("C:/nobody"))
    }

    #[test]
    fn maps_events() {
        assert_eq!(
            run(r#"{"hook_event_name":"UserPromptSubmit","prompt":"Hallo   [Image #1] Welt","session_id":"s"}"#).unwrap(),
            ["status", "working", "--first-prompt", "Hallo Welt", "--session-id", "s"]
        );
        assert_eq!(run(r#"{"hook_event_name":"PermissionRequest","tool_name":"Bash"}"#).unwrap(), ["status", "waiting", "--waiting-for", "Bash"]);
        assert_eq!(run(r#"{"hook_event_name":"Notification","notification_type":"idle_prompt"}"#).unwrap(), ["status", "idle"]);
        assert_eq!(run(r#"{"hook_event_name":"Notification","notification_type":"other"}"#), None);
        assert_eq!(run(r#"{"hook_event_name":"Stop","last_assistant_message":"**fertig** `x`"}"#).unwrap(), ["status", "idle", "--message", "fertig x"]);
        assert_eq!(run(r#"{"hook_event_name":"Stop","agent_id":"sub"}"#), None);
    }

    #[test]
    fn settings_json_has_all_events() {
        let args = launch_args();
        let v: Value = serde_json::from_str(&args[1]).unwrap();
        for e in EVENTS {
            assert_eq!(v["hooks"][e][0]["hooks"][0]["command"], COMMAND);
        }
    }

    #[test]
    fn flat_prompt_cuts() {
        assert_eq!(flat_prompt("<command>", 50), None);
        assert_eq!(flat_prompt("abc def", 4).unwrap(), "abc…");
        assert!(is_auto_name("kadrell-3f", "C:/development/projects/kadrell"));
        assert!(!is_auto_name("kadrell-xyz", "C:/development/projects/kadrell"));
    }
}
