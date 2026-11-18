//! Tests des Zustands und des Steuerkanals darauf, mit einer Attrappe statt echter Terminals.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use kadrell_claude::cli::{ClaudeCli, StripMode};
use kadrell_control::protocol::ControlRequest;
use kadrell_control::{ControlHandler, ProcessTree, execute};
use kadrell_core::{Paths, Profile};

use super::launch::LaunchEnv;
use super::{AppState, Parts, Run, Term};

/// Terminal ohne Prozess: merkt sich Eingaben und zeigt feste Zeilen.
struct FakeTerm {
    pid: u32,
    written: RefCell<Vec<u8>>,
    exited: Cell<bool>,
    paste: bool,
    rows: Vec<String>,
}

impl FakeTerm {
    fn new(pid: u32) -> Rc<FakeTerm> {
        Rc::new(FakeTerm { pid, written: RefCell::new(Vec::new()), exited: Cell::new(false), paste: true, rows: vec!["hallo  ".into(), String::new(), String::new()] })
    }
}

impl Term for FakeTerm {
    fn pid(&self) -> Option<u32> {
        Some(self.pid)
    }
    fn exited(&self) -> bool {
        self.exited.get()
    }
    fn write(&self, bytes: &[u8]) {
        self.written.borrow_mut().extend_from_slice(bytes);
    }
    fn bracketed_paste(&self) -> bool {
        self.paste
    }
    fn visible_rows(&self) -> Vec<String> {
        self.rows.clone()
    }
    fn buffer_text(&self) -> String {
        format!("verlauf\n{}", self.rows.join("\n"))
    }
    fn shutdown(&self) {
        self.exited.set(true);
    }
}

struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("kadrell-app-test-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(dir.join("work")).unwrap();
        Dir(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn state_in(dir: &Dir) -> AppState {
    let profile_dir = dir.0.join("profile");
    std::fs::create_dir_all(&profile_dir).unwrap();
    let paths = Paths::new(dir.0.join("home"), dir.0.join("support"), dir.0.join("tmp"));
    let profile = Profile { name: Some("test".into()), dir: profile_dir.clone(), temporary: false, pid: std::process::id() };
    let base = ClaudeCli::new(String::new(), BTreeMap::from([("PATH".to_string(), "x".to_string())]), StripMode::Broad);
    let claude = Some(ClaudeCli::new("claude", base.environment.clone(), StripMode::Broad));
    let env = LaunchEnv { claude, base, exe: "kadrell".into(), socket: "sock".into(), home: dir.0.join("home").to_string_lossy().into_owned() };
    AppState::new(Parts { paths, profile, lock: None, env, endpoint: profile_dir.join("kadrell.endpoint") })
}

fn cwd(dir: &Dir, name: &str) -> String {
    let p = dir.0.join("work").join(name);
    std::fs::create_dir_all(&p).unwrap();
    p.to_string_lossy().replace('\\', "/")
}

/// Startet alles Vorgemerkte mit Attrappen, wie es die Oberfläche mit echten Terminals tut.
fn start_all(state: &mut AppState) -> Vec<(String, Rc<FakeTerm>)> {
    let mut out = Vec::new();
    for (i, (key, spec)) in state.take_starts().into_iter().enumerate() {
        if spec.is_ok() {
            let term = FakeTerm::new(100 + i as u32);
            state.attach(&key, term.clone());
            out.push((key, term));
        }
    }
    out
}

struct NoTree;
impl ProcessTree for NoTree {
    fn parent(&self, _: u32) -> Option<u32> {
        None
    }
    fn session_leader(&self, _: u32) -> Option<u32> {
        None
    }
}

fn run(state: &mut AppState, argv: &[&str]) -> kadrell_control::ControlResponse {
    let req = ControlRequest::new(argv.iter().map(|s| s.to_string()).collect(), state.paths.home.to_string_lossy().into_owned(), None);
    execute(state, &req).response
}

#[test]
fn new_session_is_stored_grouped_shown_and_started_lazily() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let a = state.new_claude_session(&cwd(&dir, "alpha"), None, true);
    let b = state.new_claude_session(&cwd(&dir, "beta"), None, false);
    assert_eq!(state.store.groups.groups.len(), 2);
    assert_eq!((state.selected.clone(), state.focused.clone()), (vec![a.clone()], Some(a.clone())));
    let started = start_all(&mut state);
    assert_eq!(started.len(), 2, "beide neu angelegten starten, auch die nicht gezeigte");
    assert_eq!((state.run(&a), state.run(&b)), (Run::Running, Run::Running));
    // Neu gestartet wird mit --session-id, nicht --resume.
    state.stop_tile(&b);
    state.request_start(&b).unwrap();
    let (_, spec) = state.take_starts().remove(0);
    assert_eq!(&spec.unwrap().args[..2], ["--session-id", b.as_str()]);
}

#[test]
fn persisted_sessions_start_only_when_shown() {
    let dir = Dir::new();
    let (a, b) = {
        let mut state = state_in(&dir);
        let a = state.new_claude_session(&cwd(&dir, "alpha"), None, true);
        let b = state.new_claude_session(&cwd(&dir, "beta"), None, false);
        (a, b)
    };
    let mut state = state_in(&dir);
    assert_eq!(state.selected, std::slice::from_ref(&a), "die gespeicherte Auswahl kommt zurück");
    let started = start_all(&mut state);
    assert_eq!(started.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(), std::slice::from_ref(&a));
    assert_eq!(state.run(&b), Run::NotStarted);
    state.show_only(&b);
    assert_eq!(start_all(&mut state).len(), 1, "erst beim Zeigen startet die versteckte");
}

#[test]
fn exit_keeps_the_tile_or_closes_it_by_setting_and_shells_vanish() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let a = state.new_claude_session(&cwd(&dir, "alpha"), None, true);
    let b = state.new_claude_session(&cwd(&dir, "beta"), None, false);
    state.select_keys(std::slice::from_ref(&b), true);
    let terms = start_all(&mut state);
    terms.iter().find(|(k, _)| *k == a).unwrap().1.exited.set(true);
    assert!(state.on_exit(&a));
    assert_eq!(state.run(&a), Run::Ended);
    assert_eq!(state.selected.len(), 2, "Standard: Kachel bleibt, Klick setzt fort");

    state.resume_for_test(&a);
    state.settings.set("closeTileOnExit", serde_json::Value::Bool(true));
    start_all(&mut state);
    assert_eq!(state.run(&a), Run::Running);
    state.on_exit(&a);
    assert_eq!(state.selected, std::slice::from_ref(&b), "Einstellung: Kachel verschwindet aus der Ansicht");

    let shell = state.new_shell_session(&cwd(&dir, "alpha"), true);
    start_all(&mut state);
    state.on_exit(&shell);
    assert!(state.session(&shell).is_none(), "exit in einem Terminal ohne Claude entfernt den Eintrag");
}

impl AppState {
    fn resume_for_test(&mut self, key: &str) {
        self.request_start(key).unwrap();
    }
}

#[test]
fn removing_sessions_kills_terminals_and_archives() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let a = state.new_claude_session(&cwd(&dir, "alpha"), None, true);
    let b = state.new_claude_session(&cwd(&dir, "beta"), None, false);
    let terms = start_all(&mut state);
    state.remove_sessions(std::slice::from_ref(&a));
    assert!(terms.iter().find(|(k, _)| *k == a).unwrap().1.exited.get());
    assert!(state.session(&a).is_none());
    assert_eq!(state.store.sessions.closed.len(), 1);
    assert_eq!(state.selected, [b], "ohne Auswahl rückt die erste Session nach");
}

#[test]
fn select_adds_toggles_and_keeps_the_last() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let a = state.new_claude_session(&cwd(&dir, "alpha"), None, true);
    let b = state.new_claude_session(&cwd(&dir, "beta"), None, false);
    state.select_keys(std::slice::from_ref(&b), true);
    assert_eq!(state.selected, [a.clone(), b.clone()]);
    assert_eq!(state.focused.as_deref(), Some(b.as_str()));
    state.select_keys(std::slice::from_ref(&b), true);
    assert_eq!(state.selected, std::slice::from_ref(&a));
    state.select_keys(std::slice::from_ref(&a), true);
    assert_eq!(state.selected, [a], "die letzte Session bleibt");
}

#[test]
fn registry_report_sets_status_title_and_unseen_marker() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let a = state.new_claude_session(&cwd(&dir, "alpha"), None, true);
    let b = state.new_claude_session(&cwd(&dir, "beta"), None, false);
    start_all(&mut state);
    state.window_active = true;
    let report = |state: &mut AppState, key: &str, s: &str, title: Option<&str>| {
        let r = kadrell_claude::hook::HookReport { state: s.into(), title: title.map(String::from), ..Default::default() };
        state.report(key, &r);
    };
    report(&mut state, &b, "working", None);
    assert_eq!(state.display_status(&b), Some(kadrell_core::SessionStatus::Running));
    report(&mut state, &b, "idle", Some("Tests grün"));
    assert_eq!(state.session(&b).unwrap().title(), "Tests grün");
    assert!(state.is_unseen(&b), "fertig, aber nicht angesehen: Marke neu");
    state.show_only(&b);
    state.update_attention();
    assert!(!state.is_unseen(&b), "angesehen: Marke weg");
    assert_eq!(state.display_status(&a), Some(kadrell_core::SessionStatus::Idle));
}

#[test]
fn control_new_ls_send_capture_kill_round_trip() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    state.ready = true;
    let work = cwd(&dir, "probe");
    let r = run(&mut state, &["new", "-c", &work, "--name", "probe"]);
    assert_eq!(r.status, 0, "{}", r.stderr);
    let key = r.stdout.trim().to_string();
    assert!(!key.is_empty());
    let terms = start_all(&mut state);
    assert_eq!(terms.len(), 1);

    let ls = run(&mut state, &["ls", "--json"]);
    let v: serde_json::Value = serde_json::from_str(&ls.stdout).unwrap();
    let s = &v["groups"][0]["sessions"][0];
    assert_eq!((s["title"].as_str(), s["running"].as_bool(), s["shown"].as_bool(), s["status"].as_str()), (Some("probe"), Some(true), Some(true), Some("idle")));
    assert_eq!(v["layout"], "grid");

    let sent = run(&mut state, &["send", "-t", "probe", "echo hi"]);
    assert_eq!(sent.status, 0, "{}", sent.stderr);
    assert_eq!(String::from_utf8_lossy(&terms[0].1.written.borrow()), "\u{1b}[200~echo hi\u{1b}[201~", "bracketed paste, Enter folgt später");
    let cap = run(&mut state, &["capture", "-t", "probe"]);
    assert_eq!(cap.stdout.trim_end(), "hallo");

    assert_eq!(run(&mut state, &["stop", "-t", "probe"]).status, 0);
    assert_eq!(run(&mut state, &["capture", "-t", "probe"]).stdout.trim_end(), "hallo", "gestoppt: zuletzt gemerkte Zeilen");
    let ls = run(&mut state, &["ls"]);
    assert!(ls.stdout.contains("stopped"), "{}", ls.stdout);

    assert_eq!(run(&mut state, &["resume", "-t", "probe"]).status, 0);
    assert_eq!(state.take_starts().len(), 1);
    assert_eq!(run(&mut state, &["kill", "-t", "probe"]).status, 0);
    assert!(state.session(&key).is_none());
}

#[test]
fn control_layout_select_rename_move_and_groups() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    state.ready = true;
    let a = state.new_claude_session(&cwd(&dir, "alpha"), None, true);
    let b = state.new_claude_session(&cwd(&dir, "beta"), None, false);
    assert_eq!(run(&mut state, &["layout", "stack"]).status, 0);
    assert_eq!(state.layout.as_str(), "stack");
    assert_eq!(run(&mut state, &["select", "-t", &b, "-a"]).status, 0);
    assert_eq!(state.selected, [a.clone(), b.clone()]);
    assert_eq!(run(&mut state, &["rename", "-t", &b, "Zwei"]).status, 0);
    assert_eq!(state.session(&b).unwrap().title(), "Zwei");
    let group_a = state.store.groups.group_for_session(&a).unwrap().id.clone();
    assert_eq!(run(&mut state, &["move", "-t", &b, &group_a]).status, 0);
    assert_eq!(state.store.groups.groups.len(), 1, "die leere Gruppe ohne Herz verschwindet");
    let g = run(&mut state, &["new-group", &cwd(&dir, "gamma"), "--name", "Gamma"]);
    assert_eq!(g.status, 0, "{}", g.stderr);
    assert!(state.store.groups.groups.iter().any(|g| g.name == "Gamma" && g.favorite));
}

#[test]
fn control_starting_status_until_ready_and_kill_server() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    assert_eq!(run(&mut state, &["ls"]).status, kadrell_control::STARTING_STATUS);
    state.ready = true;
    assert_eq!(run(&mut state, &["kill-server"]).status, 0);
    assert!(state.quit_requested);
    let _ = NoTree;
}

#[test]
fn caller_tile_is_found_from_the_process_tree_pids() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let a = state.new_claude_session(&cwd(&dir, "alpha"), None, true);
    start_all(&mut state);
    assert_eq!(state.tile_pids(), [(a.clone(), 100)]);
    assert_eq!(state.pids().get(&100), Some(&a));
}

#[test]
fn applied_settings_reach_the_profile_file_without_losing_foreign_keys() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let path = state.profile.settings_path();
    // Ein Schlüssel, den eine andere Stelle direkt in die Datei schreibt, soll die Übernahme überleben.
    super::attention::patch_setting(&path, "foreign.key", serde_json::json!(1));
    let mut new = state.settings.clone();
    new.set_claude_model("haiku");
    state.apply_settings(new);
    let saved = kadrell_core::Settings::load(&path);
    assert_eq!(saved.claude_model(), "haiku");
    assert_eq!(saved.get("foreign.key"), Some(&serde_json::json!(1)));
    assert_eq!(state.settings.claude_model(), "haiku");
}

// MARK: Tray, Benachrichtigung, Klang und Badge (Regeln in `tray_wire`)

use kadrell_claude::attention::{Effect, Sound as TrackerSound};
use kadrell_core::settings::{NotificationLevel, SoundLevel};
use kadrell_tray::model::{Lang, Sound, TrayItem};

use crate::tray_wire::rules::{Action, plan};
use crate::tray_wire::view;

fn report_status(state: &mut AppState, key: &str, status: &str) {
    state.report(key, &kadrell_claude::hook::HookReport { state: status.into(), ..Default::default() });
}

/// Zwei laufende Sessions, `a` gezeigt und im Fokus, `b` nur im Baum; das Fenster ist vorn oder nicht.
fn two_running(dir: &Dir, window_active: bool) -> (AppState, String, String) {
    let mut state = state_in(dir);
    let a = state.new_claude_session(&cwd(dir, "alpha"), None, true);
    let b = state.new_claude_session(&cwd(dir, "beta"), None, false);
    start_all(&mut state);
    state.window_active = window_active;
    report_status(&mut state, &a, "working");
    report_status(&mut state, &b, "working");
    state.take_effects();
    (state, a, b)
}

fn actions(state: &mut AppState) -> Vec<Action> {
    plan(&state.take_effects(), SoundLevel::All, NotificationLevel::All)
}

#[test]
fn waiting_session_out_of_sight_notifies_plays_and_counts() {
    let dir = Dir::new();
    let (mut state, _a, b) = two_running(&dir, true);
    report_status(&mut state, &b, "waiting");
    let todo = actions(&mut state);
    assert_eq!(todo, [Action::Play(Sound::Waiting), Action::Notify { key: b.clone(), waiting: true }]);
    let v = view::build(&state, Lang::De, super::now_secs());
    assert_eq!((v.summary.waiting, v.summary.done, v.summary.working, v.badge), (1, 0, 1, 1));
    assert_eq!(v.waiting, std::slice::from_ref(&b));
    assert!(matches!(&v.menu[0], TrayItem::Entry { id, label, .. } if *id == format!("session:{b}") && label.starts_with("beta › ")), "{:?}", v.menu[0]);
}

#[test]
fn waiting_session_in_view_of_the_active_window_stays_quiet() {
    let dir = Dir::new();
    let (mut state, a, _b) = two_running(&dir, true);
    report_status(&mut state, &a, "waiting");
    assert_eq!(actions(&mut state), [], "die Fokus-Kachel im vorderen Fenster sieht man schon");
    assert_eq!(view::build(&state, Lang::De, 0.0).badge, 1, "wartend zählt trotzdem im Badge");
}

#[test]
fn hidden_window_notifies_even_for_the_focused_session() {
    let dir = Dir::new();
    let (mut state, a, _b) = two_running(&dir, false);
    report_status(&mut state, &a, "waiting");
    let todo = actions(&mut state);
    assert!(todo.contains(&Action::Notify { key: a.clone(), waiting: true }), "{todo:?}");
    assert!(todo.contains(&Action::RequestAttention), "{todo:?}");
    assert!(state.is_unseen(&a));
}

#[test]
fn finished_session_follows_the_notification_setting_and_clears_when_seen() {
    let dir = Dir::new();
    let (mut state, _a, b) = two_running(&dir, true);
    report_status(&mut state, &b, "idle");
    let effects = state.take_effects();
    assert_eq!(plan(&effects, SoundLevel::All, NotificationLevel::All), [Action::Play(Sound::Done), Action::Notify { key: b.clone(), waiting: false }]);
    assert_eq!(plan(&effects, SoundLevel::Important, NotificationLevel::Waiting), [Action::Play(Sound::Done)], "fertig meldet nur „wartet und fertig“");
    assert_eq!(plan(&effects, SoundLevel::Off, NotificationLevel::Off), []);
    let v = view::build(&state, Lang::En, 0.0);
    assert_eq!((v.summary.done, v.badge), (1, 1), "fertig und ungesehen: „neu“ und Badge");
    state.show_only(&b);
    state.update_attention();
    let v = view::build(&state, Lang::En, 0.0);
    assert_eq!((v.summary.done, v.badge), (0, 0), "angesehen: Badge weg");
}

#[test]
fn effects_are_handed_over_once_in_tracker_order() {
    let dir = Dir::new();
    let (mut state, _a, b) = two_running(&dir, true);
    report_status(&mut state, &b, "waiting");
    let effects = state.take_effects();
    assert!(matches!(effects.first(), Some(Effect::SetBadge(1))), "{effects:?}");
    assert!(effects.contains(&Effect::Play(TrackerSound::Waiting)));
    assert!(state.take_effects().is_empty());
}

#[test]
fn focus_any_moves_the_focus_outside_the_selection_and_remembers_the_old_one() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let keys: Vec<String> = ["a", "b", "c"].iter().map(|n| state.new_claude_session(&cwd(&dir, n), None, false)).collect();
    state.show_only(&keys[0]);
    state.focus_any(&keys[2]);
    assert_eq!(state.focused.as_deref(), Some(keys[2].as_str()));
    assert_eq!(state.selected, vec![keys[0].clone()], "die Auswahl bleibt");
    assert_eq!(state.last_focused.as_deref(), Some(keys[0].as_str()));
    state.focus_any("gibt-es-nicht");
    assert_eq!(state.focused.as_deref(), Some(keys[2].as_str()));
}

#[test]
fn set_selection_remembers_the_previous_focus() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    let keys: Vec<String> = ["a", "b"].iter().map(|n| state.new_claude_session(&cwd(&dir, n), None, false)).collect();
    state.show_only(&keys[0]);
    state.show_only(&keys[1]);
    assert_eq!(state.last_focused.as_deref(), Some(keys[0].as_str()));
    state.show_only(&keys[1]);
    assert_eq!(state.last_focused.as_deref(), Some(keys[0].as_str()), "gleicher Fokus ändert nichts");
}

#[test]
fn workspace_settings_are_written_one_key_at_a_time_and_removed_with_none() {
    let dir = Dir::new();
    let mut state = state_in(&dir);
    state.save_workspace_setting("layout.main.cols", Some(serde_json::json!([0.6, 0.4])));
    state.save_workspace_setting("workspace.custom.splits", Some(serde_json::json!("dr")));
    let saved = kadrell_core::Settings::load(&state.profile.settings_path());
    assert_eq!(saved.string_or("workspace.custom.splits", ""), "dr");
    assert!(saved.get("layout.main.cols").is_some());
    state.save_workspace_setting("layout.main.cols", None);
    assert!(kadrell_core::Settings::load(&state.profile.settings_path()).get("layout.main.cols").is_none());
    assert!(state.settings.get("layout.main.cols").is_none());
}
