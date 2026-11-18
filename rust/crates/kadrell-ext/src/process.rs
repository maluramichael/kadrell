//! Ein Extension-Helper (`kadrell ext-host <ordner>`) als Kindprozess: JSON-Zeilen über stdin/stdout, stderr ins Log.
//! Überwacht ihn selbst (ready, ping, Flut, Zeilenlänge, voller Eingangspuffer), killt hart und meldet das über
//! `ProcessEvent::Exit`. Eine Instanz gehört zu genau einem Prozess.
//!
//! Aufbau: Lese-Threads für stdout und stderr und ein Schreib-Thread für stdin hängen an einem Akteur-Thread, der allein
//! den Zustand hält (Timer, Drosseln, Kill-Gründe) und die Ereignisse der Reihe nach an den Handler gibt. Unter Windows
//! hängt der Prozess in einem Job Object, unter Unix führt er eine eigene Prozessgruppe; beides beendet den Baum.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::i18n::{tr, tr_args};
use crate::json::clip;
use crate::message::{ExtensionMessage, HostMessage};
use crate::sys::{self, KillGroup};

pub const READY_TIMEOUT: Duration = Duration::from_secs(3);
pub const PING_INTERVAL: Duration = Duration::from_secs(5);
pub const MAX_MESSAGES_PER_SECOND: usize = 50;
pub const MAX_LINE: usize = 1 << 20;
pub const MAX_INPUT_BUFFER: usize = 1 << 20;
pub const MAX_TEXT: usize = 500;
pub const MAX_LINES_PER_SECOND: usize = 50;
/// Nach dem Prozessende wartet der Akteur so lange auf das Ende der Pipes, falls ein Nachkomme sie noch offen hält.
const PIPE_GRACE: Duration = Duration::from_millis(500);
const EXIT_POLL: Duration = Duration::from_millis(15);

/// Gründe für einen harten Kill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kill {
    NoReady,
    NoPong,
    Flood,
    LongLine,
    InputFull,
}

impl Kill {
    pub fn reason(self) -> String {
        match self {
            Kill::NoReady => tr(
                "Startet nicht (kein Ready binnen 3 s)",
                "Does not start (no ready within 3 s)",
            ),
            Kill::NoPong => tr(
                "Antwortet nicht mehr (kein Pong binnen 5 s)",
                "Not responding (no pong within 5 s)",
            ),
            Kill::Flood => tr(
                "Zu viele Nachrichten (mehr als 50 pro Sekunde)",
                "Too many messages (more than 50 per second)",
            ),
            Kill::LongLine => tr(
                "Nachricht zu groß (über 1 MB)",
                "Message too large (over 1 MB)",
            ),
            Kill::InputFull => tr(
                "Nimmt keine Nachrichten mehr an (über 1 MB unverarbeitet)",
                "No longer accepts messages (over 1 MB unprocessed)",
            ),
        }
    }
}

/// Was der Handler vom Prozess erfährt.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcessEvent {
    Message(ExtensionMessage),
    /// Eine Zeile von stderr (gekürzt, gedrosselt) oder eine Meldung des Überwachers.
    Stderr(String),
    /// Das Ende, genau einmal. `expected` nur für Exits nach `stop()`/`kill()`, alles andere (auch ein selbst gewähltes
    /// Ende mit Status 0) ist ein Absturz.
    Exit {
        expected: bool,
        reason: String,
    },
}

pub type EventHandler = Arc<dyn Fn(ProcessEvent) + Send + Sync>;

/// Wie der Helper gestartet wird.
#[derive(Debug, Clone)]
pub struct ProcessConfig {
    pub executable: PathBuf,
    /// Argumente vor `ext-host <ordner>` (leer für `kadrell`, Platzhalter für Test-Binaries).
    pub args_prefix: Vec<String>,
    /// Die Umgebung des Helpers; `None` erbt die eigene.
    pub environment: Option<Vec<(String, String)>>,
    /// Zusätzliche Variablen auf der geerbten Umgebung (z. B. für Test-Binaries).
    pub extra_environment: Vec<(String, String)>,
}

impl ProcessConfig {
    pub fn new(executable: impl Into<PathBuf>) -> ProcessConfig {
        ProcessConfig {
            executable: executable.into(),
            args_prefix: Vec::new(),
            environment: None,
            extra_environment: Vec::new(),
        }
    }
}

/// Der Prozess vom Typ `ExtensionProcess` ist ein Handle; er läuft in eigenen Threads weiter.
pub struct ExtensionProcess {
    tx: Sender<Cmd>,
    stopping: Arc<AtomicBool>,
    pid: u32,
    abandon: AtomicBool,
}

enum Cmd {
    Out(Vec<u8>),
    Err(Vec<u8>),
    OutEof,
    ErrEof,
    Send(HostMessage),
    Stop,
    Kill,
    Abandon,
}

impl ExtensionProcess {
    /// Startet `executable [args_prefix] ext-host <dir>`, schickt `hello` und meldet alles über `handler` (von einem
    /// eigenen Thread, der Reihe nach).
    pub fn spawn(
        config: &ProcessConfig,
        dir: &Path,
        hello: HostMessage,
        handler: EventHandler,
    ) -> std::io::Result<ExtensionProcess> {
        let mut cmd = Command::new(&config.executable);
        cmd.args(&config.args_prefix).arg("ext-host").arg(dir);
        if let Some(env) = &config.environment {
            cmd.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
        }
        cmd.envs(config.extra_environment.iter().map(|(k, v)| (k, v)));
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        sys::configure(&mut cmd);
        let mut child = cmd.spawn()?;
        let pid = child.id();
        let group = KillGroup::attach(&child, true);
        let (tx, rx) = mpsc::channel();
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| std::io::Error::other("kein stdin"))?;
        let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
        let unwritten = Arc::new(AtomicUsize::new(0));
        let writer = spawn_writer(stdin, Arc::clone(&unwritten));
        if let Some(out) = stdout {
            spawn_reader(out, tx.clone(), Cmd::Out, || Cmd::OutEof);
        }
        if let Some(err) = stderr {
            spawn_reader(err, tx.clone(), Cmd::Err, || Cmd::ErrEof);
        }
        let stopping = Arc::new(AtomicBool::new(false));
        let actor = Actor::new(
            child,
            group,
            writer,
            unwritten,
            handler,
            Arc::clone(&stopping),
        );
        thread::Builder::new().name("ext-process".into()).spawn({
            let rx_tx = tx.clone();
            move || actor.run(rx, rx_tx, hello)
        })?;
        Ok(ExtensionProcess {
            tx,
            stopping,
            pid,
            abandon: AtomicBool::new(true),
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Nach `stop()` oder `kill()`: was der Prozess jetzt noch meldet, zählt nicht mehr.
    pub fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    pub fn send(&self, message: HostMessage) {
        let _ = self.tx.send(Cmd::Send(message));
    }

    /// `shutdown`, nach 1 s SIGTERM (nur Unix), nach weiteren 1 s harter Kill. Kehrt sofort zurück, das Ende kommt über
    /// `ProcessEvent::Exit`.
    pub fn stop(&self) {
        if !self.stopping.swap(true, Ordering::SeqCst) {
            self.abandon.store(false, Ordering::SeqCst);
            let _ = self.tx.send(Cmd::Stop);
        }
    }

    /// Sofort hart beenden ohne `shutdown`, etwa bei einem zu großen Panel. Das Ende kommt wie nach `stop()`.
    pub fn kill(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        self.abandon.store(false, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Kill);
    }
}

/// Ohne `stop()` fallen gelassen: stdin schließen, der Helper beendet sich dann selbst, auch in einer Endlosschleife
/// (der Akteur killt ihn nach 2 s, falls nicht).
impl Drop for ExtensionProcess {
    fn drop(&mut self) {
        if self.abandon.load(Ordering::SeqCst) {
            let _ = self.tx.send(Cmd::Abandon);
        }
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut pipe: R,
    tx: Sender<Cmd>,
    data: fn(Vec<u8>) -> Cmd,
    eof: fn() -> Cmd,
) {
    thread::spawn(move || {
        let mut buf = vec![0u8; 1 << 16];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(data(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
            }
        }
        let _ = tx.send(eof());
    });
}

/// Schreibt auf stdin des Helpers. Blockiert er, blockiert nur dieser Thread; der Akteur zählt die ungeschriebenen
/// Bytes und killt bei mehr als 1 MB.
fn spawn_writer(
    mut stdin: std::process::ChildStdin,
    unwritten: Arc<AtomicUsize>,
) -> Sender<Option<Vec<u8>>> {
    let (tx, rx) = mpsc::channel::<Option<Vec<u8>>>();
    thread::spawn(move || {
        let mut broken = false;
        for item in rx {
            let Some(bytes) = item else { break };
            if !broken && (stdin.write_all(&bytes).is_err() || stdin.flush().is_err()) {
                broken = true;
            }
            unwritten.fetch_sub(bytes.len(), Ordering::SeqCst);
        }
        // stdin wird hier geschlossen.
    });
    tx
}

// MARK: Drossel und Zeilenzerleger

/// Was die Zeilenzerleger melden.
#[derive(Debug, PartialEq)]
enum Ev {
    Message(ExtensionMessage),
    Garbage,
    Violation(Kill),
    Stderr(Vec<String>),
    Skipped(usize),
}

/// Höchstens 50 Einträge pro Sekunde (festes Fenster), der Rest wird gezählt und einmal pro Sekunde als Sammelzeile
/// gemeldet. Für Log-Zeilen und stderr: kein Kill, aber eine Extension, die viel ausgibt, legt die App nicht lahm.
#[derive(Default)]
struct Throttle {
    window_start: Option<Instant>,
    sent: usize,
    skipped: usize,
}

impl Throttle {
    /// Wann die ausstehende Sammelzeile fällig ist, `None` ohne übersprungene Einträge.
    fn due(&self) -> Option<Instant> {
        (self.skipped > 0).then(|| {
            self.window_start
                .map(|s| s + Duration::from_secs(1))
                .unwrap_or_else(Instant::now)
        })
    }

    /// Darf der Eintrag durch? Eine fällige Sammelzeile des abgelaufenen Fensters landet vorher in `events`.
    fn admit(&mut self, now: Instant, events: &mut Vec<Ev>) -> bool {
        events.extend(self.tick(now));
        if self.sent >= MAX_LINES_PER_SECOND {
            self.skipped += 1;
            return false;
        }
        self.sent += 1;
        true
    }

    /// Nach Ablauf des Fensters: neues Fenster, ausstehende Sammelzeile raus.
    fn tick(&mut self, now: Instant) -> Option<Ev> {
        let expired = self
            .window_start
            .is_none_or(|s| now.saturating_duration_since(s) >= Duration::from_secs(1));
        if !expired {
            return None;
        }
        self.window_start = Some(now);
        self.sent = 0;
        self.flush()
    }

    /// Ausstehende Sammelzeile sofort, etwa bei EOF.
    fn flush(&mut self) -> Option<Ev> {
        let n = std::mem::take(&mut self.skipped);
        (n > 0).then_some(Ev::Skipped(n))
    }
}

/// Zerlegt stdout in Zeilen und dekodiert sie. Log-Nachrichten laufen durch die Drossel, `run` und `secret` warten in Lua
/// auf ihr Ergebnis und zählen nicht, alle anderen zählen gegen die Flutgrenze. Nach einem Verstoß wird alles verworfen,
/// der Akteur killt dann den Prozess.
#[derive(Default)]
struct ProtocolLines {
    logs: Throttle,
    pending: Vec<u8>,
    recent: Vec<Instant>,
    dead: bool,
}

impl ProtocolLines {
    fn feed(&mut self, chunk: &[u8], now: Instant) -> Vec<Ev> {
        let mut events = Vec::new();
        if self.dead {
            return events;
        }
        self.pending.extend_from_slice(chunk);
        let mut start = 0;
        while !self.dead {
            let Some(rel) = self.pending[start..].iter().position(|b| *b == b'\n') else {
                break;
            };
            let line = self.pending[start..start + rel].to_vec();
            start += rel + 1;
            self.take(&line, now, &mut events);
        }
        if self.dead {
            self.pending.clear();
        } else {
            self.pending.drain(..start);
            if self.pending.len() > MAX_LINE {
                events.push(self.violation(Kill::LongLine));
            }
        }
        events
    }

    fn take(&mut self, line: &[u8], now: Instant, events: &mut Vec<Ev>) {
        if line.len() > MAX_LINE {
            return events.push(self.violation(Kill::LongLine));
        }
        let message = ExtensionMessage::decode(line);
        match &message {
            Some(ExtensionMessage::Log { .. }) => {
                if self.logs.admit(now, events) {
                    events.extend(message.map(Ev::Message));
                }
                return;
            }
            Some(ExtensionMessage::Run { .. } | ExtensionMessage::Secret { .. }) => {
                return events.extend(message.map(Ev::Message));
            }
            _ => {}
        }
        self.recent
            .retain(|t| now.saturating_duration_since(*t) < Duration::from_secs(1));
        self.recent.push(now);
        if self.recent.len() > MAX_MESSAGES_PER_SECOND {
            return events.push(self.violation(Kill::Flood));
        }
        events.push(message.map_or(Ev::Garbage, Ev::Message));
    }

    fn violation(&mut self, why: Kill) -> Ev {
        self.dead = true;
        self.pending.clear();
        Ev::Violation(why)
    }
}

/// stderr in Zeilen, auf `MAX_TEXT` Zeichen gekürzt und gedrosselt.
#[derive(Default)]
struct StderrLines {
    throttle: Throttle,
    pending: Vec<u8>,
}

fn clip_bytes(data: &[u8]) -> String {
    clip(&String::from_utf8_lossy(data), MAX_TEXT)
}

impl StderrLines {
    fn feed(&mut self, chunk: &[u8], now: Instant) -> Vec<Ev> {
        self.pending.extend_from_slice(chunk);
        let mut lines = Vec::new();
        let mut start = 0;
        while let Some(rel) = self.pending[start..].iter().position(|b| *b == b'\n') {
            let mut end = start + rel;
            if end > start && self.pending[end - 1] == b'\r' {
                end -= 1;
            }
            lines.push(clip_bytes(&self.pending[start..end]));
            start += rel + 1;
        }
        self.pending.drain(..start);
        // Ein Rest ohne Zeilenende über 4 KB gilt als eigene Zeile, damit der Puffer nicht unbegrenzt wächst.
        if self.pending.len() > 4096 {
            lines.push(clip_bytes(&self.pending));
            self.pending.clear();
        }
        self.admit(lines, now)
    }

    /// Bei EOF: Rest ohne Zeilenende und ausstehende Sammelzeile sofort.
    fn finish(&mut self, now: Instant) -> Vec<Ev> {
        let rest = if self.pending.is_empty() {
            Vec::new()
        } else {
            vec![clip_bytes(&self.pending)]
        };
        self.pending.clear();
        let mut events = self.admit(rest, now);
        events.extend(self.throttle.flush());
        events
    }

    fn admit(&mut self, lines: Vec<String>, now: Instant) -> Vec<Ev> {
        let mut events = Vec::new();
        let allowed: Vec<String> = lines
            .into_iter()
            .filter(|_| self.throttle.admit(now, &mut events))
            .collect();
        if !allowed.is_empty() {
            events.push(Ev::Stderr(allowed));
        }
        events
    }
}

// MARK: Akteur

struct Actor {
    child: Child,
    group: Option<KillGroup>,
    writer: Sender<Option<Vec<u8>>>,
    unwritten: Arc<AtomicUsize>,
    handler: EventHandler,
    stopping: Arc<AtomicBool>,
    protocol: ProtocolLines,
    stderr: StderrLines,
    ready_deadline: Option<Instant>,
    ping_at: Option<Instant>,
    term_at: Option<Instant>,
    kill_at: Option<Instant>,
    awaiting_pong: Option<i64>,
    last_ping: i64,
    killed: Option<Kill>,
    stop_started: bool,
    out_eof: bool,
    err_eof: bool,
    exit: Option<ExitStatus>,
    grace_at: Option<Instant>,
    finished: bool,
    last_error: Option<String>,
    last_stderr: Option<String>,
    next_poll: Instant,
}

impl Actor {
    fn new(
        child: Child,
        group: Option<KillGroup>,
        writer: Sender<Option<Vec<u8>>>,
        unwritten: Arc<AtomicUsize>,
        handler: EventHandler,
        stopping: Arc<AtomicBool>,
    ) -> Actor {
        Actor {
            child,
            group,
            writer,
            unwritten,
            handler,
            stopping,
            protocol: ProtocolLines::default(),
            stderr: StderrLines::default(),
            ready_deadline: Some(Instant::now() + READY_TIMEOUT),
            ping_at: None,
            term_at: None,
            kill_at: None,
            awaiting_pong: None,
            last_ping: 0,
            killed: None,
            stop_started: false,
            out_eof: false,
            err_eof: false,
            exit: None,
            grace_at: None,
            finished: false,
            last_error: None,
            last_stderr: None,
            next_poll: Instant::now(),
        }
    }

    fn run(mut self, rx: mpsc::Receiver<Cmd>, _keepalive: Sender<Cmd>, hello: HostMessage) {
        self.send(&hello);
        while !self.finished {
            let wait = self
                .next_deadline()
                .saturating_duration_since(Instant::now());
            match rx.recv_timeout(wait) {
                Ok(cmd) => self.on_cmd(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.tick(Instant::now());
        }
    }

    fn next_deadline(&self) -> Instant {
        let mut due = vec![self.next_poll];
        due.extend(
            [
                self.ready_deadline,
                self.ping_at,
                self.term_at,
                self.kill_at,
                self.grace_at,
            ]
            .into_iter()
            .flatten(),
        );
        due.extend(
            [self.protocol.logs.due(), self.stderr.throttle.due()]
                .into_iter()
                .flatten(),
        );
        due.into_iter().min().unwrap_or_else(Instant::now)
    }

    fn on_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Out(chunk) => {
                let events = self.protocol.feed(&chunk, Instant::now());
                self.handle_all(events);
            }
            Cmd::Err(chunk) => {
                let events = self.stderr.feed(&chunk, Instant::now());
                self.handle_all(events);
            }
            Cmd::OutEof => {
                let mut events: Vec<Ev> = self.protocol.logs.flush().into_iter().collect();
                self.handle_all(std::mem::take(&mut events));
                self.out_eof = true;
                self.finish_if_done();
            }
            Cmd::ErrEof => {
                let events = self.stderr.finish(Instant::now());
                self.handle_all(events);
                self.err_eof = true;
                self.finish_if_done();
            }
            Cmd::Send(m) => self.send(&m),
            Cmd::Stop => self.stop(),
            Cmd::Kill => self.kill(),
            Cmd::Abandon => self.abandon(),
        }
    }

    /// Zeitgesteuertes: Drosselfenster, Timer, Prozessende, Wartezeit auf die Pipes.
    fn tick(&mut self, now: Instant) {
        let mut events: Vec<Ev> = Vec::new();
        events.extend(self.protocol.logs.tick(now));
        events.extend(self.stderr.throttle.tick(now));
        self.handle_all(events);
        if self.ready_deadline.is_some_and(|t| now >= t) {
            self.ready_deadline = None;
            self.hard_kill(Kill::NoReady);
        }
        if self.ping_at.is_some_and(|t| now >= t) {
            self.ping_at = Some(now + PING_INTERVAL);
            self.ping();
        }
        if self.term_at.is_some_and(|t| now >= t) {
            self.term_at = None;
            self.signal(false);
        }
        if self.kill_at.is_some_and(|t| now >= t) {
            self.kill_at = None;
            self.signal(true);
        }
        self.poll_exit(now);
    }

    fn poll_exit(&mut self, now: Instant) {
        if self.exit.is_none() && now >= self.next_poll {
            self.next_poll = now + EXIT_POLL;
            if let Ok(Some(status)) = self.child.try_wait() {
                self.exit = Some(status);
                self.grace_at = Some(now + PIPE_GRACE);
                self.finish_if_done();
            }
        }
        if self.exit.is_some() && self.grace_at.is_some_and(|t| now >= t) {
            self.out_eof = true;
            self.err_eof = true;
            self.finish_if_done();
        }
    }

    fn handle_all(&mut self, events: Vec<Ev>) {
        for e in events {
            self.handle(e);
        }
    }

    fn handle(&mut self, e: Ev) {
        match e {
            Ev::Message(m) => self.receive(m),
            Ev::Garbage => {
                if !self.finished {
                    (self.handler)(ProcessEvent::Stderr(tr(
                        "ungültige Nachricht verworfen",
                        "invalid message discarded",
                    )));
                }
            }
            Ev::Violation(why) => self.hard_kill(why),
            Ev::Stderr(lines) => {
                self.last_stderr = lines.last().cloned().or_else(|| self.last_stderr.take());
                for l in lines {
                    (self.handler)(ProcessEvent::Stderr(l));
                }
            }
            Ev::Skipped(n) => {
                let n = group_digits(n);
                (self.handler)(ProcessEvent::Stderr(tr_args(
                    "{0} Zeilen übersprungen",
                    "{0} lines skipped",
                    &[&n],
                )));
            }
        }
    }

    fn receive(&mut self, m: ExtensionMessage) {
        if self.finished || self.killed.is_some() {
            return;
        }
        match m {
            ExtensionMessage::Ready if self.ready_deadline.is_some() => {
                self.ready_deadline = None;
                self.ping_at = Some(Instant::now() + PING_INTERVAL);
            }
            ExtensionMessage::Pong(id) if Some(id) == self.awaiting_pong => {
                self.awaiting_pong = None
            }
            ExtensionMessage::Log { level, text } => {
                let clipped = clip(&text, MAX_TEXT);
                if level == "error" {
                    self.last_error = clipped.split('\n').next().map(String::from);
                }
                return (self.handler)(ProcessEvent::Message(ExtensionMessage::Log {
                    level,
                    text: clipped,
                }));
            }
            _ => {}
        }
        (self.handler)(ProcessEvent::Message(m));
    }

    // MARK: Ausgang

    fn send(&mut self, m: &HostMessage) {
        if self.finished || self.killed.is_some() || self.exit.is_some() {
            return;
        }
        let bytes = m.line();
        let total = self.unwritten.fetch_add(bytes.len(), Ordering::SeqCst) + bytes.len();
        let _ = self.writer.send(Some(bytes));
        if total > MAX_INPUT_BUFFER {
            self.hard_kill(Kill::InputFull);
        }
    }

    fn ping(&mut self) {
        if self.awaiting_pong.is_some() {
            return self.hard_kill(Kill::NoPong);
        }
        self.last_ping += 1;
        self.awaiting_pong = Some(self.last_ping);
        self.send(&HostMessage::Ping(self.last_ping));
    }

    // MARK: Beenden

    fn cancel_timers(&mut self) {
        self.ready_deadline = None;
        self.ping_at = None;
        self.term_at = None;
        self.kill_at = None;
    }

    fn hard_kill(&mut self, why: Kill) {
        if self.killed.is_some() || self.finished {
            return;
        }
        self.killed = Some(why);
        self.cancel_timers();
        self.signal(true);
    }

    fn stop(&mut self) {
        if self.finished || self.killed.is_some() || self.stop_started || self.exit.is_some() {
            return;
        }
        self.stop_started = true;
        self.cancel_timers();
        self.send(&HostMessage::Shutdown);
        let now = Instant::now();
        self.term_at = Some(now + Duration::from_secs(1));
        self.kill_at = Some(now + Duration::from_secs(2));
    }

    fn kill(&mut self) {
        if self.killed.is_some() || self.finished {
            return;
        }
        self.stop_started = true;
        self.cancel_timers();
        self.signal(true);
    }

    /// Ohne `stop()` fallen gelassen: stdin schließen, nach 2 s killen.
    fn abandon(&mut self) {
        if self.finished || self.stop_started || self.killed.is_some() {
            return;
        }
        self.stop_started = true;
        self.stopping.store(true, Ordering::SeqCst);
        self.cancel_timers();
        let _ = self.writer.send(None);
        self.kill_at = Some(Instant::now() + Duration::from_secs(2));
    }

    /// Nur solange der Prozess lebt: nach dem Einsammeln könnte die pid schon einem anderen Prozess gehören.
    fn signal(&mut self, hard: bool) {
        if self.finished || self.exit.is_some() {
            return;
        }
        // Unter Unix zuerst die Kinder aus `kadrell.exec` (eigene Prozessgruppen), unter Windows trägt das Job Object sie.
        #[cfg(unix)]
        sys::kill_descendants(self.child.id());
        match (&self.group, hard) {
            (Some(g), true) => g.kill(),
            (Some(g), false) => g.terminate(),
            (None, true) => {
                let _ = self.child.kill();
            }
            (None, false) => {}
        }
    }

    // MARK: Ende

    /// Erst wenn der Prozess eingesammelt und beide Pipes zu Ende gelesen sind, damit die letzte Fehlermeldung im Grund
    /// steht.
    fn finish_if_done(&mut self) {
        if self.finished || !self.out_eof || !self.err_eof {
            return;
        }
        let Some(status) = self.exit else { return };
        self.finished = true;
        self.cancel_timers();
        let _ = self.writer.send(None);
        let expected = self.stop_started;
        let reason = self.exit_reason(status);
        (self.handler)(ProcessEvent::Exit { expected, reason });
    }

    fn exit_reason(&self, status: ExitStatus) -> String {
        if self.stop_started {
            return tr("Gestoppt", "Stopped");
        }
        if let Some(why) = self.killed {
            return why.reason();
        }
        let (signal, code) = exit_code(status);
        let code = code.to_string();
        let base = if signal {
            tr_args("Beendet durch Signal {0}", "Killed by signal {0}", &[&code])
        } else {
            tr_args("Beendet mit Status {0}", "Exited with status {0}", &[&code])
        };
        match self.last_error.as_ref().or(self.last_stderr.as_ref()) {
            Some(detail) => format!("{base}: {detail}"),
            None => base,
        }
    }
}

/// Ob ein Signal das Ende war, und der Code (Signalnummer oder Exit-Status).
fn exit_code(status: ExitStatus) -> (bool, i32) {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return (true, sig);
        }
    }
    (false, status.code().unwrap_or(-1))
}

/// 1234 wird zu "1,234" bzw. nach Sprache "1.234", wie die Zahlenformatierung der Swift-App (Tausendertrenner).
fn group_digits(n: usize) -> String {
    let digits = n.to_string();
    let sep = if crate::i18n::locale() == "en" {
        ','
    } else {
        '.'
    };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Ob `text` eine Sammelzeile "N Zeilen übersprungen" ist (Zahl mit Tausendertrennern), in beiden Sprachen.
pub fn is_skipped_summary(text: &str) -> bool {
    [("{0} Zeilen übersprungen"), ("{0} lines skipped")]
        .iter()
        .any(|pattern| {
            let (pre, post) = pattern.split_once("{0}").unwrap_or(("", ""));
            text.strip_prefix(pre)
                .and_then(|t| t.strip_suffix(post))
                .is_some_and(|n| {
                    n.chars().next().is_some_and(|c| c.is_ascii_digit())
                        && n.chars()
                            .all(|c| c.is_ascii_digit() || ".,\u{a0} ".contains(c))
                })
        })
}

/// Sammelt Ereignisse eines Prozesses für Tests und einfache Einbettungen.
#[derive(Default, Clone)]
pub struct EventLog(Arc<Mutex<Vec<ProcessEvent>>>);

impl EventLog {
    pub fn new() -> EventLog {
        EventLog::default()
    }

    pub fn handler(&self) -> EventHandler {
        let sink = Arc::clone(&self.0);
        Arc::new(move |e| sink.lock().unwrap_or_else(|p| p.into_inner()).push(e))
    }

    pub fn events(&self) -> Vec<ProcessEvent> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn messages(&self) -> Vec<ExtensionMessage> {
        self.events()
            .into_iter()
            .filter_map(|e| {
                if let ProcessEvent::Message(m) = e {
                    Some(m)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn stderr(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|e| {
                if let ProcessEvent::Stderr(s) = e {
                    Some(s)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn exits(&self) -> Vec<(bool, String)> {
        self.events()
            .into_iter()
            .filter_map(|e| {
                if let ProcessEvent::Exit { expected, reason } = e {
                    Some((expected, reason))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_admits_fifty_per_window_and_summarizes() {
        let mut t = Throttle::default();
        let now = Instant::now();
        let mut ev = Vec::new();
        let admitted = (0..120).filter(|_| t.admit(now, &mut ev)).count();
        assert_eq!(admitted, 50);
        assert_eq!(t.due(), Some(now + Duration::from_secs(1)));
        assert_eq!(
            t.tick(now + Duration::from_millis(500)),
            None,
            "Fenster läuft noch"
        );
        assert_eq!(t.tick(now + Duration::from_secs(1)), Some(Ev::Skipped(70)));
        assert!(
            t.admit(now + Duration::from_secs(1), &mut ev),
            "neues Fenster"
        );
    }

    #[test]
    fn protocol_lines_count_flood_but_not_run_or_log() {
        let mut p = ProtocolLines::default();
        let now = Instant::now();
        let runs = "{\"t\":\"run\",\"id\":1,\"argv\":[]}\n".repeat(200);
        assert!(
            p.feed(runs.as_bytes(), now)
                .iter()
                .all(|e| matches!(e, Ev::Message(_)))
        );
        let status = "{\"t\":\"status\",\"item\":null}\n".repeat(60);
        let events = p.feed(status.as_bytes(), now);
        assert!(events.contains(&Ev::Violation(Kill::Flood)));
        assert!(
            p.feed(b"{\"t\":\"ready\"}\n", now).is_empty(),
            "nach einem Verstoß wird alles verworfen"
        );
    }

    #[test]
    fn protocol_lines_reassemble_split_chunks_and_flag_garbage() {
        let mut p = ProtocolLines::default();
        let now = Instant::now();
        assert!(p.feed(b"{\"t\":\"rea", now).is_empty());
        assert_eq!(
            p.feed(b"dy\"}\nkaputt\n", now),
            vec![Ev::Message(ExtensionMessage::Ready), Ev::Garbage]
        );
    }

    #[test]
    fn protocol_lines_reject_overlong_lines() {
        let mut p = ProtocolLines::default();
        let long = vec![b'a'; MAX_LINE + 1];
        assert_eq!(
            p.feed(&long, Instant::now()),
            vec![Ev::Violation(Kill::LongLine)]
        );
    }

    #[test]
    fn stderr_lines_are_clipped_and_split() {
        let mut s = StderrLines::default();
        let mut data = b"eins\r\n".to_vec();
        data.extend(vec![b'b'; 2000]);
        data.push(b'\n');
        let events = s.feed(&data, Instant::now());
        assert_eq!(
            events,
            vec![Ev::Stderr(vec!["eins".into(), "b".repeat(500)])]
        );
    }

    #[test]
    fn skipped_summary_pattern() {
        assert!(is_skipped_summary("1.234 Zeilen übersprungen"));
        assert!(is_skipped_summary("70 lines skipped"));
        assert!(!is_skipped_summary("viele Zeilen übersprungen"));
        assert_eq!(
            group_digits(1234567),
            if crate::i18n::locale() == "en" {
                "1,234,567"
            } else {
                "1.234.567"
            }
        );
    }
}
