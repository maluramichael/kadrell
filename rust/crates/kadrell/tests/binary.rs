//! Das echte Binary als Kommandozeile gegen einen Server auf einem eigenen Kanal, nicht dem der laufenden App
//! (Swift `ControlTests.testClientBinaryAgainstServer`). `KADRELL_SOCKET` zeigt auf den Temp-Kanal, ein Start der App
//! findet nie statt: es lauscht ja jemand.

use kadrell_control::testing::TempDir;
use kadrell_control::{ControlRequest, ControlResponse, ControlServer, Service, caller_tile};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// Was der Server beim Aufruf gesehen hat: Anfrage, Kachel für den Client, Kachel für einen Fremden.
type Seen = Arc<Mutex<Vec<(ControlRequest, Option<String>, Option<String>)>>>;

fn run_cli(socket: &Path, cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_kadrell"))
        .args(args)
        .current_dir(cwd)
        .env("KADRELL_SOCKET", socket)
        .env("KADRELL_SESSION_KEY", "k1")
        .env_remove("KADRELL_PROFILE")
        .output()
        .expect("kadrell starten");
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned(), String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn client_binary_against_server() {
    let dir = TempDir::new();
    let socket = kadrell_control::endpoint::endpoint_path(dir.path());
    let seen: Seen = Arc::default();
    let log = seen.clone();
    let me = std::process::id();
    let service: Service = Arc::new(move |req, peer| {
        // Der Client ist ein Kind dieses Testprozesses und wartet noch auf die Antwort.
        let own = peer.and_then(|p| caller_tile(p, &[(String::from("test"), me)]));
        let stranger = peer.and_then(|p| caller_tile(p, &[(String::from("fremd"), 99_999_999)]));
        let reply = if req.argv == ["ls"] { ControlResponse::ok("hallo") } else { ControlResponse::fail("nein") };
        log.lock().unwrap().push((req, own, stranger));
        reply
    });
    let server = ControlServer::start(&socket, service).unwrap();
    let cwd = dir.path().to_path_buf();

    let (code, out, _) = run_cli(server.path(), &cwd, &["ls"]);
    assert_eq!((code, out.as_str()), (0, "hallo\n"));
    {
        let seen = seen.lock().unwrap();
        let (req, own, stranger) = &seen[0];
        assert_eq!(req.caller.as_deref(), Some("k1"));
        assert_eq!(own.as_deref(), Some("test"));
        assert_eq!(*stranger, None);
        // Das Kind sieht den aufgelösten Pfad (macOS: /var ist ein Link nach /private/var).
        let real = cwd.canonicalize().unwrap();
        assert_eq!(req.cwd.replace('\\', "/"), real.to_string_lossy().replace('\\', "/").trim_start_matches("//?/"));
    }

    let (code, out, err) = run_cli(server.path(), &cwd, &["kill"]);
    assert_eq!((code, out.as_str(), err.as_str()), (1, "", "kadrell: nein\n"));

    let (code, out, _) = run_cli(server.path(), &cwd, &["help"]);
    assert_eq!(code, 0);
    assert!(out.contains("kadrell new-group"));
}
