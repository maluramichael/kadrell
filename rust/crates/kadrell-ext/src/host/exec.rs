//! `kadrell.exec`: ein Programm ohne Shell starten, stdout und stderr getrennt einsammeln, nach dem Timeout den ganzen
//! Prozessbaum beenden. Startfehler ergeben Status 127 wie in der Shell.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use crate::sys::{self, KillGroup};

/// So lange wartet der Helper nach dem Prozessende noch auf Ausgabe, die ein Nachkomme offen hält.
const LINGER: Duration = Duration::from_millis(300);
/// Status eines wegen Timeout beendeten Prozesses (wie SIGKILL).
const KILLED: i32 = 9;

fn fields(status: i32, stdout: &[u8], stderr: &[u8]) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("status".into(), json!(status));
    m.insert("stdout".into(), json!(String::from_utf8_lossy(stdout)));
    m.insert("stderr".into(), json!(String::from_utf8_lossy(stderr)));
    m
}

/// Führt `argv` aus. Ohne "/" oder "\" im Namen sucht das System über PATH.
pub fn run(argv: &[String], cwd: Option<&str>, timeout: f64) -> Map<String, Value> {
    let Some(first) = argv.first() else {
        return fields(127, b"", b"no such file");
    };
    let mut cmd = Command::new(first);
    cmd.args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    sys::configure(&mut cmd);
    match cmd.spawn() {
        Ok(child) => supervise(child, timeout),
        Err(e) => fields(127, b"", e.to_string().as_bytes()),
    }
}

fn reader<R: Read + Send + 'static>(mut pipe: R, index: usize, tx: mpsc::Sender<(usize, Vec<u8>)>) {
    std::thread::spawn(move || {
        let mut data = Vec::new();
        let _ = pipe.read_to_end(&mut data);
        let _ = tx.send((index, data));
    });
}

fn supervise(mut child: std::process::Child, timeout: f64) -> Map<String, Value> {
    let group = KillGroup::attach(&child, false);
    if let Some(g) = &group {
        sys::track_exec(g);
    }
    let (tx, rx) = mpsc::channel();
    if let Some(out) = child.stdout.take() {
        reader(out, 0, tx.clone());
    }
    if let Some(err) = child.stderr.take() {
        reader(err, 1, tx.clone());
    }
    drop(tx);
    let limit = Duration::try_from_secs_f64(timeout.max(0.0)).unwrap_or(Duration::from_secs(30));
    let deadline = Instant::now() + limit;
    let status = wait(&mut child, group.as_ref(), deadline);
    if let Some(g) = &group {
        sys::untrack_exec(g);
    }
    let mut data = [Vec::new(), Vec::new()];
    let linger_until = Instant::now() + LINGER;
    for _ in 0..2 {
        let wait = linger_until.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait.max(Duration::from_millis(1))) {
            Ok((i, d)) => data[i] = d,
            Err(_) => break,
        }
    }
    fields(status, &data[0], &data[1])
}

/// Wartet auf das Ende; nach dem Timeout wird der Baum beendet.
fn wait(child: &mut std::process::Child, group: Option<&KillGroup>, deadline: Instant) -> i32 {
    loop {
        match child.try_wait() {
            Ok(Some(st)) => return exit_status(st),
            Ok(None) => {}
            Err(_) => return KILLED,
        }
        if Instant::now() >= deadline {
            match group {
                Some(g) => g.kill(),
                None => {
                    let _ = child.kill();
                }
            }
            let _ = child.wait();
            return KILLED;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn exit_status(st: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = st.signal() {
            return sig;
        }
    }
    st.code().unwrap_or(KILLED)
}
