//! Rundlauf über den echten Kanal: Server auf einem Temp-Socket (Unix) bzw. einer Temp-Endpunktdatei (Windows) mit
//! einem Handler, Client in diesem Prozess. Die Gegenstücke zu `testClientBinaryAgainstServer`,
//! `testClientWaitsWhileStarting` und `testHandlerRunsSequentiallyAcrossCalls` der Mac-App.

use kadrell_control::client::{ClientOptions, Launcher, run, send};
use kadrell_control::hook::{HookEnv, run_hook};
use kadrell_control::testing::{MockHandler, TempDir};
use kadrell_control::{ControlRequest, ControlResponse, ControlServer, Controller, STARTING_STATUS, Service, caller_tile};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Starter, der nur zählt und nie wirklich startet.
struct FakeLauncher {
    running: bool,
    quit: bool,
    launches: Mutex<u32>,
}

impl FakeLauncher {
    fn new() -> FakeLauncher {
        FakeLauncher { running: false, quit: false, launches: Mutex::new(0) }
    }
}

impl Launcher for FakeLauncher {
    fn already_running(&self) -> bool {
        self.running
    }
    fn quit_recently(&self) -> bool {
        self.quit
    }
    fn launch(&self) -> Result<(), String> {
        *self.launches.lock().unwrap() += 1;
        Ok(())
    }
}

fn endpoint(dir: &TempDir) -> PathBuf {
    kadrell_control::endpoint::endpoint_path(dir.path())
}

fn options<'a>(socket: PathBuf, launcher: &'a dyn Launcher, caller: Option<&str>) -> ClientOptions<'a> {
    ClientOptions {
        socket,
        caller: caller.map(str::to_string),
        cwd: "/tmp".into(),
        launcher,
        wait: Duration::from_secs(5),
        poll: Duration::from_millis(20),
    }
}

/// Führt `kadrell <argv>` aus, gibt Exit-Code, stdout und stderr zurück.
fn cli(opts: &ClientOptions, argv: &[&str]) -> (i32, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    let code = run(&argv, opts, &mut out, &mut err);
    (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
}

type Seen = Arc<Mutex<Vec<(Option<String>, Option<String>, Option<u32>)>>>;

fn service(f: impl Fn(ControlRequest, Option<u32>) -> ControlResponse + Send + Sync + 'static) -> Service {
    Arc::new(f)
}

#[test]
fn round_trip_with_fake_handler() {
    let dir = TempDir::new();
    let mut h = MockHandler::new();
    h.add_session("k1", &dir.path().to_string_lossy().replace('\\', "/"), "Eins");
    let server = ControlServer::start(&endpoint(&dir), Controller::new(Arc::new(Mutex::new(h))).into_service()).unwrap();
    let launcher = FakeLauncher::new();
    let opts = options(server.path().to_path_buf(), &launcher, None);

    let (code, out, _) = cli(&opts, &["ls", "--json"]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["groups"][0]["sessions"][0]["title"], "Eins");

    let (code, out, err) = cli(&opts, &["kill", "-t", "zzz"]);
    assert_eq!((code, out.as_str(), err.as_str()), (1, "", "kadrell: keine Session „zzz“\n"));

    // Hilfe und Tippfehler kommen ohne Verbindung (und ohne die App zu starten).
    let dead = options(dir.path().join("nirgends"), &launcher, None);
    let (code, out, _) = cli(&dead, &["help"]);
    assert_eq!(code, 0);
    assert!(out.contains("kadrell new-group"));
    let (code, _, err) = cli(&dead, &["frobnicate"]);
    assert_eq!((code, err.as_str()), (1, "kadrell: unbekannter Befehl „frobnicate“, siehe kadrell help\n"));
    assert_eq!(*launcher.launches.lock().unwrap(), 0);
}

/// Der Client schickt ein erfundenes `caller`; der Server ersetzt es durch die Kachel aus dem Prozessbaum.
#[test]
fn caller_comes_from_the_peer_pid_not_the_claim() {
    let dir = TempDir::new();
    let seen: Seen = Arc::default();
    let log = seen.clone();
    let me = std::process::id();
    let svc = service(move |req, peer| {
        let own = peer.and_then(|p| caller_tile(p, &[("test".to_string(), me)]));
        let stranger = peer.and_then(|p| caller_tile(p, &[("fremd".to_string(), 99_999_999)]));
        log.lock().unwrap().push((own, stranger, peer));
        assert_eq!(req.caller.as_deref(), Some("fake"), "das Feld kommt unverändert an, der Server ignoriert es");
        ControlResponse::ok("hallo")
    });
    let server = ControlServer::start(&endpoint(&dir), svc).unwrap();
    let launcher = FakeLauncher::new();
    let (code, out, _) = cli(&options(server.path().to_path_buf(), &launcher, Some("fake")), &["ls"]);
    assert_eq!((code, out.as_str()), (0, "hallo\n"));
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0], (Some("test".to_string()), None, Some(me)));
}

/// Über die ganze Kette: ein Handler, in dem die Kachel `s1` meinen Prozess hält, sieht mich als `s1`.
#[test]
fn controller_attributes_the_client_to_its_tile() {
    let dir = TempDir::new();
    let mut h = MockHandler::new();
    let cwd = dir.path().to_string_lossy().replace('\\', "/");
    h.add_running("s1", &cwd, "eins", std::process::id());
    h.add_running("s2", &cwd, "zwei", 99_999_999);
    h.focused = Some("s2".into());
    let handler = Arc::new(Mutex::new(h));
    let server = ControlServer::start(&endpoint(&dir), Controller::new(handler.clone()).into_service()).unwrap();
    let launcher = FakeLauncher::new();
    // Behauptet s2 zu sein, ist aber der Prozess von s1: ohne -t gilt s1.
    let opts = options(server.path().to_path_buf(), &launcher, Some("s2"));
    assert_eq!(cli(&opts, &["rename", "von-s1"]).0, 0);
    assert!(handler.lock().unwrap().calls.contains(&"rename s1 von-s1".to_string()));
}

#[test]
fn client_waits_while_the_app_is_starting() {
    let dir = TempDir::new();
    let calls = Arc::new(Mutex::new(0));
    let counter = calls.clone();
    let svc = service(move |_, _| {
        let mut n = counter.lock().unwrap();
        *n += 1;
        if *n < 3 { ControlResponse { status: STARTING_STATUS, stdout: String::new(), stderr: "Kadrell startet noch\n".into() } } else { ControlResponse::ok("hallo") }
    });
    let server = ControlServer::start(&endpoint(&dir), svc).unwrap();
    let launcher = FakeLauncher::new();
    let (code, out, _) = cli(&options(server.path().to_path_buf(), &launcher, None), &["ls"]);
    assert_eq!((code, out.as_str()), (0, "hallo\n"));
    assert_eq!(*calls.lock().unwrap(), 3);
    assert_eq!(*launcher.launches.lock().unwrap(), 0, "es lauscht schon jemand");
}

#[test]
fn client_starts_the_app_and_waits_for_the_channel() {
    let dir = TempDir::new();
    let path = endpoint(&dir);
    let launcher = FakeLauncher::new();
    // Der Server erscheint erst, nachdem der Client angefangen hat zu warten.
    let late = std::thread::spawn({
        let path = path.clone();
        move || {
            std::thread::sleep(Duration::from_millis(300));
            ControlServer::start(&path, service(|_, _| ControlResponse::ok("da"))).unwrap()
        }
    });
    let (code, out, err) = cli(&options(path, &launcher, None), &["ls"]);
    let _server = late.join().unwrap();
    assert_eq!((code, out.as_str()), (0, "da\n"));
    assert_eq!(err, "kadrell: starte Kadrell …\n");
    assert_eq!(*launcher.launches.lock().unwrap(), 1);
}

#[test]
fn client_errors_without_a_server() {
    let dir = TempDir::new();
    let path = endpoint(&dir);
    let mut launcher = FakeLauncher::new();
    let mut opts = options(path.clone(), &launcher, None);
    opts.wait = Duration::from_millis(200);
    let (code, _, err) = cli(&opts, &["ls"]);
    assert_eq!(code, 1);
    assert_eq!(err, format!("kadrell: starte Kadrell …\nkadrell: Kadrell antwortet nicht ({})\n", path.display()));

    // Gerade beendet: nicht neu starten.
    launcher.quit = true;
    let mut opts = options(path.clone(), &launcher, None);
    opts.wait = Duration::from_millis(200);
    let (code, _, err) = cli(&opts, &["ls"]);
    assert_eq!((code, err.as_str()), (1, "kadrell: Kadrell wurde gerade beendet und startet nicht von selbst neu. Von Hand öffnen, dann geht es wieder.\n"));

    // Läuft schon, antwortet aber nicht: nichts starten, andere Meldung.
    launcher.quit = false;
    launcher.running = true;
    let mut opts = options(path.clone(), &launcher, None);
    opts.wait = Duration::from_millis(200);
    let (code, _, err) = cli(&opts, &["ls"]);
    assert_eq!(code, 1);
    assert_eq!(err, format!("kadrell: Kadrell läuft, lauscht aber nicht auf {}. Alte Version? Kadrell neu starten.\n", path.display()));
    assert_eq!(*launcher.launches.lock().unwrap(), 1, "nur der erste Aufruf hat gestartet");
}

/// Der Server beantwortet auch zwei Verbindungen zugleich, ein langsamer Handler blockiert den Rest nicht beim Lesen.
#[test]
fn stop_removes_the_endpoint_and_frees_the_channel() {
    let dir = TempDir::new();
    let path = endpoint(&dir);
    let mut server = ControlServer::start(&path, service(|_, _| ControlResponse::ok("x"))).unwrap();
    assert!(path.exists());
    let req = ControlRequest::new(vec!["ls".into()], "/", None);
    assert!(send(&path, &req, Duration::from_secs(5)).unwrap().is_some());
    server.stop();
    assert!(!path.exists());
    assert_eq!(send(&path, &req, Duration::from_secs(5)).unwrap(), None);
    // Nach dem Beenden lässt sich am selben Pfad neu starten.
    let _again = ControlServer::start(&path, service(|_, _| ControlResponse::ok("y"))).unwrap();
    assert_eq!(send(&path, &req, Duration::from_secs(5)).unwrap().unwrap().stdout, "y\n");
}

#[test]
fn garbage_request_gets_an_error_line() {
    let dir = TempDir::new();
    let server = ControlServer::start(&endpoint(&dir), service(|_, _| ControlResponse::ok("x"))).unwrap();
    let reply = raw_exchange(server.path(), b"das ist kein json\n");
    let resp: ControlResponse = serde_json::from_slice(&reply.expect("Antwort")).unwrap();
    assert_eq!((resp.status, resp.stderr.as_str()), (1, "kadrell: ungültige Anfrage\n"));
}

/// Eine rohe Zeile hin, die Antwort zurück (`None`: Verbindung ohne Antwort geschlossen).
#[cfg(unix)]
fn raw_exchange(path: &std::path::Path, line: &[u8]) -> Option<Vec<u8>> {
    use std::io::{Read, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(path).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream.write_all(line).ok()?;
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).ok()?;
    (!reply.is_empty()).then_some(reply)
}

#[cfg(windows)]
fn raw_exchange(path: &std::path::Path, line: &[u8]) -> Option<Vec<u8>> {
    use std::io::{Read, Write};
    let endpoint = kadrell_control::endpoint::Endpoint::read(path)?;
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", endpoint.port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream.write_all(line).ok()?;
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).ok()?;
    (!reply.is_empty()).then_some(reply)
}

#[cfg(unix)]
#[test]
fn socket_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let server = ControlServer::start(&endpoint(&dir), service(|_, _| ControlResponse::ok("x"))).unwrap();
    assert_eq!(std::fs::metadata(server.path()).unwrap().permissions().mode() & 0o777, 0o600);
}

/// Windows: ohne das Token aus der Endpunktdatei antwortet der Server nicht.
#[cfg(windows)]
#[test]
fn wrong_token_gets_no_answer() {
    let dir = TempDir::new();
    let server = ControlServer::start(&endpoint(&dir), service(|_, _| panic!("darf nicht aufgerufen werden"))).unwrap();
    let bad = serde_json::to_vec(&ControlRequest { token: "falsch".into(), ..ControlRequest::new(vec!["ls".into()], "/", None) }).unwrap();
    let mut line = bad;
    line.push(b'\n');
    assert_eq!(raw_exchange(server.path(), &line), None);
    let none = serde_json::to_vec(&ControlRequest::new(vec!["ls".into()], "/", None)).unwrap();
    let mut line = none;
    line.push(b'\n');
    assert_eq!(raw_exchange(server.path(), &line), None, "fehlendes Token zählt wie ein falsches");
}

// MARK: hook claude

/// stdin, das nie gelesen werden darf.
struct NoRead;

impl std::io::Read for NoRead {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        panic!("stdin gelesen");
    }
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[test]
fn hook_sends_the_mapped_status_and_always_exits_zero() {
    let dir = TempDir::new();
    let got: Arc<Mutex<Vec<ControlRequest>>> = Arc::default();
    let log = got.clone();
    let server = ControlServer::start(&endpoint(&dir), service(move |req, _| {
        log.lock().unwrap().push(req);
        ControlResponse::ok("")
    }))
    .unwrap();
    let map = |json: &[u8]| (json == b"{\"event\":1}").then(|| argv(&["status", "idle", "--session-id", "s1"]));
    let env = HookEnv { socket: Some(server.path().to_path_buf()), caller: Some("k1".into()), cwd: "/p".into() };
    let mut err = Vec::new();
    assert_eq!(run_hook(&argv(&["hook", "claude"]), &env, &mut &b"{\"event\":1}"[..], &map, &mut err), 0);
    {
        let got = got.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].argv.clone(), got[0].cwd.as_str(), got[0].caller.as_deref()), (argv(&["status", "idle", "--session-id", "s1"]), "/p", Some("k1")));
    }
    // Uninteressantes Ereignis: nichts gesendet, Exit 0.
    assert_eq!(run_hook(&argv(&["hook", "claude"]), &env, &mut &b"anderes"[..], &map, &mut err), 0);
    assert_eq!(got.lock().unwrap().len(), 1);
    assert!(err.is_empty());
}

#[test]
fn hook_is_silent_outside_kadrell_and_without_a_listener() {
    let map = |_: &[u8]| Some(argv(&["status", "idle"]));
    let mut err = Vec::new();
    // Kein Socket: stdin bleibt ungelesen, sofort Exit 0.
    assert_eq!(run_hook(&argv(&["hook", "claude"]), &HookEnv::default(), &mut NoRead, &map, &mut err), 0);
    // Socket, aber niemand lauscht: still Exit 0, nie die App starten.
    let dir = TempDir::new();
    let env = HookEnv { socket: Some(endpoint(&dir)), ..HookEnv::default() };
    assert_eq!(run_hook(&argv(&["hook", "claude"]), &env, &mut &b"{}"[..], &map, &mut err), 0);
    assert!(err.is_empty());
    // Falsche Argumente: Meldung und 1.
    assert_eq!(run_hook(&argv(&["hook", "gemini"]), &env, &mut NoRead, &map, &mut err), 1);
    assert_eq!(String::from_utf8(err).unwrap(), "kadrell: hook claude (liest das Hook-JSON von stdin)\n");
}
