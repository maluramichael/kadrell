//! Die Kommandozeile `kadrell <befehl>`: dasselbe Binary wie die App. Schickt die Anfrage an den Kanal der laufenden
//! App und startet sie, falls sie nicht läuft (wie `ControlClient` der Mac-App). Alles Äußere (Pfad, Aufrufer,
//! Ordner, Starten, Wartezeiten) kommt über [`ClientOptions`] herein, damit Tests ohne Umgebungsvariablen laufen.

use crate::command::{self, Command};
use crate::endpoint::default_endpoint_path;
use crate::protocol::{ControlRequest, ControlResponse, STARTING_STATUS, read_line, write_line};
use kadrell_core::Paths;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// So lange wartet die CLI auf eine startende App (Login-Shell-Umgebung einlesen braucht ein paar Sekunden).
pub const WAIT_FOR_APP: Duration = Duration::from_secs(30);
pub const POLL: Duration = Duration::from_millis(250);

/// Startet die App und beantwortet, ob sie schon läuft.
pub trait Launcher {
    /// Läuft die App (dieses Profils) schon, ohne dass ihr Kanal antwortet? Dann wird nicht noch eine gestartet.
    fn already_running(&self) -> bool;
    /// Hat der Nutzer die App gerade beendet? Dann startet die CLI sie nicht neu (`profile::quit_recently`).
    fn quit_recently(&self) -> bool;
    fn launch(&self) -> Result<(), String>;
}

pub struct ClientOptions<'a> {
    /// Socket (Unix) bzw. Endpunktdatei (Windows).
    pub socket: PathBuf,
    /// `KADRELL_SESSION_KEY` des Aufrufers, nur ein Hinweis: der Server bestimmt die Kachel selbst.
    pub caller: Option<String>,
    pub cwd: String,
    pub launcher: &'a dyn Launcher,
    pub wait: Duration,
    pub poll: Duration,
}

/// Schickt die Anfrage, `Ok(None)`: niemand lauscht. Ein Fehler beim Verbinden heißt immer „niemand lauscht“ (kein
/// Socket, abgelehnt, Rest eines Absturzes); danach ist jeder Fehler einer.
pub fn send(path: &Path, req: &ControlRequest, timeout: Duration) -> Result<Option<ControlResponse>, String> {
    crate::endpoint::check_socket_path(path)?;
    let Some((mut stream, req)) = connect(path, req) else { return Ok(None) };
    stream.set_timeout(timeout).map_err(|e| e.to_string())?;
    write_line(&mut stream, &req).map_err(|e| e.to_string())?;
    stream.finish_write();
    let line = read_line(&mut stream).ok_or("keine Antwort von Kadrell")?;
    serde_json::from_slice(&line).map(Some).map_err(|e| e.to_string())
}

/// So lange darf die App mit der Antwort brauchen.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

/// Eine offene Verbindung zur App, auf allen Systemen gleich bedienbar.
trait Duplex: Read + Write {
    fn set_timeout(&self, timeout: Duration) -> std::io::Result<()>;
    /// Der Server liest bis Zeilenende oder EOF: das Schreiben beenden, ohne das Lesen zu beenden.
    fn finish_write(&self);
}

#[cfg(unix)]
impl Duplex for std::os::unix::net::UnixStream {
    fn set_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        self.set_read_timeout(Some(timeout))
    }
    fn finish_write(&self) {
        let _ = self.shutdown(std::net::Shutdown::Write);
    }
}

#[cfg(windows)]
impl Duplex for std::net::TcpStream {
    fn set_timeout(&self, timeout: Duration) -> std::io::Result<()> {
        self.set_read_timeout(Some(timeout))
    }
    fn finish_write(&self) {
        let _ = self.shutdown(std::net::Shutdown::Write);
    }
}

/// Verbindung samt der Anfrage, wie sie auf diesem System gesendet wird (Windows trägt das Token ein).
#[cfg(unix)]
fn connect(path: &Path, req: &ControlRequest) -> Option<(Box<dyn Duplex>, ControlRequest)> {
    let stream = std::os::unix::net::UnixStream::connect(path).ok()?;
    Some((Box::new(stream), req.clone()))
}

#[cfg(windows)]
fn connect(path: &Path, req: &ControlRequest) -> Option<(Box<dyn Duplex>, ControlRequest)> {
    let endpoint = crate::endpoint::Endpoint::read(path)?;
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], endpoint.port));
    let stream = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok()?;
    Some((Box::new(stream), ControlRequest { token: endpoint.token, ..req.clone() }))
}

#[cfg(not(any(unix, windows)))]
fn connect(_path: &Path, _req: &ControlRequest) -> Option<(Box<dyn Duplex>, ControlRequest)> {
    None
}

/// Führt einen Befehl aus: Ausgabe der App nach `out`, Fehler nach `err`, Rückgabe ist der Exit-Code. `hook claude`
/// ist nicht Teil davon (siehe [`crate::hook::run_hook`]).
pub fn run(argv: &[String], opts: &ClientOptions, out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    // Vor dem Verbinden prüfen: ein Tippfehler soll die App nicht starten.
    match command::parse(argv) {
        Ok(Command::Help) => {
            let _ = writeln!(out, "{}", command::USAGE);
            return 0;
        }
        Ok(_) => {}
        Err(e) => return fail(err, &e.0),
    }
    let req = ControlRequest::new(argv.to_vec(), opts.cwd.clone(), opts.caller.clone());
    match request(&req, opts, err) {
        Ok(resp) => {
            let _ = out.write_all(resp.stdout.as_bytes());
            let _ = err.write_all(resp.stderr.as_bytes());
            resp.status
        }
        Err(message) => fail(err, &message),
    }
}

fn fail(err: &mut dyn Write, message: &str) -> i32 {
    let _ = writeln!(err, "kadrell: {message}");
    1
}

/// Schickt die Anfrage und wartet auf eine startende App: kein Kanal (App startet gerade erst) oder Kanal da, aber
/// die App bootet noch (`STARTING_STATUS`), beides bis `wait`.
fn request(req: &ControlRequest, opts: &ClientOptions, err: &mut dyn Write) -> Result<ControlResponse, String> {
    let path = &opts.socket;
    let timeout = RESPONSE_TIMEOUT;
    let mut resp = send(path, req, timeout)?;
    let running = opts.launcher.already_running();
    if resp.is_none() && !running {
        launch_unless_quit(opts, err)?;
    }
    let deadline = Instant::now() + opts.wait;
    while resp.as_ref().is_none_or(|r| r.status == STARTING_STATUS) && Instant::now() < deadline {
        std::thread::sleep(opts.poll);
        resp = send(path, req, timeout)?;
    }
    match resp {
        Some(r) if r.status != STARTING_STATUS => Ok(r),
        _ if running => Err(format!("Kadrell läuft, lauscht aber nicht auf {}. Alte Version? Kadrell neu starten.", path.display())),
        _ => Err(format!("Kadrell antwortet nicht ({})", path.display())),
    }
}

fn launch_unless_quit(opts: &ClientOptions, err: &mut dyn Write) -> Result<(), String> {
    if opts.launcher.quit_recently() {
        return Err("Kadrell wurde gerade beendet und startet nicht von selbst neu. Von Hand öffnen, dann geht es wieder.".into());
    }
    let _ = writeln!(err, "kadrell: starte Kadrell …");
    opts.launcher.launch()
}

/// Startet dieses Binary losgelöst (kein Terminal, überlebt die CLI), mit `--profile <name>`, falls gesetzt.
pub struct SelfLauncher {
    pub exe: PathBuf,
    pub profile: Option<String>,
    /// Profilordner, für den Marker „gerade beendet“.
    pub profile_dir: PathBuf,
}

impl SelfLauncher {
    /// Dieses Binary, Standardprofil oder das von `--profile`.
    pub fn from_env(paths: &Paths, profile: Option<String>) -> SelfLauncher {
        let profile_dir = match profile.as_deref() {
            None => paths.default_profile_dir(),
            Some(name) => paths.named_profile_dir(name),
        };
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("kadrell"));
        SelfLauncher { exe, profile, profile_dir }
    }
}

impl Launcher for SelfLauncher {
    /// Die pid-Datei des Profils nennt eine noch lebende Instanz: dann läuft Kadrell, auch wenn der Kanal schweigt
    /// (alte Version, Start noch nicht fertig).
    fn already_running(&self) -> bool {
        use crate::caller::{ProcessTree, SystemProcessTree};
        kadrell_core::profile::recorded_pid(&self.profile_dir).is_some_and(|pid| SystemProcessTree.parent(pid).is_some())
    }

    fn quit_recently(&self) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
        self.profile.as_deref() != Some(kadrell_core::profile::TEMP_NAME) && kadrell_core::profile::quit_recently(&self.profile_dir, now)
    }

    fn launch(&self) -> Result<(), String> {
        let mut cmd = std::process::Command::new(&self.exe);
        if let Some(name) = &self.profile {
            cmd.args(["--profile", name]);
        }
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        detach(&mut cmd);
        cmd.spawn().map(|_| ()).map_err(|e| format!("Kadrell starten: {e}"))
    }
}

#[cfg(unix)]
fn detach(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: `setsid` zwischen fork und exec ist async-signal-safe und ändert nur den Kindprozess.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
}

#[cfg(windows)]
fn detach(cmd: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn detach(_cmd: &mut std::process::Command) {}

/// Optionen aus der Umgebung des Prozesses: `KADRELL_SOCKET` überschreibt den Pfad (in jeder Kachel gesetzt), sonst
/// der Kanal des Profils. `None`, wenn kein Pfad zu bestimmen ist (Wegwerfprofil ohne `KADRELL_SOCKET`).
pub fn socket_from_env(profile: Option<&str>) -> Option<PathBuf> {
    match std::env::var_os("KADRELL_SOCKET").filter(|p| !p.is_empty()) {
        Some(path) => Some(PathBuf::from(path)),
        None => default_endpoint_path(&Paths::from_env(), profile),
    }
}

/// `KADRELL_SESSION_KEY` der aufrufenden Kachel, falls gesetzt.
pub fn caller_from_env() -> Option<String> {
    std::env::var("KADRELL_SESSION_KEY").ok().filter(|k| !k.is_empty())
}

/// Aktueller Ordner des Aufrufers.
pub fn cwd_from_env() -> String {
    std::env::current_dir().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default()
}

/// Der ganze Weg für das Binary: Umgebung lesen, Befehl ausführen, nach stdout und stderr schreiben. `hook claude`
/// geht an `hook_map`, siehe [`crate::hook::run_hook`]. Gibt den Exit-Code zurück.
pub fn main_cli(argv: &[String], profile: Option<String>, hook_map: &dyn Fn(&[u8]) -> Option<Vec<String>>) -> i32 {
    let mut err = std::io::stderr();
    if argv.first().map(String::as_str) == Some("hook") {
        let env = crate::hook::HookEnv::from_env();
        return crate::hook::run_hook(argv, &env, &mut std::io::stdin().lock(), hook_map, &mut err);
    }
    let Some(socket) = socket_from_env(profile.as_deref()) else {
        return fail(&mut err, "Wegwerfprofil: KADRELL_SOCKET zeigt auf den Kanal der Instanz, ohne läuft keine zu finden");
    };
    let launcher = SelfLauncher::from_env(&Paths::from_env(), profile);
    let opts = ClientOptions { socket, caller: caller_from_env(), cwd: cwd_from_env(), launcher: &launcher, wait: WAIT_FOR_APP, poll: POLL };
    run(argv, &opts, &mut std::io::stdout(), &mut err)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn launcher(dir: &Path) -> SelfLauncher {
        SelfLauncher { exe: PathBuf::from("kadrell"), profile: None, profile_dir: dir.to_path_buf() }
    }

    /// Eine Instanz ohne Kanal erkennt die CLI an der pid-Datei des Profils: lebt die pid, wird keine zweite gestartet.
    #[test]
    fn already_running_follows_the_recorded_pid() {
        let dir = crate::testing::TempDir::new();
        assert!(!launcher(dir.path()).already_running(), "ohne pid-Datei läuft nichts");
        std::fs::write(dir.path().join("kadrell.pid"), format!("{}\n", std::process::id())).unwrap();
        assert!(launcher(dir.path()).already_running());
        std::fs::write(dir.path().join("kadrell.pid"), "4294967290\n").unwrap();
        assert!(!launcher(dir.path()).already_running(), "tote pid");
    }
}
