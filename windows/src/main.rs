//! Kadrell für Windows. Ein Binary für beides, wie auf dem Mac: ohne Befehl (oder nur mit `--profile`) öffnet es
//! das Fenster im laufenden Terminal und hält die Sessions; mit Befehl (`kadrell ls`, `kadrell new` …) schickt es
//! ihn an die laufende Instanz. Läuft keine, startet es sie in einem eigenen Windows-Terminal-Fenster.

mod app;
mod command;
mod hook;
mod input;
mod profile;
mod protocol;
mod pty;
mod store;
mod ui;

use crate::app::App;
use crate::profile::Profile;
use crate::protocol::{ControlRequest, ControlResponse, Endpoint};
use crossterm::event::{self, Event, KeyEventKind};
use std::io::Read;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

type Job = (ControlRequest, Sender<ControlResponse>);

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let code = match argv.first().map(String::as_str) {
        None => run_app(None),
        Some("--profile") => match argv.get(1) {
            Some(name) if argv.len() == 2 => run_app(Some(name)),
            _ => fail("--profile braucht genau einen Namen"),
        },
        Some("hook") => run_hook(&argv),
        Some(_) => run_cli(argv),
    };
    std::process::exit(code);
}

fn fail(message: &str) -> i32 {
    eprintln!("kadrell: {message}");
    1
}

// MARK: App

fn run_app(profile: Option<&str>) -> i32 {
    let profile = match Profile::open(profile) {
        Ok(p) => p,
        Err(e) => return fail(&format!("Profil: {e}")),
    };
    if profile::alive(&profile.dir) {
        return fail("Kadrell läuft in diesem Profil schon");
    }
    let (tx, rx) = mpsc::channel::<Job>();
    if let Err(e) = serve(&profile, tx) {
        return fail(&format!("Steuerkanal: {e}"));
    }
    let mut app = App::new(profile);
    let result = event_loop(&mut app, &rx);
    app.shutdown();
    app.profile.close();
    match result {
        Ok(()) => 0,
        Err(e) => fail(&e.to_string()),
    }
}

/// Lauscht auf 127.0.0.1 mit zufälligem Port und reicht jede Anfrage mit passendem Token an die Ereignisschleife.
fn serve(profile: &Profile, tx: Sender<Job>) -> std::io::Result<Endpoint> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let token = format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple());
    let endpoint = Endpoint { port: listener.local_addr()?.port(), token: token.clone(), pid: std::process::id() };
    endpoint.write(&profile.endpoint_path())?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            let token = token.clone();
            std::thread::spawn(move || answer(stream, &tx, &token));
        }
    });
    Ok(endpoint)
}

fn answer(stream: std::net::TcpStream, tx: &Sender<Job>, token: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Some(line) = protocol::read_line(&stream) else { return };
    let response = match serde_json::from_slice::<ControlRequest>(&line) {
        Ok(req) if req.token == token => {
            let (reply_tx, reply_rx) = mpsc::channel();
            if tx.send((req, reply_tx)).is_err() {
                return;
            }
            reply_rx.recv().unwrap_or_else(|_| ControlResponse::fail("Kadrell beendet sich"))
        }
        Ok(_) => return,
        Err(_) => ControlResponse::fail("ungültige Anfrage"),
    };
    let _ = protocol::write_line(&stream, &response);
}

fn event_loop(app: &mut App, rx: &Receiver<Job>) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mut ui = ui::Ui::new();
    let result = (|| -> std::io::Result<()> {
        let mut last_exit_poll = Instant::now();
        while !ui.quit && !app.quit_requested {
            while let Ok((req, reply)) = rx.try_recv() {
                let _ = reply.send(app.handle(&req));
            }
            if last_exit_poll.elapsed() > Duration::from_millis(250) {
                app.poll_exits();
                last_exit_poll = Instant::now();
            }
            let area = ui::tile_area(terminal.size()?.into(), ui.sidebar);
            if (area.height, area.width) != app.tile_size {
                app.resize_tiles(area.height, area.width);
                app.mark_dirty();
            }
            if let Some(key) = app.focused.clone().filter(|k| !app.tiles.contains_key(k) && !app.ended.contains(k)) {
                app.focus(&key);
            }
            if app.dirty.swap(false, Ordering::AcqRel) {
                terminal.draw(|f| ui::draw(f, app, &ui))?;
            }
            if event::poll(Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(k) if k.kind != KeyEventKind::Release => input::handle_key(app, &mut ui, k),
                    Event::Paste(text) => paste(app, &text),
                    _ => app.mark_dirty(),
                }
            }
        }
        Ok(())
    })();
    ratatui::restore();
    result
}

fn paste(app: &App, text: &str) {
    let Some(tile) = app.focused.as_ref().and_then(|k| app.tiles.get(k)).filter(|t| t.alive) else { return };
    let data = if tile.pane.bracketed_paste() { format!("\u{1b}[200~{text}\u{1b}[201~") } else { text.to_string() };
    tile.pane.write(data.as_bytes());
}

// MARK: Kommandozeile

fn endpoint_path() -> (PathBuf, bool) {
    match std::env::var_os("KADRELL_SOCKET") {
        Some(p) => (PathBuf::from(p), true),
        None => (profile::default_dir().join(profile::ENDPOINT), false),
    }
}

fn run_cli(argv: Vec<String>) -> i32 {
    // Vor dem Verbinden prüfen: ein Tippfehler soll die App nicht starten.
    match command::parse(&argv) {
        Ok(command::Command::Help) => {
            println!("{}", command::USAGE);
            return 0;
        }
        Ok(_) => {}
        Err(e) => return fail(&e.0),
    }
    let req = ControlRequest {
        argv,
        cwd: std::env::current_dir().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default(),
        caller: std::env::var("KADRELL_SESSION_KEY").ok().filter(|s| !s.is_empty()),
        token: String::new(),
    };
    let (path, explicit) = endpoint_path();
    let response = match request(&path, &req, !explicit) {
        Ok(r) => r,
        Err(e) => return fail(&e),
    };
    print!("{}", response.stdout);
    eprint!("{}", response.stderr);
    response.status
}

/// Schickt die Anfrage; läuft keine Instanz und darf eine starten (`launch`), startet sie und wartet bis 20 s.
fn request(path: &std::path::Path, req: &ControlRequest, launch: bool) -> Result<ControlResponse, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut launched = false;
    loop {
        if let Some(endpoint) = Endpoint::read(path) {
            match protocol::send(&endpoint, req, Duration::from_secs(30))? {
                Some(r) if r.status != protocol::STARTING_STATUS => return Ok(r),
                _ => {}
            }
        }
        if !launch {
            return Err(format!("Kadrell läuft nicht (Endpunkt {})", path.display()));
        }
        if !launched {
            launch_app()?;
            launched = true;
        }
        if Instant::now() > deadline {
            return Err("Kadrell startet nicht (20 s gewartet)".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Eigenes Windows-Terminal-Fenster „kadrell“, sonst eine neue Konsole.
fn launch_app() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let wt = std::process::Command::new("wt.exe").args(["-w", "kadrell", "new-tab", "--title", "Kadrell"]).arg(&exe).spawn();
    if wt.is_ok() {
        return Ok(());
    }
    std::process::Command::new("cmd.exe").args(["/c", "start", "Kadrell"]).arg(&exe).spawn().map(|_| ()).map_err(|e| format!("Kadrell starten: {e}"))
}

/// `kadrell hook claude`: außerhalb einer Kachel oder bei jedem Fehler still mit 0, damit Claude nie hängt.
fn run_hook(argv: &[String]) -> i32 {
    if argv.get(1).map(String::as_str) != Some("claude") || std::env::var_os("KADRELL_SOCKET").is_none() {
        return 0;
    }
    let mut input = Vec::new();
    if std::io::stdin().take(protocol::MAX_REQUEST as u64).read_to_end(&mut input).is_err() {
        return 0;
    }
    let env: std::collections::HashMap<String, String> = std::env::vars().collect();
    let Some(args) = hook::status_argv(&input, &env, &profile::home()) else { return 0 };
    let req = ControlRequest {
        argv: args,
        cwd: String::new(),
        caller: env.get("KADRELL_SESSION_KEY").cloned(),
        token: String::new(),
    };
    let (path, _) = endpoint_path();
    if let Some(endpoint) = Endpoint::read(&path) {
        let _ = protocol::send(&endpoint, &req, Duration::from_secs(4));
    }
    0
}
