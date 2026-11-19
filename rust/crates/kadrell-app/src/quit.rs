//! Beenden und Neustart (Swift: `applicationShouldTerminate`, `confirmShutdown`, `restart` im AppDelegate). Laufen
//! Claude-Prozesse, fragt jeder Weg ins Beenden zuerst nach: Strg+Q und Menü, Tray-Menü, Signale (SIGTERM, SIGINT,
//! SIGHUP) und „Kadrell neu starten“. Die Rückfrage ist ein Overlay, das eigentliche Beenden (`cx.quit`) folgt erst
//! nach der Bestätigung. Ohne laufende Prozesse oder mit abgeschalteter Rückfrage („Kadrell beenden, während Claude
//! läuft“) geht es sofort. `kadrell kill-server` fragt nie, wie `tmux kill-server`.
//!
//! Nach der Bestätigung bekommen alle Prozesse ihr Ende (SIGHUP bzw. `taskkill`), die App wartet bis zu fünf Sekunden,
//! und was dann noch lebt, wird hart beendet. Der Neustart startet den losgelösten Helfer erst ganz am Schluss
//! (`Shell::shutdown`), wenn der Profil-Lock frei ist.

use std::time::{Duration, Instant};

use gpui::{App, Context, Window};
use kadrell_core::SessionStatus;
use kadrell_core::settings::Ask;
use kadrell_i18n::trd;

use crate::Shell;
use crate::misc::confirm::Spec;
use crate::state::Run;

/// So lange warten die Prozesse nach dem Beenden-Signal, bevor sie hart beendet werden (`AttachManager.shutdown`).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Zustand des Beendens, ein Feld der Shell.
#[derive(Debug, Default)]
pub struct QuitState {
    /// Die Rückfrage ist beantwortet: ein weiteres Beenden fragt nicht noch einmal.
    pub confirmed: bool,
    /// Nach dem Beenden startet ein Helfer dieselbe App neu.
    pub relaunch: bool,
    /// Die Rückfrage wird gerade vorbereitet (Status wird frisch gelesen), damit ein zweites Kürzel nicht doppelt fragt.
    pending: bool,
}

/// Eine laufende Session für die Rückfrage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Running {
    pub title: String,
    pub status: SessionStatus,
}

/// Sessions, die gerade arbeiten oder auf eine Antwort warten (Beenden bricht ihre Arbeit ab).
pub fn busy_count(running: &[Running]) -> usize {
    running.iter().filter(|r| matches!(r.status, SessionStatus::Running | SessionStatus::Waiting)).count()
}

/// Titel der Rückfrage: ohne arbeitende Session die einfache Frage, sonst mit Warnung und Zahl.
pub fn shutdown_title(relaunch: bool, busy: usize) -> String {
    match (relaunch, busy) {
        (false, 0) => trd!("Kadrell beenden?"),
        (true, 0) => trd!("Kadrell neu starten?"),
        (false, n) => trd!("Kadrell beenden? %lld Session(s) arbeiten gerade!", n as i64),
        (true, n) => trd!("Kadrell neu starten? %lld Session(s) arbeiten gerade!", n as i64),
    }
}

/// Eine Zeile der Liste: `· <Titel>`, dazu `  ARBEITET` oder `  WARTET AUF ANTWORT`.
fn list_line(r: &Running) -> String {
    let mark = match r.status {
        SessionStatus::Running => trd!("  ARBEITET"),
        SessionStatus::Waiting => trd!("  WARTET AUF ANTWORT"),
        _ => String::new(),
    };
    format!("· {}{mark}", r.title)
}

/// Text der Rückfrage: was passiert, dann die Liste der Sessions.
pub fn shutdown_text(running: &[Running]) -> String {
    let info = trd!("%lld Claude-Prozess(e) werden sauber beendet. Laufende Arbeit bricht dabei ab. Die Konversationen bleiben erhalten und werden beim nächsten Start fortgesetzt.", running.len() as i64);
    let list: Vec<String> = running.iter().map(list_line).collect();
    format!("{info}\n\n{}", list.join("\n"))
}

/// Beschriftung der Haupttaste.
pub fn shutdown_button(relaunch: bool) -> String {
    if relaunch { trd!("Neu starten") } else { trd!("Beenden") }
}

/// Einmal-Aktion auf der Shell.
type ShellAction = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)>;

/// Führt `f` auf der Shell des Hauptfensters aus. `false`: es gibt (noch) keine Shell.
pub fn with_shell(cx: &mut App, f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)) -> bool {
    let Some(handle) = cx.windows().into_iter().find_map(|w| w.downcast::<Shell>()) else { return false };
    handle.update(cx, f).is_ok()
}

/// Beenden aus einer globalen Aktion (Strg+Q, Menü): mit Rückfrage, ohne Shell sofort.
pub fn request(cx: &mut App) {
    if !with_shell(cx, |shell, window, cx| shell.request_quit(window, cx)) {
        cx.quit();
    }
}

/// Neustart aus einer globalen Aktion.
pub fn request_restart(cx: &mut App) {
    with_shell(cx, |shell, window, cx| shell.restart(window, cx));
}

/// Signal (SIGTERM, SIGINT, SIGHUP): wie Beenden mit Rückfrage. Hängt das bestätigte Beenden, beendet ein weiteres
/// Signal sofort (`128 + Signal`). Ohne Shell (Signal beim Start) endet die App ohne Rückfrage.
pub fn on_signal(cx: &mut App, sig: i32) {
    let hanging = std::cell::Cell::new(false);
    let found = with_shell(cx, |shell, window, cx| {
        hanging.set(shell.quit.confirmed);
        if !shell.quit.confirmed {
            shell.request_quit(window, cx);
        }
    });
    if hanging.get() {
        std::process::exit(128 + sig);
    }
    if !found {
        cx.quit();
    }
}

impl Shell {
    /// ⌘Q, Menü, Tray, Signal: beenden, bei laufenden Prozessen erst nach der Rückfrage.
    pub(crate) fn request_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quit.confirmed || self.state.running_count() == 0 {
            cx.quit();
        } else {
            self.confirm_shutdown(false, window, cx);
        }
    }

    /// „Kadrell neu starten“: mit Prozessen dieselbe Rückfrage wie beim Beenden, ohne sofort.
    pub(crate) fn restart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.running_count() > 0 {
            return self.confirm_shutdown(true, window, cx);
        }
        self.quit.relaunch = true;
        self.quit.confirmed = true;
        cx.quit();
    }

    /// Fenster nach vorn, Status frisch lesen, dann fragen.
    fn confirm_shutdown(&mut self, relaunch: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.quit.pending || self.misc.confirm.is_some() {
            return;
        }
        self.show_main_window(window, cx);
        window.activate_window();
        self.quit.pending = true;
        let job = self.state.poll_job();
        cx.spawn_in(window, async move |this, cx| {
            let (change, _) = cx.background_executor().spawn(async move { job.run_counted() }).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.state.apply_registry(change);
                shell.quit.pending = false;
                shell.ask_shutdown(relaunch, window, cx);
            });
        })
        .detach();
    }

    /// Titel der laufenden Sessions mit ihrem Status.
    fn running_sessions(&self) -> Vec<Running> {
        let sessions = &self.state.store.sessions.sessions;
        sessions.iter().filter(|s| self.state.run(&s.id) == Run::Running).map(|s| Running { title: s.title(), status: s.status() }).collect()
    }

    fn ask_shutdown(&mut self, relaunch: bool, window: &mut Window, cx: &mut Context<Self>) {
        let running = self.running_sessions();
        let spec = Spec {
            title: shutdown_title(relaunch, busy_count(&running)),
            text: shutdown_text(&running),
            button: shutdown_button(relaunch),
            destructive: false,
            notice: false,
        };
        let action: ShellAction = Box::new(move |shell, window, cx| shell.finish_shutdown(relaunch, window, cx));
        self.ask_unless_off(Ask::Quit, spec, action, window, cx);
    }

    /// Fragt, es sei denn die Rückfrage ist in den Einstellungen abgeschaltet: dann läuft die Aktion sofort.
    pub(crate) fn ask_unless_off(&mut self, ask: Ask, spec: Spec, action: ShellAction, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.settings.ask_enabled(ask) {
            self.ask(spec, Some(action), window, cx);
        } else {
            action(self, window, cx);
        }
    }

    /// Nach der Bestätigung: Prozesse beenden, auf ihr Ende warten, dann die App beenden.
    fn finish_shutdown(&mut self, relaunch: bool, _window: &mut Window, cx: &mut Context<Self>) {
        self.quit.relaunch = relaunch;
        self.quit.confirmed = true;
        self.state.shutdown_all();
        cx.spawn(async move |this, cx| {
            let start = Instant::now();
            while start.elapsed() < SHUTDOWN_GRACE {
                let gone = this.update(cx, |shell, _| shell.state.pids().is_empty()).unwrap_or(true);
                if gone {
                    break;
                }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
            let _ = this.update(cx, |shell, cx| {
                shell.kill_stragglers();
                cx.quit();
            });
        })
        .detach();
    }

    /// Was nach der Wartezeit noch lebt, bekommt SIGKILL (Unix, an die ganze Gruppe) bzw. `taskkill /F`.
    fn kill_stragglers(&self) {
        for (pid, key) in self.state.pids() {
            eprintln!("kadrell: Prozess von {key} reagiert nicht, hartes Ende");
            kill_hard(pid);
        }
    }
}

/// `forkpty` macht den Prozess zum Gruppenleiter: das Signal geht an die ganze Gruppe, damit Kinder nicht verwaisen.
#[cfg(unix)]
fn kill_hard(pid: u32) {
    // SAFETY: `kill` mit pid und Signal hat keine weiteren Voraussetzungen.
    unsafe {
        if libc::kill(-(pid as libc::pid_t), libc::SIGKILL) != 0 {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
    }
}

#[cfg(windows)]
fn kill_hard(pid: u32) {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("taskkill");
    cmd.args(["/F", "/T", "/PID", &pid.to_string()]).creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let _ = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(title: &str, status: SessionStatus) -> Running {
        Running { title: title.into(), status }
    }

    #[test]
    fn busy_counts_working_and_waiting_sessions_only() {
        let all = [running("a", SessionStatus::Running), running("b", SessionStatus::Waiting), running("c", SessionStatus::Idle)];
        assert_eq!(busy_count(&all), 2);
        assert_eq!(busy_count(&all[2..]), 0);
        assert_eq!(busy_count(&[]), 0);
    }

    #[test]
    fn the_title_warns_only_when_sessions_are_working() {
        assert_eq!(shutdown_title(false, 0), "Kadrell beenden?");
        assert_eq!(shutdown_title(true, 0), "Kadrell neu starten?");
        assert_eq!(shutdown_title(false, 3), "Kadrell beenden? 3 Session(s) arbeiten gerade!");
        assert_eq!(shutdown_title(true, 1), "Kadrell neu starten? 1 Session(s) arbeiten gerade!");
    }

    #[test]
    fn the_text_lists_every_process_and_marks_the_busy_ones() {
        let text = shutdown_text(&[running("api", SessionStatus::Running), running("docs", SessionStatus::Waiting), running("ruhig", SessionStatus::Idle)]);
        assert!(text.starts_with("3 Claude-Prozess(e) werden sauber beendet."), "{text}");
        assert!(text.contains("fortgesetzt.\n\n· api  ARBEITET\n· docs  WARTET AUF ANTWORT\n· ruhig"), "{text}");
        assert!(text.ends_with("· ruhig"), "ein ruhiger Prozess trägt keine Marke: {text}");
    }

    #[test]
    fn the_button_names_the_action() {
        assert_eq!(shutdown_button(false), "Beenden");
        assert_eq!(shutdown_button(true), "Neu starten");
    }

    #[test]
    fn nothing_is_confirmed_at_the_start() {
        let state = QuitState::default();
        assert!(!state.confirmed && !state.relaunch && !state.pending);
    }
}
