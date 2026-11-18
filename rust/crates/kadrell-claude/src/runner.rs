//! Echte Prozesse für `CommandRunner` (Swift: `ProcessRunner.execute`). Gelesen wird bis zum Prozessende, nicht bis
//! EOF: ein Hintergrundjob aus dem Shell-Profil, der die Pipe erbt, hält sie sonst ewig offen.

use crate::cli::{CommandRunner, RunOutput};
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

/// Startet echte Prozesse; nach `timeout` wird der Prozess beendet (mit Warnung auf stderr), die bis dahin gelesene
/// Ausgabe kommt zurück.
pub struct SystemRunner;

/// Wie lange nach dem Prozessende noch auf bereits geschriebene Ausgabe gewartet wird.
const GRACE: Duration = Duration::from_millis(250);
const TICK: Duration = Duration::from_millis(10);

/// Ein Stück Ausgabe: `true` = stderr.
type Chunk = (bool, Vec<u8>);

#[derive(Default)]
struct Collected {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Collected {
    fn push(&mut self, (is_err, bytes): Chunk) {
        if is_err { &mut self.stderr } else { &mut self.stdout }.extend_from_slice(&bytes);
    }

    fn into_output(self, status: i32) -> RunOutput {
        let text = |b: Vec<u8>| String::from_utf8_lossy(&b).into_owned();
        RunOutput { status, stdout: text(self.stdout), stderr: text(self.stderr) }
    }
}

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[String], env: Option<&BTreeMap<String, String>>, cwd: Option<&str>, timeout: Duration) -> Result<RunOutput, String> {
        let mut cmd = Command::new(program);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(env) = env {
            cmd.env_clear().envs(env);
        }
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let (tx, rx) = channel();
        spawn_reader(child.stdout.take(), false, tx.clone());
        spawn_reader(child.stderr.take(), true, tx);
        let mut got = Collected::default();
        let status = wait_collecting(&mut child, &rx, &mut got, Instant::now() + timeout)?;
        if status.is_none() {
            eprintln!("kadrell: {}", timeout_warning(program, args, timeout));
        }
        Ok(got.into_output(status.unwrap_or(-1)))
    }
}

/// `<exe> <args>: nach N s abgebrochen`.
pub fn timeout_warning(program: &str, args: &[String], timeout: Duration) -> String {
    let mut line = program.to_string();
    for a in args {
        line.push(' ');
        line.push_str(a);
    }
    format!("{line}: nach {} s abgebrochen", timeout.as_secs())
}

/// Liest in einem eigenen Thread und reicht jedes Stück weiter. Der Thread endet bei EOF, hält ein Hintergrundjob die
/// Pipe offen, bleibt er hängen, ohne den Aufrufer aufzuhalten.
fn spawn_reader<R: Read + Send + 'static>(pipe: Option<R>, is_err: bool, tx: Sender<Chunk>) {
    let Some(mut pipe) = pipe else { return };
    std::thread::spawn(move || {
        let mut buf = vec![0u8; 1 << 16];
        while let Ok(n) = pipe.read(&mut buf) {
            if n == 0 || tx.send((is_err, buf[..n].to_vec())).is_err() {
                break;
            }
        }
    });
}

/// Sammelt Ausgabe, bis der Prozess endet (`Some(Exit-Code)`) oder `deadline` erreicht ist (Prozess beendet, `None`).
fn wait_collecting(child: &mut Child, rx: &Receiver<Chunk>, got: &mut Collected, deadline: Instant) -> Result<Option<i32>, String> {
    loop {
        match rx.recv_timeout(TICK) {
            Ok(chunk) => got.push(chunk),
            Err(RecvTimeoutError::Timeout) => {}
            // Beide Pipes sind zu: nur noch aufs Prozessende warten.
            Err(RecvTimeoutError::Disconnected) => std::thread::sleep(TICK),
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            drain_ready(rx, got);
            return Ok(Some(status.code().unwrap_or(-1)));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            drain_ready(rx, got);
            return Ok(None);
        }
    }
}

/// Nach dem Prozessende: was schon in der Pipe liegt, noch einsammeln, höchstens `GRACE` lang.
fn drain_ready(rx: &Receiver<Chunk>, got: &mut Collected) {
    let until = Instant::now() + GRACE;
    while let Some(left) = until.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left.min(Duration::from_millis(30))) {
            Ok(chunk) => got.push(chunk),
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str], timeout: Duration) -> (RunOutput, Duration) {
        let (program, rest) = args.split_first().unwrap();
        let rest: Vec<String> = rest.iter().map(|s| s.to_string()).collect();
        let start = Instant::now();
        let out = SystemRunner.run(program, &rest, None, None, timeout).unwrap();
        (out, start.elapsed())
    }

    /// Ein Hintergrundjob, der die Pipe erbt, darf nicht bis zu seinem Ende festhalten.
    #[test]
    fn returns_at_process_exit_even_if_a_background_job_holds_the_pipe() {
        let script: &[&str] = if cfg!(windows) { &["cmd", "/C", "echo hallo & start /B ping -n 8 127.0.0.1"] } else { &["sh", "-c", "echo hallo; sleep 7 &"] };
        let (out, took) = run(script, Duration::from_secs(30));
        assert_eq!(out.status, 0);
        assert!(out.stdout.contains("hallo"), "{out:?}");
        assert!(took < Duration::from_secs(4), "wartete {took:?} auf den Hintergrundjob");
    }

    #[test]
    fn timeout_kills_the_process_and_keeps_the_output_so_far() {
        let script: &[&str] = if cfg!(windows) { &["cmd", "/C", "echo vorab & ping -n 20 127.0.0.1"] } else { &["sh", "-c", "echo vorab; sleep 20"] };
        let (out, took) = run(script, Duration::from_millis(1500));
        assert_eq!(out.status, -1);
        assert!(out.stdout.contains("vorab"), "{out:?}");
        assert!(took < Duration::from_secs(10), "{took:?}");
    }

    #[test]
    fn captures_stderr_separately() {
        let script: &[&str] = if cfg!(windows) { &["cmd", "/C", "echo aus & echo fehler 1>&2"] } else { &["sh", "-c", "echo aus; echo fehler >&2"] };
        let (out, _) = run(script, Duration::from_secs(10));
        assert!(out.stdout.contains("aus") && !out.stdout.contains("fehler"), "{out:?}");
        assert!(out.stderr.contains("fehler"), "{out:?}");
    }

    #[test]
    fn warning_names_program_arguments_and_seconds() {
        let w = timeout_warning("claude", &["agents".into(), "--json".into()], Duration::from_secs(10));
        assert_eq!(w, "claude agents --json: nach 10 s abgebrochen");
    }
}
