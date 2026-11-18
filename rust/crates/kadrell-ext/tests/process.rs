//! `ExtensionProcess`: Start, Protokoll und Überwachung eines Extension-Helpers (Port von `ExtensionProcessTests.swift`).

mod common;

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use common::*;
use kadrell_ext::message::{ExtensionMessage as M, HostMessage};
use kadrell_ext::process::{
    EventLog, ExtensionProcess, Kill, ProcessConfig, ProcessEvent, is_skipped_summary,
};
use serde_json::{Map, Value, json};

struct Launched {
    p: Arc<ExtensionProcess>,
    log: EventLog,
    _root: TempDir,
}

fn hello(dir: &std::path::Path) -> HostMessage {
    HostMessage::Hello {
        api: 1,
        name: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dir: dir.to_string_lossy().into_owned(),
        storage_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        config: Map::new(),
        locale: "de".into(),
        sessions: vec![],
    }
}

fn host_config() -> ProcessConfig {
    let mut c = ProcessConfig::new(HOST_EXE);
    c.extra_environment = helper_env();
    c
}

fn fake_config(mode: &str) -> ProcessConfig {
    let mut c = ProcessConfig::new(FAKE_EXE);
    c.extra_environment = vec![("FAKE_MODE".into(), mode.into())];
    c
}

/// Startet mit eigenem Handler (der den Prozess über ein `OnceLock` erreichen kann).
fn launch_with(
    config: &ProcessConfig,
    init_lua: &str,
    extra: impl Fn(&ProcessEvent, &ExtensionProcess) + Send + Sync + 'static,
) -> Launched {
    let root = TempDir::new("proc");
    let dir = make_extension(root.path(), "p", init_lua, None);
    let log = EventLog::new();
    let cell: Arc<OnceLock<Arc<ExtensionProcess>>> = Arc::new(OnceLock::new());
    let inner = log.handler();
    let c2 = Arc::clone(&cell);
    let handler: kadrell_ext::process::EventHandler = Arc::new(move |e| {
        if let Some(p) = c2.get() {
            extra(&e, p);
        }
        inner(e);
    });
    let p = Arc::new(ExtensionProcess::spawn(config, &dir, hello(&dir), handler).expect("spawn"));
    let _ = cell.set(Arc::clone(&p));
    Launched {
        p,
        log,
        _root: root,
    }
}

fn launch(config: &ProcessConfig, init_lua: &str) -> Launched {
    launch_with(config, init_lua, |_, _| {})
}

fn launch_lua(init_lua: &str) -> Launched {
    launch(&host_config(), init_lua)
}

/// Startet die Lua-Extension, wartet auf `ready` und schickt `app.ready`.
fn launch_ready(init_lua: &str) -> Launched {
    let l = launch_lua(init_lua);
    assert!(
        wait_until(3.0, || l.log.messages().contains(&M::Ready)),
        "{:?}",
        l.log.events()
    );
    l.p.send(HostMessage::Event {
        name: "app.ready".into(),
        data: json!({}),
    });
    l
}

fn has_panel(l: &Launched, title: &str) -> bool {
    l.log
        .messages()
        .iter()
        .any(|m| matches!(m, M::Panel { tree: Some(t), .. } if t["title"] == title))
}

impl Drop for Launched {
    fn drop(&mut self) {
        self.p.kill();
        wait_until(3.0, || !self.log.exits().is_empty());
    }
}

#[test]
fn hello_delivers_ready_and_panel() {
    let l = launch_ready(
        r#"kadrell.on("app.ready", function() kadrell.panel.set{title = "A", children = {}} end)"#,
    );
    assert!(
        wait_until(3.0, || has_panel(&l, "A")),
        "{:?}",
        l.log.events()
    );
    assert_eq!(l.log.messages().first(), Some(&M::Ready));
}

#[test]
fn crash_reports_unexpected_exit() {
    let l = launch_lua(r#"error("x")"#);
    assert!(wait_until(5.0, || !l.log.exits().is_empty()));
    let (expected, reason) = l.log.exits()[0].clone();
    assert!(!expected);
    assert!(reason.contains("init.lua:1: x"), "{reason}");
}

#[test]
fn infinite_loop_is_killed_by_ping() {
    let l = launch_ready(r#"kadrell.on("app.ready", function() while true do end end)"#);
    assert!(wait_until(14.0, || !l.log.exits().is_empty()));
    // Ende kommt erst, wenn der Prozess beendet und eingesammelt ist.
    let (expected, reason) = l.log.exits()[0].clone();
    assert!(!expected);
    assert_eq!(reason, Kill::NoPong.reason());
    assert!(reason.contains("5 s"));
}

/// Aus Lua lässt sich keine Flut mehr erzeugen (Panel und Status zusammengefasst, run zählt nicht), also ein fremdes
/// Programm, das 1000 Statuszeilen schreibt.
#[test]
fn flood_is_killed() {
    let l = launch(&fake_config("flood"), "");
    assert!(wait_until(5.0, || !l.log.exits().is_empty()));
    let (expected, reason) = l.log.exits()[0].clone();
    assert!(!expected);
    assert_eq!(reason, Kill::Flood.reason());
    assert!(
        l.log.messages().len() <= 51,
        "nach der Grenze kommt nichts mehr durch: {}",
        l.log.messages().len()
    );
}

/// Tausend panel.set in einem Handler gehen als ein Panel raus, kein Flut-Kill.
#[test]
fn panel_set_loop_is_coalesced() {
    let l = launch_ready(
        r#"kadrell.on("app.ready", function() for i = 1, 1000 do kadrell.panel.set{title = tostring(i)} end end)"#,
    );
    assert!(
        wait_until(3.0, || has_panel(&l, "1000")),
        "{:?}",
        l.log.events()
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(l.log.exits().is_empty());
    assert_eq!(
        l.log
            .messages()
            .iter()
            .filter(|m| matches!(m, M::Panel { .. }))
            .count(),
        1
    );
}

/// run wartet auf sein Ergebnis und zählt nicht zur Flutgrenze, auch 200 hintereinander nicht.
#[test]
fn sequential_runs_are_not_flood() {
    let l = launch_with(
        &host_config(),
        r#"kadrell.on("app.ready", function() for i = 1, 200 do kadrell.run("x", i) end kadrell.log("fertig") end)"#,
        |e, p| {
            if let ProcessEvent::Message(M::Run { id, .. }) = e {
                p.send(HostMessage::Result {
                    id: *id,
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
        },
    );
    assert!(wait_until(3.0, || l.log.messages().contains(&M::Ready)));
    l.p.send(HostMessage::Event {
        name: "app.ready".into(),
        data: json!({}),
    });
    let done = M::Log {
        level: "info".into(),
        text: "fertig".into(),
    };
    assert!(wait_until(8.0, || l.log.messages().contains(&done)
        || !l.log.exits().is_empty()));
    assert!(l.log.exits().is_empty(), "{:?}", l.log.exits());
    assert!(l.log.messages().contains(&done));
}

fn exec_child(l: &Launched) -> Option<u32> {
    wait_until(3.0, || !children_of(l.p.pid()).is_empty());
    children_of(l.p.pid()).first().copied()
}

fn sleeping_child_lua() -> String {
    format!(
        r#"kadrell.on("app.ready", function() kadrell.exec({{"{}", "sleep", "77"}}) end)"#,
        fake_lua()
    )
}

/// Ein laufendes kadrell.exec stirbt mit dem Helper, auch beim Abschalten.
#[test]
fn stop_kills_exec_children() {
    let l = launch_ready(&sleeping_child_lua());
    let child = exec_child(&l).expect("Kind");
    l.p.stop();
    assert!(wait_until(4.0, || !l.log.exits().is_empty()));
    assert!(
        wait_until(3.0, || !alive(child)),
        "sleep 77 ({child}) lebt nach stop() weiter"
    );
}

/// Auch beim harten Kill wegen Hängers (kein Pong) bleibt kein Kind übrig.
#[test]
fn ping_kill_kills_exec_children() {
    let l = launch_ready(&format!(
        r#"kadrell.on("app.ready", function() kadrell.exec({{"{}", "sleep", "77"}}) end)
        kadrell.on("hang", function() while true do end end)"#,
        fake_lua()
    ));
    let child = exec_child(&l).expect("Kind");
    l.p.send(HostMessage::Event {
        name: "hang".into(),
        data: json!({}),
    });
    assert!(wait_until(14.0, || !l.log.exits().is_empty()));
    assert_eq!(l.log.exits()[0].1, Kill::NoPong.reason());
    assert!(
        wait_until(3.0, || !alive(child)),
        "sleep 77 ({child}) lebt nach dem Kill weiter"
    );
}

/// Log-Zeilen zählen nicht zur Flutgrenze, sie werden gedrosselt wie stderr: höchstens 50 pro Sekunde plus Sammelzeile.
#[test]
fn log_flood_is_throttled_not_killed() {
    let l = launch_ready(
        r#"kadrell.on("app.ready", function() for i = 1, 1000 do kadrell.log(i) end end)"#,
    );
    std::thread::sleep(Duration::from_millis(1500));
    let logs = l
        .log
        .messages()
        .iter()
        .filter(|m| matches!(m, M::Log { .. }))
        .count();
    assert!(logs <= 50, "{logs}");
    assert!(
        l.log.stderr().iter().any(|s| is_skipped_summary(s)),
        "{:?}",
        l.log.stderr()
    );
    assert!(l.log.exits().is_empty());
    l.p.send(HostMessage::Ping(99));
    assert!(wait_until(3.0, || l.log.messages().contains(&M::Pong(99))));
}

#[test]
fn log_text_is_clipped() {
    let l = launch_ready(
        r#"kadrell.on("app.ready", function() kadrell.log(string.rep("a", 2000)) end)"#,
    );
    assert!(wait_until(3.0, || l.log.messages().len() >= 2));
    assert_eq!(
        l.log.messages().last(),
        Some(&M::Log {
            level: "info".into(),
            text: "a".repeat(500)
        })
    );
}

#[test]
fn garbage_line_is_ignored() {
    let l = launch(&fake_config("garbage"), "");
    assert!(wait_until(3.0, || l.log.messages().contains(&M::Ready)));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        l.log.events(),
        vec![
            ProcessEvent::Stderr(kadrell_ext::i18n::tr(
                "ungültige Nachricht verworfen",
                "invalid message discarded"
            )),
            ProcessEvent::Message(M::Ready)
        ]
    );
}

#[test]
fn stderr_lines_are_forwarded_and_clipped() {
    let l = launch(&fake_config("stderr"), "");
    assert!(wait_until(3.0, || l.log.stderr().len() >= 2));
    assert_eq!(l.log.stderr(), vec!["eins".to_string(), "b".repeat(500)]);
}

/// stderr ist gedrosselt: höchstens 50 Zeilen pro Sekunde, der Rest als Sammelzeile, kein Kill.
#[test]
fn stderr_flood_is_throttled_not_killed() {
    let l = launch_ready(
        r#"kadrell.on("app.ready", function() for i = 1, 100000 do io.write(i, "\n") end end)"#,
    );
    std::thread::sleep(Duration::from_secs(2));
    let stderr = l.log.stderr();
    assert!(stderr.len() < 200, "{}", stderr.len());
    assert!(
        stderr.iter().any(|s| is_skipped_summary(s)),
        "{:?}",
        &stderr[stderr.len().saturating_sub(3)..]
    );
    assert!(l.log.exits().is_empty());
}

#[test]
fn missing_ready_is_killed() {
    let l = launch(&fake_config("noready"), "");
    assert!(wait_until(6.0, || !l.log.exits().is_empty()));
    let (expected, reason) = l.log.exits()[0].clone();
    assert!(!expected);
    assert_eq!(reason, Kill::NoReady.reason());
}

#[test]
fn overlong_line_is_killed() {
    let l = launch(&fake_config("longline"), "");
    assert!(wait_until(5.0, || !l.log.exits().is_empty()));
    let (expected, reason) = l.log.exits()[0].clone();
    assert!(!expected);
    assert_eq!(reason, Kill::LongLine.reason());
}

/// Ein Helper, der stdin nicht liest: über 1 MB ungeschriebene Nachrichten gelten als Hänger, die App blockiert nicht.
#[test]
fn full_input_buffer_is_killed() {
    let l = launch(&fake_config("idle"), "");
    assert!(wait_until(3.0, || l.log.messages().contains(&M::Ready)));
    let big = Value::String("x".repeat(100_000));
    for _ in 0..12 {
        l.p.send(HostMessage::Event {
            name: "big".into(),
            data: big.clone(),
        });
    }
    assert!(wait_until(5.0, || !l.log.exits().is_empty()));
    let (expected, reason) = l.log.exits()[0].clone();
    assert!(!expected);
    assert_eq!(reason, Kill::InputFull.reason());
}

#[test]
fn stop_is_graceful_and_then_hard() {
    let l = launch_ready(r#"kadrell.on("app.ready", function() while true do end end)"#);
    std::thread::sleep(Duration::from_millis(200));
    let begin = Instant::now();
    l.p.stop();
    assert!(wait_until(4.0, || !l.log.exits().is_empty()));
    assert!(begin.elapsed() < Duration::from_secs(3));
    assert!(l.log.exits()[0].0, "stop() ist ein erwartetes Ende");
    // Der harte Schritt nach 2 s darf kein zweites Ende auslösen.
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(l.log.exits().len(), 1);
}

#[test]
fn stop_of_idle_helper_uses_shutdown() {
    let l = launch_ready("");
    let begin = Instant::now();
    l.p.stop();
    assert!(wait_until(3.0, || !l.log.exits().is_empty()));
    assert!(
        begin.elapsed() < Duration::from_millis(900),
        "vor dem Eskalieren nach 1 s: {:?}",
        begin.elapsed()
    );
    assert!(l.log.exits()[0].0);
    assert_eq!(
        l.log.exits()[0].1,
        kadrell_ext::i18n::tr("Gestoppt", "Stopped")
    );
}

/// Ein Helper, der `shutdown` ignoriert, wird unter Windows nach 2 s hart beendet, unter Unix schon nach 1 s per SIGTERM
/// (der Fake ignoriert nur `shutdown`, nicht das Signal).
#[test]
fn stop_escalates_to_kill() {
    let l = launch(&fake_config("idle"), "");
    assert!(wait_until(3.0, || l.log.messages().contains(&M::Ready)));
    let begin = Instant::now();
    l.p.stop();
    assert!(wait_until(5.0, || !l.log.exits().is_empty()));
    let earliest = if cfg!(unix) { 900 } else { 1500 };
    assert!(
        begin.elapsed() > Duration::from_millis(earliest),
        "{:?}",
        begin.elapsed()
    );
    assert!(l.log.exits()[0].0);
}

#[test]
fn kill_ends_immediately_as_expected() {
    let l = launch_ready(r#"kadrell.on("app.ready", function() while true do end end)"#);
    std::thread::sleep(Duration::from_millis(200));
    let begin = Instant::now();
    l.p.kill();
    assert!(l.p.is_stopping());
    assert!(wait_until(3.0, || !l.log.exits().is_empty()));
    assert!(begin.elapsed() < Duration::from_secs(1));
    assert!(l.log.exits()[0].0);
}

/// Ohne `stop()` fallen gelassen: stdin schließt, der Helper beendet sich selbst.
#[test]
fn dropped_handle_closes_stdin_and_helper_exits() {
    let root = TempDir::new("proc");
    let dir = make_extension(root.path(), "p", "", None);
    let log = EventLog::new();
    let p =
        ExtensionProcess::spawn(&host_config(), &dir, hello(&dir), log.handler()).expect("spawn");
    assert!(wait_until(3.0, || log.messages().contains(&M::Ready)));
    drop(p);
    assert!(
        wait_until(4.0, || !log.exits().is_empty()),
        "{:?}",
        log.events()
    );
}

/// Kadrell endet, die Extension hängt in einer Schleife: ohne stdin beendet sich der Helper selbst.
#[test]
fn parent_gone_kills_looping_helper() {
    let (mut p, _root) =
        HostPipe::start_ready(r#"kadrell.on("app.ready", function() while true do end end)"#);
    std::thread::sleep(Duration::from_millis(200));
    p.close_stdin();
    assert!(p.wait_for_exit(2.0).is_some());
}

#[test]
fn exit_reason_names_status_and_last_error() {
    let l = launch_lua(r#"error("boom")"#);
    assert!(wait_until(5.0, || !l.log.exits().is_empty()));
    let (expected, reason) = l.log.exits()[0].clone();
    assert!(!expected);
    assert!(
        reason.starts_with(&kadrell_ext::i18n::tr(
            "Beendet mit Status 1",
            "Exited with status 1"
        )),
        "{reason}"
    );
    assert!(
        reason.contains("boom"),
        "der letzte Fehler steht im Grund: {reason}"
    );
}

/// Unter Unix liegen `kadrell.exec`-Kinder in eigenen Prozessgruppen; der Akteur beendet sie darum vor jedem Signal über
/// die Prozessliste. Die Funktion selbst ist plattformunabhängig und hier prüfbar.
#[test]
fn kill_descendants_reaches_exec_children() {
    let (p, _root) = HostPipe::start_ready(&sleeping_child_lua());
    assert!(wait_until(3.0, || !children_of(p.pid()).is_empty()));
    let child = children_of(p.pid())[0];
    assert!(kadrell_ext::sys::descendants(p.pid()).contains(&child));
    kadrell_ext::sys::kill_descendants(p.pid());
    assert!(
        wait_until(3.0, || !alive(child)),
        "sleep 77 ({child}) lebt nach kill_descendants weiter"
    );
    assert!(!kadrell_ext::sys::descendants(p.pid()).contains(&child));
}
