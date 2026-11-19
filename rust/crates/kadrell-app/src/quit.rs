//! Beenden und Neustart (Swift: `applicationShouldTerminate`, `confirmShutdown`, `restart` im AppDelegate). Laufen
//! Claude-Prozesse, fragt jeder Weg ins Beenden zuerst nach: Strg+Q und Menü, Tray-Menü, Signale (SIGTERM, SIGINT,
//! SIGHUP) und „Kadrell neu starten“. Die Rückfrage ist ein Overlay, das eigentliche Beenden (`cx.quit`) folgt erst
//! nach der Bestätigung. Ohne laufende Prozesse oder mit abgeschalteter Rückfrage („Kadrell beenden, während Claude
//! läuft“) geht es sofort. `kadrell kill-server` fragt nie, wie `tmux kill-server`.
//!
//! Nach der Bestätigung beendet `Shell::shutdown` die Prozesse (mit Wartezeit, dann hart). Der Neustart startet den losgelösten Helfer erst ganz am Schluss
//! (`Shell::shutdown`), wenn der Profil-Lock frei ist.

use gpui::{App, Context, Window};

use crate::Shell;

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

/// Führt `f` auf der Shell des Hauptfensters aus. `false`: es gibt (noch) keine Shell.
pub fn with_shell(cx: &mut App, f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>)) -> bool {
    let Some(handle) = cx.windows().into_iter().find_map(|w| w.downcast::<Shell>()) else { return false };
    handle.update(cx, f).is_ok()
}

/// Beenden aus einer globalen Aktion (Strg+Q, Menü): mit Rückfrage, ohne Shell sofort.
///
/// `defer`: das Fenster steckt gerade im Dispatch der Aktion und lässt sich dann nicht ein zweites Mal ändern.
pub fn request(cx: &mut App) {
    cx.defer(|cx| {
        if !with_shell(cx, |shell, window, cx| shell.request_quit(window, cx)) {
            cx.quit();
        }
    });
}

/// Neustart aus einer globalen Aktion.
pub fn request_restart(cx: &mut App) {
    cx.defer(|cx| {
        with_shell(cx, |shell, window, cx| shell.restart(window, cx));
    });
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
        if self.quit.pending || self.groups_ui.confirm.is_some() {
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

    /// Die Rückfrage (`ui::groups::confirm::quit_spec`, mit „Nicht mehr fragen“ für `Ask::Quit`); bestätigt oder
    /// abgeschaltet läuft `finish_shutdown`.
    fn ask_shutdown(&mut self, relaunch: bool, window: &mut Window, cx: &mut Context<Self>) {
        let running = self.running_sessions();
        let spec = crate::ui::groups::confirm::quit_spec(&running, relaunch);
        self.confirm_or_run(spec, false, window, cx);
    }

    /// Nach der Bestätigung: beenden. `shutdown` (beim Verlassen der App) beendet die Prozesse, wartet bis zu
    /// `SHUTDOWN_TIMEOUT` und beendet übrige hart; beim Neustart startet danach der Helfer.
    pub(crate) fn finish_shutdown(&mut self, relaunch: bool, cx: &mut Context<Self>) {
        self.quit.relaunch = relaunch;
        self.quit.confirmed = true;
        cx.quit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_confirmed_at_the_start() {
        let state = QuitState::default();
        assert!(!state.confirmed && !state.relaunch && !state.pending);
    }
}
