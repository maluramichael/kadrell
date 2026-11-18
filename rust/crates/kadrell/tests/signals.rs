//! Echter Prozess: SIGTERM und SIGINT beenden die App wie Quit. Sie startet mit `--profile tmp`, eigenem `HOME` und
//! eigenem `TMPDIR` (nie das Profil des Nutzers), bekommt eine Session mit einem Fake-`claude` und muss danach
//! sauber enden: Exit-Code 0, Wegwerfprofil samt Socket weg, Kindprozess beendet. Beendet wird nur der eigene Prozess.
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const APP_ID: &str = "de.malura.kadrell.rust";

/// Eigener Arbeitsordner je Test; beim Verwerfen werden die gestarteten Prozesse (nur diese pids) und der Ordner entfernt.
struct Sandbox {
    root: PathBuf,
    app: Option<Child>,
}

impl Sandbox {
    fn new(tag: &str) -> Sandbox {
        // Kurzer Pfad: ein Unix-Socket darf auf dem Mac höchstens 104 Bytes lang sein, `$TMPDIR` allein frisst die Hälfte.
        let root = PathBuf::from("/tmp").join(format!("kadrell-sig-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        let settings_dir = home.join("Library/Application Support").join(APP_ID);
        std::fs::create_dir_all(&settings_dir).unwrap();
        std::fs::create_dir_all(root.join("tmp")).unwrap();
        std::fs::create_dir_all(root.join("work")).unwrap();
        let fake = root.join("fake-claude");
        std::fs::write(&fake, format!("#!/bin/sh\necho $$ > '{}'\nexec sleep 600\n", root.join("claude.pid").display())).unwrap();
        run(&["/bin/chmod", "755", fake.to_str().unwrap()]);
        let settings = format!("{{\"claudeBinary\":{:?}}}", fake.to_str().unwrap());
        std::fs::write(settings_dir.join("settings.json"), settings).unwrap();
        Sandbox { root, app: None }
    }

    fn start_app(&mut self) -> u32 {
        let child = Command::new(env!("CARGO_BIN_EXE_kadrell"))
            .args(["--profile", "tmp"])
            .env("HOME", self.root.join("home"))
            .env("TMPDIR", self.root.join("tmp"))
            .env_remove("KADRELL_SOCKET")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("kadrell startet");
        let pid = child.id();
        self.app = Some(child);
        pid
    }

    fn profile_dir(&self, pid: u32) -> PathBuf {
        self.root.join("tmp").join(format!("kadrell-rust-tmp-{pid}"))
    }

    fn socket(&self, pid: u32) -> PathBuf {
        self.profile_dir(pid).join("kadrell.sock")
    }

    /// `kadrell <args>` gegen den Steuerkanal dieser Instanz.
    fn cli(&self, pid: u32, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_kadrell"))
            .args(args)
            .env("HOME", self.root.join("home"))
            .env("TMPDIR", self.root.join("tmp"))
            .env("KADRELL_SOCKET", self.socket(pid))
            .output()
            .expect("kadrell-Befehl startet")
    }

    fn fake_claude_pid(&self) -> Option<u32> {
        std::fs::read_to_string(self.root.join("claude.pid")).ok()?.trim().parse().ok()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if let Some(pid) = self.fake_claude_pid().filter(|p| alive(*p)) {
            run(&["/bin/kill", "-KILL", &pid.to_string()]);
        }
        if let Some(app) = self.app.as_mut() {
            let _ = app.kill();
            let _ = app.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run(argv: &[&str]) {
    let status = Command::new(argv[0]).args(&argv[1..]).status().expect("Befehl startet");
    assert!(status.success(), "{argv:?} schlug fehl");
}

/// `kill -0`: lebt der Prozess (auch als Zombie, den aber niemand hier zurücklässt)?
fn alive(pid: u32) -> bool {
    Command::new("/bin/kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn wait_until(what: &str, limit: Duration, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < limit, "Zeit abgelaufen: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn exists(p: &Path) -> bool {
    p.exists()
}

fn quits_cleanly_on(signal: &str) {
    let mut sb = Sandbox::new(signal);
    let pid = sb.start_app();
    wait_until("Steuerkanal der App", Duration::from_secs(30), || exists(&sb.socket(pid)));
    let work = sb.root.join("work");
    let out = sb.cli(pid, &["new", "-d", "-c", work.to_str().unwrap()]);
    assert!(out.status.success(), "new: {}", String::from_utf8_lossy(&out.stderr));
    wait_until("Fake-claude läuft", Duration::from_secs(20), || sb.fake_claude_pid().is_some_and(alive));
    let claude = sb.fake_claude_pid().unwrap();

    run(&["/bin/kill", signal, &pid.to_string()]);

    let app = sb.app.as_mut().unwrap();
    let mut status = None;
    wait_until("App beendet sich", Duration::from_secs(15), || {
        status = app.try_wait().unwrap();
        status.is_some()
    });
    assert_eq!(status.unwrap().code(), Some(0), "sauber beendet, nicht durch das Signal getötet");
    assert!(!exists(&sb.profile_dir(pid)), "Wegwerfprofil bleibt liegen");
    wait_until("Kindprozess endet", Duration::from_secs(10), || !alive(claude));
}

#[test]
fn sigterm_quits_like_quit() {
    quits_cleanly_on("-TERM");
}

#[test]
fn sigint_quits_like_quit() {
    quits_cleanly_on("-INT");
}
