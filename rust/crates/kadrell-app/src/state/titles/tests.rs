use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use kadrell_claude::cli::{ClaudeCli, StripMode};
use kadrell_claude::hook::HookReport;
use kadrell_core::{Paths, Profile, Session};
use kadrell_models::http::fake::Fake;
use kadrell_models::{LocalModels, LocalSettings};
use serde_json::{Value, json};

use super::candidate_of;
use crate::state::launch::LaunchEnv;
use crate::state::{AppState, Parts};

struct Dir(std::path::PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let dir = std::env::temp_dir().join(format!("kadrell-titles-{}-{name}", std::process::id()));
        std::fs::create_dir_all(dir.join("work")).unwrap();
        Dir(dir)
    }

    fn work(&self) -> String {
        self.0.join("work").to_string_lossy().replace('\\', "/")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Zustand mit Wegwerfprofil; Sessiontitel an, Modell `ollama|t:1`, HTTP geht an die Attrappe.
fn state_with_fake(dir: &Dir, fake: Arc<Fake>) -> AppState {
    let profile_dir = dir.0.join("profile");
    std::fs::create_dir_all(&profile_dir).unwrap();
    let paths = Paths::new(dir.0.join("home"), dir.0.join("support"), dir.0.join("tmp"));
    let profile = Profile { name: Some("test".into()), dir: profile_dir.clone(), temporary: false, pid: std::process::id() };
    let base = ClaudeCli::new(String::new(), BTreeMap::from([("PATH".to_string(), "x".to_string())]), StripMode::Broad);
    let env = LaunchEnv { claude: None, base, exe: "kadrell".into(), socket: "sock".into(), home: dir.0.join("home").to_string_lossy().into_owned() };
    let mut state = AppState::new(Parts { paths, profile, lock: None, env, endpoint: profile_dir.join("kadrell.endpoint") });
    state.settings.set("local.feature.sessionTitle.enabled", json!(true));
    state.settings.set("local.feature.sessionTitle.model", json!("ollama|t:1"));
    let models = LocalModels::new(fake, LocalSettings::from_settings(&state.settings), BTreeMap::new(), &state.home(), None);
    models.set_launcher(|| false);
    state.models = models;
    state
}

fn ollama_fake() -> Arc<Fake> {
    Arc::new(Fake::new(|r| {
        let reply = |body: &str| Ok((200, body.to_string()));
        match r.url.split("11434").nth(1).unwrap_or("") {
            "/api/chat" => reply(r#"{"message":{"content":"{\"title\":\"Login-Timeout beheben.\"}"}}"#),
            "/api/version" => reply(r#"{"version":"0.9.0"}"#),
            _ => reply("{}"),
        }
    }))
}

/// Bodies der Aufrufe von `/api/chat`: Aufwärmen schickt keine Nachrichten, die Frage zwei (System und Nutzer).
fn chats(fake: &Fake) -> Vec<Value> {
    let n = fake.requests().len();
    (0..n).filter(|i| fake.requests()[*i].url.ends_with("/api/chat")).map(|i| fake.body_json(i)).collect()
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < until, "Zeit abgelaufen: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn report(state: &mut AppState, key: &str, first_prompt: Option<&str>) {
    state.report(key, &HookReport { state: "idle".into(), first_prompt: first_prompt.map(String::from), ..Default::default() });
}

fn local_title(state: &AppState, key: &str) -> Option<String> {
    state.store.sessions.get(key).unwrap().local_title.clone()
}

#[test]
fn status_reports_warm_up_the_model_then_ask_once_and_set_the_title() {
    let dir = Dir::new("flow");
    let fake = ollama_fake();
    let mut state = state_with_fake(&dir, fake.clone());
    let key = state.new_claude_session(&dir.work(), None, true);

    report(&mut state, &key, None);
    wait_for("Aufwärmen", || chats(&fake).iter().any(|b| b["messages"].as_array().is_some_and(Vec::is_empty)));
    assert_eq!(local_title(&state, &key), None, "vor dem ersten Prompt kein Titel");

    report(&mut state, &key, Some("Der Login läuft nach 30 Sekunden ab"));
    wait_for("Titel", || {
        state.apply_titles();
        local_title(&state, &key).is_some()
    });
    assert_eq!(local_title(&state, &key).as_deref(), Some("Login-Timeout beheben"));
    let asked = chats(&fake).into_iter().find(|b| b["messages"].as_array().is_some_and(|m| m.len() == 2)).unwrap();
    assert!(asked["messages"][1]["content"].as_str().unwrap().contains("Prompt: Der Login läuft nach 30 Sekunden ab"));
    assert!(asked["format"].is_object(), "Antwortformat (Schema) geht mit");

    report(&mut state, &key, Some("Der Login läuft nach 30 Sekunden ab"));
    std::thread::sleep(Duration::from_millis(100));
    let asks = chats(&fake).iter().filter(|b| b["messages"].as_array().is_some_and(|m| m.len() == 2)).count();
    assert_eq!(asks, 1, "höchstens eine Frage je Session");
}

#[test]
fn nothing_happens_while_the_function_is_off() {
    let dir = Dir::new("off");
    let fake = ollama_fake();
    let mut state = state_with_fake(&dir, fake.clone());
    state.settings.set("local.feature.sessionTitle.enabled", json!(false));
    state.models.set_settings(LocalSettings::from_settings(&state.settings));
    let key = state.new_claude_session(&dir.work(), None, true);
    report(&mut state, &key, None);
    report(&mut state, &key, Some("irgendwas"));
    std::thread::sleep(Duration::from_millis(150));
    assert!(chats(&fake).is_empty(), "das Modell bleibt unberührt");
}

#[test]
fn only_claude_sessions_have_a_transcript() {
    let claude = Session::new("a", "/p", 1.0, "a", "");
    assert!(candidate_of(&claude).has_transcript);
    let shell = Session::new(&Session::new_shell_id(), "/p", 1.0, "s", "");
    assert!(!candidate_of(&shell).has_transcript);
    let pi = Session::new(&Session::new_pi_id(), "/p", 1.0, "p", "");
    assert!(!candidate_of(&pi).has_transcript);
    let mut remote = Session::new("ssh-x", "/p", 1.0, "r", "");
    remote.host = Some("box".into());
    assert!(!candidate_of(&remote).has_transcript);
    let mut named = claude.clone();
    named.custom_name = Some("Eigener".into());
    assert_eq!(candidate_of(&named).custom_name.as_deref(), Some("Eigener"));
}
