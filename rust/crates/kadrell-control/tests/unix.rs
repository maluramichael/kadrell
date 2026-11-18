//! Unix-Pfade des Steuerkanals mit echten Prozessen: Peer-pid (`LOCAL_PEERPID` auf macOS, `SO_PEERCRED` auf Linux),
//! Aufrufer aus Prozessbaum und Session-Leader, Start per `setsid`, Socket-Rechte und Pfadgrenzen.
//! Auf Windows leer (dort gilt `roundtrip.rs` mit TCP und Token).
#![cfg(unix)]

use kadrell_control::client::{Launcher, SelfLauncher};
use kadrell_control::testing::TempDir;
use kadrell_control::{ControlRequest, ControlResponse, ControlServer, ProcessTree, Service, SystemProcessTree, caller_tile, caller_tile_in};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Ergebnis, das der Service-Thread für den Test hinterlässt.
type Shared<T> = Arc<Mutex<Option<T>>>;

/// Echter Prozessbaum, aber ohne die Terminal-Session: zeigt, dass nur der Session-Weg die Kachel findet.
struct NoSession;

impl ProcessTree for NoSession {
    fn parent(&self, pid: u32) -> Option<u32> {
        SystemProcessTree.parent(pid)
    }
    fn session_leader(&self, _pid: u32) -> Option<u32> {
        None
    }
}

fn socket(dir: &TempDir) -> PathBuf {
    kadrell_control::endpoint::endpoint_path(dir.path())
}

/// Wartet, bis `cond` wahr ist (höchstens 10 s).
fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !cond() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    cond()
}

/// Server, der die Peer-pid jeder Anfrage festhält und mit `ok` antwortet.
fn recording_server(dir: &TempDir) -> (ControlServer, Arc<Mutex<Vec<Option<u32>>>>) {
    let seen: Arc<Mutex<Vec<Option<u32>>>> = Arc::default();
    let log = seen.clone();
    let service: Service = Arc::new(move |_req, peer| {
        log.lock().unwrap().push(peer);
        ControlResponse::ok("ok")
    });
    (ControlServer::start(&socket(dir), service).unwrap(), seen)
}

fn request_line() -> Vec<u8> {
    let mut line = serde_json::to_vec(&ControlRequest::new(vec!["ls".into()], "/", None)).unwrap();
    line.push(b'\n');
    line
}

/// Schickt die Anfrage aus einem echten Fremdprozess (`nc -U`), liefert dessen pid.
fn request_from_child(path: &Path, req_file: &Path) -> u32 {
    std::fs::write(req_file, request_line()).unwrap();
    let mut child = Command::new("/usr/bin/nc")
        .arg("-U")
        .arg(path)
        .stdin(std::fs::File::open(req_file).unwrap())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("/usr/bin/nc fehlt");
    let pid = child.id();
    let _ = child.wait();
    pid
}

/// `LOCAL_PEERPID`: Der Server sieht die pid des Prozesses am anderen Ende, nicht seine eigene.
#[test]
fn peer_pid_is_the_connecting_process() {
    let dir = TempDir::new();
    let (_server, seen) = recording_server(&dir);
    let child_pid = request_from_child(&socket(&dir), &dir.path().join("req.json"));
    assert!(wait_until(|| !seen.lock().unwrap().is_empty()), "keine Anfrage angekommen");
    let peer = seen.lock().unwrap()[0].expect("Peer-pid");
    assert_eq!(peer, child_pid);
    assert_ne!(peer, std::process::id());
}

/// Auch im eigenen Prozess (Client und Server in einem Prozess) ist die Peer-pid die eigene.
#[test]
fn peer_pid_of_in_process_client_is_own_pid() {
    let dir = TempDir::new();
    let (_server, seen) = recording_server(&dir);
    let req = ControlRequest::new(vec!["ls".into()], "/", None);
    let resp = kadrell_control::client::send(&socket(&dir), &req, Duration::from_secs(5)).unwrap().expect("Antwort");
    assert_eq!(resp.stdout, "ok\n");
    assert_eq!(seen.lock().unwrap()[0], Some(std::process::id()));
}

/// Lebendes Kind unter einer "Kachel": Der Prozessbaum (`parent` per `proc_pidinfo`) führt zur Kachel-pid.
#[test]
fn live_child_is_attributed_to_its_tile_via_the_process_tree() {
    let me = std::process::id();
    let mut child = Command::new("sleep").arg("5").spawn().unwrap();
    let found = caller_tile(child.id(), &[("kachel".into(), me)]);
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(found.as_deref(), Some("kachel"));
}

/// Doppelt geforkt und von der Kachel gelöst (Elternprozess gestorben, hängt an launchd): Die Terminal-Session
/// (`getsid`) führt weiter zur Kachel. `sh` ist hier die Kachel (Session-Leader wie nach `forkpty`).
#[test]
fn orphaned_grandchild_is_attributed_via_the_session_leader() {
    let dir = TempDir::new();
    let tile_pid = Arc::new(AtomicU32::new(0));
    let tile = tile_pid.clone();
    // (mit Session, nur über den Elternbaum)
    let result: Shared<(Option<String>, Option<String>)> = Arc::default();
    let out = result.clone();
    let service: Service = Arc::new(move |_req, peer| {
        let tiles = vec![("kachel-1".to_string(), tile.load(Ordering::SeqCst))];
        let with_session = peer.and_then(|p| caller_tile(p, &tiles));
        let parents_only = peer.and_then(|p| caller_tile_in(&NoSession, p, &tiles));
        *out.lock().unwrap() = Some((with_session, parents_only));
        ControlResponse::ok("ok")
    });
    let _server = ControlServer::start(&socket(&dir), service).unwrap();
    let req_file = dir.path().join("req.json");
    std::fs::write(&req_file, request_line()).unwrap();
    // Die äußere Shell (Kachel) endet sofort. Die Unter-Shell wartet 1 s, hängt dann an launchd und startet nc: nc hat im
    // Elternbaum keinen Weg mehr zur Kachel, nur die Session führt dorthin.
    let script = format!(
        "( ( sleep 1; /usr/bin/nc -U '{}' < '{}' ) > /dev/null 2>&1 & ); exit 0",
        socket(&dir).display(),
        req_file.display()
    );
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(script).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // SAFETY: `setsid` zwischen fork und exec ist async-signal-safe; die Shell wird Session-Leader wie die Kachel nach `forkpty`.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut sh = cmd.spawn().unwrap();
    tile_pid.store(sh.id(), Ordering::SeqCst);
    assert!(wait_until(|| result.lock().unwrap().is_some()), "keine Anfrage angekommen");
    let _ = sh.wait();
    let (with_session, parents_only) = result.lock().unwrap().clone().unwrap();
    assert_eq!(with_session.as_deref(), Some("kachel-1"), "über die Terminal-Session");
    assert_eq!(parents_only, None, "über den Elternbaum allein ist die Kachel nicht zu erreichen");
}

/// Ein Prozess außerhalb jeder Kachel wird nie einer zugeordnet, auch wenn er einen Key im Feld `caller` schickt.
#[test]
fn outsider_is_not_attributed() {
    let dir = TempDir::new();
    let result: Shared<Option<String>> = Arc::default();
    let out = result.clone();
    let service: Service = Arc::new(move |req, peer| {
        assert_eq!(req.caller.as_deref(), Some("kachel-1"), "der Client behauptet eine Kachel");
        *out.lock().unwrap() = Some(peer.and_then(|p| caller_tile(p, &[("kachel-1".to_string(), 1_999_999)])));
        ControlResponse::ok("ok")
    });
    let _server = ControlServer::start(&socket(&dir), service).unwrap();
    let mut line = serde_json::to_vec(&ControlRequest::new(vec!["ls".into()], "/", Some("kachel-1".into()))).unwrap();
    line.push(b'\n');
    let req_file = dir.path().join("req.json");
    std::fs::write(&req_file, line).unwrap();
    let mut nc = Command::new("/usr/bin/nc").arg("-U").arg(socket(&dir)).stdin(std::fs::File::open(&req_file).unwrap()).stdout(Stdio::null()).spawn().unwrap();
    assert!(wait_until(|| result.lock().unwrap().is_some()));
    let _ = nc.wait();
    assert_eq!(result.lock().unwrap().clone().unwrap(), None);
}

/// Socket 0600, auch wenn die umask weit offen ist, und ein alter Socket am Pfad wird ersetzt.
#[test]
fn socket_is_0600_and_replaces_a_stale_file() {
    let dir = TempDir::new();
    let path = socket(&dir);
    std::fs::write(&path, b"alt").unwrap();
    let (_server, _seen) = recording_server(&dir);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "Modus {mode:o}");
}

/// Der Pfad eines Unix-Sockets ist auf macOS auf 104 Bytes begrenzt: zu lang gibt einen Fehler mit dem Pfad, keinen Absturz.
#[test]
fn too_long_socket_path_is_a_clean_error() {
    let dir = TempDir::new();
    let long = dir.path().join("x".repeat(120)).join("kadrell.sock");
    std::fs::create_dir_all(long.parent().unwrap()).unwrap();
    let service: Service = Arc::new(|_, _| ControlResponse::ok("x"));
    let err = ControlServer::start(&long, service).err().expect("Fehler erwartet");
    assert!(err.to_string().contains("bind"), "{err}");
}

/// `SelfLauncher` startet losgelöst: eigene Session und Prozessgruppe (`setsid`), nicht in der des Aufrufers.
#[test]
fn launcher_starts_the_app_in_its_own_session() {
    let dir = TempDir::new();
    let out = dir.path().join("ps.txt");
    let script = dir.path().join("fake-kadrell.sh");
    std::fs::write(&script, format!("#!/bin/sh\n/bin/ps -o pid=,pgid= -p $$ > '{}'\n", out.display())).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let launcher = SelfLauncher { exe: script, profile: None, profile_dir: dir.path().to_path_buf() };
    launcher.launch().expect("starten");
    assert!(wait_until(|| std::fs::read_to_string(&out).is_ok_and(|s| !s.trim().is_empty())), "Skript lief nicht");
    let text = std::fs::read_to_string(&out).unwrap();
    let nums: Vec<u32> = text.split_whitespace().filter_map(|n| n.parse().ok()).collect();
    assert_eq!(nums.len(), 2, "{text:?}");
    assert_eq!(nums[0], nums[1], "pid und Prozessgruppe gleich: Session-Leader nach setsid");
    // SAFETY: `getpgrp` hat keine Voraussetzungen.
    assert_ne!(nums[1], unsafe { libc::getpgrp() } as u32, "nicht in der Gruppe des Aufrufers");
}
