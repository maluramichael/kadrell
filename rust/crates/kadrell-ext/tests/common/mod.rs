//! Gemeinsame Hilfen der Integrationstests: der Helper als Kindprozess und Prozessbaum-Abfragen.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sysinfo::{Pid, ProcessesToUpdate, System};

pub use kadrell_ext::testutil::{TempDir, make_extension, wait_until};

pub const HOST_EXE: &str = env!("CARGO_BIN_EXE_kadrell-ext-host");
pub const FAKE_EXE: &str = env!("CARGO_BIN_EXE_kadrell-ext-fake");

/// Pfad der Fake-Binärdatei mit Schrägstrichen, bereit für einen Lua-String.
pub fn fake_lua() -> String {
    FAKE_EXE.replace('\\', "/")
}

/// Umgebung für Helfer: die eigene, mit dem Ordner der Fake-Binärdatei vorn im PATH.
pub fn helper_env() -> Vec<(String, String)> {
    let dir = Path::new(FAKE_EXE).parent().map(PathBuf::from).unwrap_or_default();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let joined = std::env::join_paths(std::iter::once(dir).chain(std::env::split_paths(&path))).unwrap_or_default();
    vec![("PATH".to_string(), joined.to_string_lossy().into_owned())]
}

/// Startet `kadrell-ext-host ext-host <dir>` und spricht mit ihm JSON-Zeilen über stdin/stdout.
pub struct HostPipe {
    pub child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<Value>,
    stderr: Arc<Mutex<String>>,
}

impl HostPipe {
    pub fn new(dir: &Path) -> HostPipe {
        let mut cmd = Command::new(HOST_EXE);
        cmd.arg("ext-host").arg(dir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        for (k, v) in helper_env() {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn().expect("host start");
        let stdout = child.stdout.take().unwrap();
        let stderr_pipe = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(v) = serde_json::from_str::<Value>(&line) {
                    let _ = tx.send(v);
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr_pipe).lines().map_while(Result::ok) {
                let mut s = sink.lock().unwrap();
                s.push_str(&line);
                s.push('\n');
            }
        });
        let stdin = child.stdin.take();
        HostPipe { child, stdin, rx, stderr }
    }

    /// Startet die Extension mit `init_lua` und schickt `hello`.
    pub fn start(init_lua: &str) -> (HostPipe, TempDir) {
        Self::start_in(init_lua, &std::env::temp_dir())
    }

    pub fn start_in(init_lua: &str, storage_dir: &Path) -> (HostPipe, TempDir) {
        let root = TempDir::new("host");
        let dir = make_extension(root.path(), "hello", init_lua, None);
        let p = HostPipe::new(&dir);
        p.send(json!({"t": "hello", "api": 1, "name": "hello", "dir": dir.to_string_lossy(),
            "storageDir": storage_dir.to_string_lossy(), "config": {}, "locale": "de", "sessions": []}));
        (p, root)
    }

    /// Wie `start`, wartet auf `ready` und schickt `app.ready`.
    pub fn start_ready(init_lua: &str) -> (HostPipe, TempDir) {
        let (p, root) = Self::start(init_lua);
        p.expect_ready();
        (p, root)
    }

    pub fn expect_ready(&self) {
        assert_eq!(self.next(3.0).and_then(|m| m["t"].as_str().map(String::from)).as_deref(), Some("ready"));
        self.send(json!({"t": "event", "name": "app.ready", "data": {}}));
    }

    pub fn send(&self, v: Value) {
        if let Some(mut stdin) = self.stdin.as_ref() {
            let mut line = v.to_string();
            line.push('\n');
            let _ = stdin.write_all(line.as_bytes());
            let _ = stdin.flush();
        }
    }

    pub fn next(&self, timeout: f64) -> Option<Value> {
        self.rx.recv_timeout(Duration::from_secs_f64(timeout)).ok()
    }

    /// Nächste Nachricht vom Typ `t`, andere (ready, log) werden übersprungen.
    pub fn next_of(&self, t: &str, timeout: f64) -> Option<Value> {
        let deadline = Instant::now() + Duration::from_secs_f64(timeout);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let m = self.rx.recv_timeout(left).ok()?;
            if m["t"] == t {
                return Some(m);
            }
        }
    }

    pub fn text(&self, timeout: f64) -> Option<String> {
        self.next(timeout).and_then(|m| m["text"].as_str().map(String::from))
    }

    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }

    pub fn wait_for_exit(&mut self, timeout: f64) -> Option<std::process::ExitStatus> {
        let deadline = Instant::now() + Duration::from_secs_f64(timeout);
        while Instant::now() < deadline {
            if let Ok(Some(st)) = self.child.try_wait() {
                return Some(st);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    pub fn stderr_text(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }

    pub fn assert_alive(&self, id: i64) {
        self.send(json!({"t": "ping", "id": id}));
        let pong = self.next_of("pong", 3.0).expect("pong");
        assert_eq!(pong["id"], id);
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}

impl Drop for HostPipe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Direkte Kinder eines Prozesses, unabhängig vom Code unter Test.
pub fn children_of(pid: u32) -> Vec<u32> {
    let mut sys = System::new().expect("sysinfo");
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .iter()
        .filter(|(_, p)| p.parent() == Some(Pid::from_u32(pid)))
        .map(|(id, _)| id.as_u32())
        .collect()
}

/// Lebt der Prozess noch?
pub fn alive(pid: u32) -> bool {
    let mut sys = System::new().expect("sysinfo");
    sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
    sys.process(Pid::from_u32(pid)).is_some()
}

/// Ein Mini-HTTP-Server auf 127.0.0.1, der genau eine Anfrage mit dem vorgegebenen Text beantwortet.
pub struct OneShotHttpServer {
    pub port: u16,
    received: Arc<Mutex<String>>,
}

impl OneShotHttpServer {
    pub fn new(response: &'static str) -> OneShotHttpServer {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&received);
        std::thread::spawn(move || {
            use std::io::Read;
            let Ok((mut conn, _)) = listener.accept() else { return };
            let mut buf = vec![0u8; 8192];
            // Kopfzeilen und, falls vorhanden, den kurzen Text lesen.
            let mut got = Vec::new();
            conn.set_read_timeout(Some(Duration::from_millis(500))).ok();
            while let Ok(n) = conn.read(&mut buf) {
                if n == 0 {
                    break;
                }
                got.extend_from_slice(&buf[..n]);
                if got.windows(4).any(|w| w == b"\r\n\r\n") && got.len() > 8 {
                    // Kurz auf einen eventuellen Text warten.
                    conn.set_read_timeout(Some(Duration::from_millis(100))).ok();
                }
            }
            *sink.lock().unwrap() = String::from_utf8_lossy(&got).into_owned();
            let _ = conn.write_all(response.as_bytes());
            let _ = conn.flush();
        });
        OneShotHttpServer { port, received }
    }

    pub fn request(&self) -> String {
        for _ in 0..100 {
            let r = self.received.lock().unwrap().clone();
            if !r.is_empty() {
                return r;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        String::new()
    }
}
