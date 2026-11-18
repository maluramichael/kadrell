use super::*;
use crate::command::LayoutMode;
use crate::handler::{Capture, ResumeBlocker, RunState, SshHosts};
use crate::testing::MockHandler;
use serde_json::{Value, json};
use std::collections::HashMap;

fn req(argv: &[&str], caller: Option<&str>) -> ControlRequest {
    ControlRequest::new(argv.iter().map(|s| s.to_string()).collect(), std::env::temp_dir().to_string_lossy().replace('\\', "/"), caller.map(str::to_string))
}

/// Antwort ohne Nachspiel (Enter von `send` bleibt aus).
fn run(h: &mut MockHandler, argv: &[&str]) -> ControlResponse {
    execute(h, &req(argv, None)).response
}

fn run_as(h: &mut MockHandler, caller: &str, argv: &[&str]) -> ControlResponse {
    execute(h, &req(argv, Some(caller))).response
}

fn stderr(r: ControlResponse) -> String {
    assert_eq!(r.status, 1, "{r:?}");
    r.stderr
}

/// Zwei Gruppen: `a` mit s1 und s2 (laufend), `b` mit s3.
fn two_groups() -> MockHandler {
    let mut h = MockHandler::new();
    for dir in ["ga", "gb"] {
        std::fs::create_dir_all(h.dir.path().join(dir)).unwrap();
    }
    let (a, b) = (h.dir.path().join("ga").to_string_lossy().replace('\\', "/"), h.dir.path().join("gb").to_string_lossy().replace('\\', "/"));
    h.add_running("s1", &a, "eins", 100);
    h.add_running("s2", &a, "zwei", 200);
    h.add_running("s3", &b, "drei", 300);
    h
}

/// Schlüssel rekursiv, Arrays über ihr erstes Element: dieselbe Form wie `docs/contract/ls.json`.
fn shape(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), shape(v))).collect()),
        Value::Array(a) => Value::Array(a.first().map(shape).into_iter().collect()),
        Value::Bool(_) => json!("bool"),
        Value::Number(_) => json!("number"),
        Value::Null | Value::String(_) => json!("string?"),
    }
}

#[test]
fn list_json_matches_contract() {
    let mut h = two_groups();
    h.focused = Some("s1".into());
    h.selected = vec!["s1".into()];
    h.layout = LayoutMode::Spiral;
    h.store.sessions.get_mut("s1").unwrap().branch = Some("master".into());
    h.store.groups.groups[0].favorite = true;
    let r = run(&mut h, &["ls", "--json"]);
    assert_eq!(r.status, 0);
    let ours: Value = serde_json::from_str(&r.stdout).unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../docs/contract/ls.json");
    let contract: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(shape(&ours), shape(&contract));
    assert_eq!(ours["layout"], "spiral");
    assert_eq!(ours["groups"][0]["sessions"][0]["branch"], "master");
    assert_eq!(ours["groups"][0]["sessions"][0]["status"], "idle");
    assert_eq!(ours["groups"][0]["sessions"][0]["sessionId"], "s1");
    assert_eq!(ours["groups"][0]["sessions"][1]["shown"], false);
    // Sortierte Schlüssel wie bei der Mac-App, unabhängig von `preserve_order`.
    let keys: Vec<&str> = r.stdout.lines().filter(|l| l.starts_with("      \"") || l.starts_with("          \"")).map(|l| l.trim().split('"').nth(1).unwrap()).collect();
    let first_session: Vec<&str> = keys.iter().copied().skip_while(|k| *k != "branch").take(9).collect();
    assert_eq!(first_session, ["branch", "cwd", "focused", "key", "running", "sessionId", "shown", "status", "title"]);
    assert!(r.stdout.starts_with("{\n  \"focused\": \"s1\",\n  \"groups\": ["), "{}", r.stdout);
}

#[test]
fn list_json_omits_missing_focus_and_branch() {
    let mut h = two_groups();
    let v: Value = serde_json::from_str(&run(&mut h, &["ls", "--json"]).stdout).unwrap();
    assert!(v.get("focused").is_none());
    assert!(v["groups"][0]["sessions"][0].get("branch").is_none());
    assert_eq!(v["layout"], "grid");
}

#[test]
fn list_json_status_words() {
    let mut h = two_groups();
    h.store.sessions.get_mut("s1").unwrap().raw_status = Some("working".into());
    h.store.sessions.get_mut("s2").unwrap().raw_status = Some("waiting".into());
    h.run.insert("s3".into(), RunState::Ended);
    let v: Value = serde_json::from_str(&run(&mut h, &["ls", "--json"]).stdout).unwrap();
    let status = |g: usize, s: usize| v["groups"][g]["sessions"][s]["status"].as_str().unwrap().to_string();
    assert_eq!((status(0, 0), status(0, 1), status(1, 0)), ("running".to_string(), "waiting".to_string(), "ended".to_string()));
    assert_eq!(v["groups"][1]["sessions"][0]["running"], false);
    h.run.remove("s3");
    let v: Value = serde_json::from_str(&run(&mut h, &["ls", "--json"]).stdout).unwrap();
    assert_eq!(v["groups"][1]["sessions"][0]["status"], "stopped");
}

#[test]
fn list_text_marks_and_pads() {
    let mut h = two_groups();
    h.focused = Some("s1".into());
    h.selected = vec!["s1".into(), "s2".into()];
    h.store.groups.groups[0].favorite = true;
    h.store.sessions.get_mut("s2").unwrap().raw_status = Some("waiting".into());
    let out = run(&mut h, &["ls"]).stdout;
    let lines: Vec<&str> = out.lines().collect();
    let group_a = h.store.groups.groups[0].clone();
    assert_eq!(lines[0], format!("{}  ga ★  {}", &group_a.id[..8], group_a.cwd));
    assert_eq!(lines[1], "  * s1  idle      eins");
    assert_eq!(lines[2], "  + s2  waiting   zwei");
    assert_eq!(lines[4], "    s3  idle      drei");
    assert_eq!(run(&mut MockHandler::new(), &["ls"]).stdout, "");
}

#[test]
fn starting_app_answers_75() {
    let mut h = two_groups();
    h.ready = false;
    let r = run(&mut h, &["ls"]);
    assert_eq!((r.status, r.stdout.as_str(), r.stderr.as_str()), (75, "", "Kadrell startet noch\n"));
}

#[test]
fn parse_errors_become_responses() {
    let mut h = two_groups();
    assert_eq!(stderr(run(&mut h, &["frobnicate"])), "kadrell: unbekannter Befehl „frobnicate“, siehe kadrell help\n");
    assert!(run(&mut h, &["help"]).stdout.contains("kadrell new-group"));
    assert!(run(&mut h, &[]).stdout.starts_with("Kadrell fernsteuern"));
}

/// Ohne Freigabe nur die eigene Gruppe: `send`, `capture`, `stop`, `kill` ...; `ls`, `select`, `layout`, `new` bleiben frei.
#[test]
fn policy_limits_tiles_to_their_group() {
    let mut h = two_groups();
    h.others_allowed = false;
    let denied = "kadrell: „drei“ gehört nicht zur Gruppe dieser Session (Einstellung „Sessions dürfen andere Sessions steuern“)\n";
    for argv in [&["send", "-t", "s3", "x"][..], &["capture", "-t", "s3"], &["stop", "-t", "s3"], &["kill", "-t", "s3"], &["rename", "-t", "s3", "n"], &["resume", "-t", "s3"]] {
        assert_eq!(stderr(run_as(&mut h, "s1", argv)), denied, "{argv:?}");
    }
    assert_eq!(run_as(&mut h, "s1", &["capture", "-t", "s2"]).status, 0, "eigene Gruppe");
    assert_eq!(run_as(&mut h, "s1", &["capture", "-t", "s1"]).status, 0, "sich selbst");
    assert_eq!(run_as(&mut h, "s1", &["ls"]).status, 0);
    assert_eq!(run_as(&mut h, "s1", &["select", "-t", "s3"]).status, 0);
    assert_eq!(run_as(&mut h, "s1", &["layout", "stack"]).status, 0);
    assert_eq!(run(&mut h, &["capture", "-t", "s3"]).status, 0, "von außen nie eingeschränkt");
    // Gruppen: die eigene ja, fremde nein; als Ziel von move ebenso.
    assert!(stderr(run_as(&mut h, "s1", &["kill-group", "-t", "gb"])).starts_with("kadrell: Gruppe „gb“ ist nicht die dieser Session"));
    assert_eq!(stderr(run_as(&mut h, "s1", &["move", "-t", "s1", "gb"])), "kadrell: Gruppe „gb“ ist nicht die dieser Session (Einstellung „Sessions dürfen andere Sessions steuern“)\n");
    // new -t auf eine fremde Gruppe bleibt frei.
    assert_eq!(run_as(&mut h, "s1", &["new", "-d", "-t", "gb"]).status, 0);
    h.others_allowed = true;
    assert_eq!(run_as(&mut h, "s1", &["stop", "-t", "s3"]).status, 0);
}

#[test]
fn caller_default_target_then_focus() {
    let mut h = two_groups();
    h.focused = Some("s1".into());
    assert_eq!(run_as(&mut h, "s2", &["rename", "neu"]).status, 0);
    assert_eq!(h.calls.last().unwrap(), "rename s2 neu");
    assert_eq!(run(&mut h, &["rename", "fokus"]).status, 0);
    assert_eq!(h.calls.last().unwrap(), "rename s1 fokus");
    h.focused = None;
    assert_eq!(stderr(run(&mut h, &["rename", "x"])), "kadrell: keine Session angegeben (-t) und keine fokussiert\n");
    assert_eq!(stderr(run(&mut h, &["kill", "-t", "nix"])), "kadrell: keine Session „nix“\n");
}

#[test]
fn send_pastes_then_enter_after_delay() {
    let mut h = two_groups();
    let out = execute(&mut h, &req(&["send", "-t", "s1", "hallo", "welt"], None));
    assert_eq!(out.response, ControlResponse::ok(""));
    assert_eq!(h.writes, vec![("s1".to_string(), "hallo welt".to_string())]);
    assert_eq!(out.followup, Some(Followup { delay: ENTER_DELAY, key: "s1".into(), data: "\r".into() }), "Enter kommt getrennt");
    finish(&mut h, &out.followup.unwrap());
    assert_eq!(h.writes[1], ("s1".to_string(), "\r".to_string()));

    // Bracketed paste an: Text eingerahmt, ein eigenes Ende im Text fällt weg.
    h.paste.insert("s1".into(), true);
    h.writes.clear();
    execute(&mut h, &req(&["send", "-t", "s1", "a\u{1b}[201~b"], None));
    assert_eq!(h.writes[0].1, "\u{1b}[200~ab\u{1b}[201~");
    // --no-enter: kein Nachspiel.
    assert_eq!(execute(&mut h, &req(&["send", "--no-enter", "-t", "s1", "x"], None)).followup, None);
}

#[test]
fn send_keys_and_not_running() {
    let mut h = two_groups();
    let out = execute(&mut h, &req(&["send", "-t", "s1", "-k", "C-c", "Enter", "Up"], None));
    assert_eq!(out.followup, None);
    assert_eq!(h.writes, vec![("s1".to_string(), "\u{3}\r\u{1b}[A".to_string())]);
    h.paste.remove("s2");
    assert_eq!(stderr(run(&mut h, &["send", "-t", "s2", "x"])), "kadrell: „zwei“ läuft nicht, erst kadrell resume -t s2\n");
}

#[test]
fn send_blocking_waits_for_the_enter() {
    let mut h = two_groups();
    let tree = NoTree;
    let started = std::time::Instant::now();
    let r = handle_blocking(&mut h, &tree, &req(&["send", "-t", "s1", "x"], None), None);
    assert_eq!(r.status, 0);
    assert!(started.elapsed() >= ENTER_DELAY);
    assert_eq!(h.writes.len(), 2, "Text und Enter, bevor die Antwort da ist");
}

#[test]
fn capture_trimming_rules() {
    let mut h = two_groups();
    h.captures.insert("s1".into(), Capture::Rows(vec!["Zeile eins   ".into(), "".into(), "zwei \t".into(), "   ".into(), "".into()]));
    assert_eq!(run(&mut h, &["capture", "-t", "s1"]).stdout, "Zeile eins\n\nzwei\n");
    h.captures.insert("s1".into(), Capture::Rows(vec!["  ".into()]));
    assert_eq!(run(&mut h, &["capture", "-t", "s1"]).stdout, "");
    h.captures.insert("s1".into(), Capture::Buffer("alles  \n\n".into()));
    assert_eq!(run(&mut h, &["capture", "-t", "s1", "--all"]).stdout, "alles  \n\n", "--all unverändert");
    h.captures.insert("s2".into(), Capture::Stopped(vec!["a ".into(), "b".into()]));
    assert_eq!(run(&mut h, &["capture", "-t", "s2"]).stdout, "a \nb\n");
}

#[test]
fn status_needs_target_or_caller() {
    let mut h = two_groups();
    assert_eq!(stderr(run(&mut h, &["status", "working"])), "kadrell: status braucht -t, wenn es nicht aus einer Session kommt\n");
    assert_eq!(run(&mut h, &["status", "waiting", "-t", "eins", "--waiting-for", "Bash"]).status, 0);
    assert_eq!(run_as(&mut h, "s2", &["status", "idle", "--session-id", "x"]).status, 0);
    assert_eq!(h.reported.len(), 2);
    assert_eq!((h.reported[0].0.as_str(), h.reported[0].1.state.as_str(), h.reported[0].1.waiting_for.as_deref()), ("s1", "waiting", Some("Bash")));
    assert_eq!(h.reported[1].0, "s2");
}

#[test]
fn stop_resume_kill() {
    let mut h = two_groups();
    assert_eq!(run(&mut h, &["stop", "-t", "s1"]).status, 0);
    assert_eq!(stderr(run(&mut h, &["stop", "-t", "s1"])), "kadrell: „eins“ läuft nicht\n");
    assert_eq!(run(&mut h, &["resume", "-t", "s1"]).status, 0);
    assert_eq!(run(&mut h, &["kill-session", "-t", "s1"]).status, 0);
    assert_eq!(run(&mut h, &["kill-group", "-t", "gb"]).status, 0);
    assert_eq!(h.calls.iter().filter(|c| c.starts_with("kill")).count(), 2);
    assert!(h.calls.last().unwrap().starts_with("kill_group "));
    assert!(!h.server_killed);
    assert_eq!(run(&mut h, &["kill-server"]).status, 0);
    assert!(h.server_killed);
}

#[test]
fn select_zoom_layout() {
    let mut h = two_groups();
    run(&mut h, &["select", "-t", "s2"]);
    assert_eq!(h.selected, ["s2"]);
    run(&mut h, &["select", "-a", "-t", "s3"]);
    assert_eq!(h.selected, ["s3", "s2"]);
    // Gruppe: alle ihre Sessions.
    run(&mut h, &["select", "-t", "ga"]);
    assert_eq!(h.selected, ["s1", "s2"]);
    assert_eq!(stderr(run(&mut h, &["select", "-t", "zzz"])), "kadrell: keine Session „zzz“\n");
    assert_eq!(run(&mut h, &["layout", "row"]).status, 0);
    assert_eq!(h.layout, LayoutMode::Row);
    // zoom ohne Fokus.
    assert_eq!(stderr(run(&mut h, &["zoom"])), "kadrell: keine Kachel fokussiert\n");
    h.calls.clear();
    h.selected.clear();
    assert_eq!(run(&mut h, &["zoom", "-t", "s3"]).status, 0);
    assert_eq!(h.calls, ["select s3 add=true", "focus s3", "zoom"]);
    h.calls.clear();
    assert_eq!(run(&mut h, &["zoom"]).status, 0);
    assert_eq!(h.calls, ["zoom"], "ohne -t nur umschalten");
}

#[test]
fn groups_new_set_move() {
    let mut h = two_groups();
    let tmp = h.dir.path().to_string_lossy().replace('\\', "/");
    let r = run(&mut h, &["new-group", &tmp, "--name", "Neu", "--color", "ABCDEF"]);
    assert_eq!(r.status, 0, "{}", r.stderr);
    let id = r.stdout.trim().to_string();
    let g = h.store.groups.group(&id).unwrap().clone();
    assert_eq!((g.name.as_str(), g.color.as_str(), g.favorite), ("Neu", "#abcdef", true));
    let nope = h.dir.path().join("nope").to_string_lossy().replace('\\', "/");
    assert_eq!(stderr(run(&mut h, &["new-group", &nope])), format!("kadrell: kein Ordner: {nope}\n"));
    assert_eq!(run(&mut h, &["set-group", "-t", "neu", "--name", "Anders", "--favorite", "off"]).status, 0);
    let g = h.store.groups.group(&id).unwrap();
    assert_eq!((g.name.as_str(), g.color.as_str(), g.favorite), ("Anders", "#abcdef", false), "Farbe bleibt");
    assert_eq!(run(&mut h, &["move", "-t", "s3", "ga"]).status, 0);
    assert_eq!(h.store.groups.group_for_session("s3").unwrap().name, "ga");
    assert_eq!(stderr(run(&mut h, &["move", "-t", "s3", "nix"])), "kadrell: keine Gruppe „nix“\n");
}

#[test]
fn new_session_follows_caller_group_and_folder() {
    let mut h = two_groups();
    let a = h.store.sessions.get("s1").unwrap().cwd.clone();
    let group_a = h.store.groups.group_for_session("s1").unwrap().id.clone();
    // Aus s1 heraus, ohne -t und -c: gleiche Gruppe, gleicher Ordner, Auswahl folgt (kein -d).
    assert_eq!(run_as(&mut h, "s1", &["new", "--name", "Nr. 2", "hallo", "welt"]).status, 0);
    let crate::handler::NewRequest { group, cwd, show, prompt, .. } = last_new(&h);
    assert_eq!((group, cwd, show, prompt), (Some(group_a.clone()), a.clone(), true, Some("hallo welt".to_string())));
    assert!(h.calls.iter().any(|c| c.starts_with("rename ") && c.ends_with(" Nr. 2")));
    // -t Gruppe b: deren Ordner, -d: im Hintergrund.
    assert_eq!(run_as(&mut h, "s1", &["new", "-d", "-t", "gb"]).status, 0);
    let request = last_new(&h);
    assert_eq!((request.cwd.as_str(), request.show), (h.store.groups.group_for_session("s3").unwrap().cwd.as_str(), false));
    // -c geht vor; relative Pfade gegen den Ordner des Aufrufs, `..` aufgelöst.
    let r = ControlRequest::new(vec!["new".into(), "-c".into(), "../ga".into()], format!("{}/gb", h.dir.path().to_string_lossy().replace('\\', "/")), None);
    assert_eq!(execute(&mut h, &r).response.status, 0);
    assert_eq!(last_new(&h).cwd, a);
    // Von außen ohne alles: der Ordner des Aufrufs.
    assert_eq!(run(&mut h, &["new"]).status, 0);
    assert_eq!(last_new(&h).group, None);
    let nope = h.dir.path().join("gibt-es-nicht").to_string_lossy().replace('\\', "/");
    assert_eq!(stderr(run(&mut h, &["new", "-c", &nope])), format!("kadrell: kein Ordner: {nope}\n"));
}

fn last_new(h: &MockHandler) -> crate::handler::NewRequest {
    h.new_requests.last().cloned().expect("new-Aufruf")
}

#[test]
fn new_session_resume_checks() {
    let mut h = two_groups();
    let uuid = "0b7e1c2a-3f4d-4e5f-8a9b-0c1d2e3f4a5b";
    h.blockers.insert(uuid.into(), ResumeBlocker::InKadrell);
    assert_eq!(stderr(run(&mut h, &["new", "--resume", uuid])), format!("kadrell: {uuid} ist schon in Kadrell\n"));
    h.blockers.insert(uuid.into(), ResumeBlocker::NoTranscript);
    assert_eq!(stderr(run(&mut h, &["new", "--resume", uuid])), format!("kadrell: kein Transcript für {uuid}\n"));
    h.blockers.insert(uuid.into(), ResumeBlocker::Running(4242));
    assert_eq!(stderr(run(&mut h, &["new", "--resume", uuid])), format!("kadrell: {uuid} läuft noch (pid 4242), erst dort beenden\n"));
    h.blockers.clear();
    assert_eq!(run(&mut h, &["new", "-d", "--resume", uuid]).status, 0);
    assert_eq!(last_new(&h).resume.as_deref(), Some(uuid));
}

#[test]
fn new_session_pi_and_host() {
    let mut h = two_groups();
    assert_eq!(stderr(run(&mut h, &["new", "--pi"])), "kadrell: --pi braucht pi und ein Modell unter Einstellungen › Modelle für Funktionen › Pi-Sessions\n");
    h.pi = true;
    assert_eq!(run(&mut h, &["new", "--pi", "-d", "Idee"]).status, 0);
    assert!(last_new(&h).pi);
    // --host: eigene Gruppe und Ordner, Name und -d wirken, kein Ordner nötig.
    let r = run(&mut h, &["new", "--host", "vault", "--tmux", "main", "-d", "--name", "Tresor"]);
    assert_eq!(r.status, 0, "{}", r.stderr);
    let request = last_new(&h);
    assert_eq!((request.host.as_deref(), request.tmux.as_deref(), request.show, request.cwd.as_str()), (Some("vault"), Some("main"), false, ""));
    assert!(h.calls.last().unwrap().ends_with(" Tresor"));
    assert_eq!(r.stdout, "ssh-vault\n");
}

#[test]
fn ssh_hosts_output() {
    let mut h = two_groups();
    h.hosts = SshHosts { hosts: vec!["vault".into(), "web".into()], recent: vec!["web".into()] };
    assert_eq!(run(&mut h, &["ssh-hosts"]).stdout, "vault\nweb\n");
    assert_eq!(run(&mut h, &["ssh-hosts", "--json"]).stdout, "{\"hosts\":[\"vault\",\"web\"],\"recent\":[\"web\"]}\n");
    h.hosts = SshHosts::default();
    assert_eq!(run(&mut h, &["ssh-hosts"]).stdout, "");
    assert_eq!(run(&mut h, &["ssh-hosts", "--json"]).stdout, "{\"hosts\":[],\"recent\":[]}\n");
}

/// Prozessbaum ohne Eintrag: jeder Aufrufer ist von außen.
struct NoTree;

impl ProcessTree for NoTree {
    fn parent(&self, _pid: u32) -> Option<u32> {
        None
    }
    fn session_leader(&self, _pid: u32) -> Option<u32> {
        None
    }
}

struct MapTree(HashMap<u32, u32>);

impl ProcessTree for MapTree {
    fn parent(&self, pid: u32) -> Option<u32> {
        self.0.get(&pid).copied()
    }
    fn session_leader(&self, _pid: u32) -> Option<u32> {
        None
    }
}

/// Das mitgeschickte `caller` zählt nie: die Kachel ergibt sich aus der pid des Clients.
#[test]
fn handle_replaces_the_callers_claim() {
    let mut h = two_groups();
    h.others_allowed = false;
    let tree = MapTree(HashMap::from([(5000, 4999), (4999, 100)]));
    // Prozess unter der Kachel von s1 behauptet, s3 zu sein: gilt als s1, darf s3 nicht anfassen.
    let r = handle(&mut h, &tree, &req(&["capture", "-t", "s3"], Some("s3")), Some(5000)).response;
    assert_eq!(r.status, 1, "{r:?}");
    // Dasselbe von einem fremden Prozess aus: von außen, nicht eingeschränkt, ein behaupteter Aufrufer bleibt wirkungslos.
    let r = handle(&mut h, &tree, &req(&["capture", "-t", "s3"], Some("s1")), Some(7777)).response;
    assert_eq!(r.status, 0, "{r:?}");
    // Ohne pid: ebenfalls von außen. Und ohne -t gilt weder der behauptete Aufrufer noch ein erfundener.
    let r = handle(&mut h, &tree, &req(&["rename", "x"], Some("s2")), None).response;
    assert_eq!(stderr(r), "kadrell: keine Session angegeben (-t) und keine fokussiert\n");
    // Aus der Kachel: ohne -t die eigene Session.
    assert_eq!(handle(&mut h, &tree, &req(&["rename", "mein"], None), Some(5000)).response.status, 0);
    assert_eq!(h.calls.last().unwrap(), "rename s1 mein");
}

/// Zwei `kadrell send` hintereinander laufen nie ineinander: die Antwort kommt erst nach dem Enter.
#[test]
fn controller_runs_calls_one_after_the_other() {
    use std::sync::{Arc, Mutex};
    let h = Arc::new(Mutex::new(two_groups()));
    let controller = Controller::with_tree(h.clone(), Box::new(NoTree));
    assert_eq!(controller.call(&req(&["send", "-t", "s1", "foo"], None), None).status, 0);
    assert_eq!(controller.call(&req(&["send", "-t", "s1", "bar"], None), None).status, 0);
    let order: Vec<String> = h.lock().unwrap().writes.iter().map(|(_, d)| d.clone()).collect();
    assert_eq!(order, ["foo", "\r", "bar", "\r"]);
}
