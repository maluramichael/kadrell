//! Anbindung des Zustands an die Oberfläche: Kacheln starten und halten, Steuerkanal (Aufträge vom Server-Thread
//! auf dem GPUI-Thread ausführen), Statuspoll, Beenden. Der Zustand selbst steht in `state`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use gpui::{AppContext, Context, Window};

use kadrell_claude::cli::LaunchSpec;
use kadrell_control::exec::{self, Followup, Outcome};
use kadrell_control::protocol::{ControlRequest, ControlResponse};
use kadrell_control::{Service, SystemProcessTree};
use kadrell_term::{TermSpec, TerminalEvent, TerminalPane};

use crate::Shell;
use crate::state::launch::env_pairs;
use crate::state::{Run, Term};
use crate::ui::layout::Selection;
use crate::ui::model;

/// Ein Auftrag vom Server-Thread des Steuerkanals an den GPUI-Thread.
pub enum Job {
    Request { req: ControlRequest, peer: Option<u32>, reply: mpsc::Sender<Outcome> },
    /// Nachspiel von `send`: das Enter, nachdem der Text angekommen ist.
    Finish { followup: Followup, reply: mpsc::Sender<Outcome> },
}

/// So lange wartet der Server-Thread auf den GPUI-Thread, bevor er aufgibt.
const JOB_TIMEOUT: Duration = Duration::from_secs(30);

/// Der Dienst für `ControlServer::start`: gibt jede Anfrage an den GPUI-Thread und wartet auf die Antwort.
pub fn service(jobs: async_channel::Sender<Job>) -> Service {
    Arc::new(move |req, peer| {
        let Some(outcome) = ask(&jobs, |reply| Job::Request { req, peer, reply }) else {
            return ControlResponse::fail("Kadrell antwortet nicht");
        };
        if let Some(followup) = outcome.followup.clone() {
            std::thread::sleep(followup.delay);
            let _ = ask(&jobs, |reply| Job::Finish { followup, reply });
        }
        outcome.response
    })
}

fn ask(jobs: &async_channel::Sender<Job>, make: impl FnOnce(mpsc::Sender<Outcome>) -> Job) -> Option<Outcome> {
    let (tx, rx) = mpsc::channel();
    jobs.send_blocking(make(tx)).ok()?;
    rx.recv_timeout(JOB_TIMEOUT).ok()
}

impl Shell {
    // MARK: Ansicht aus dem Zustand

    /// Baut Baum und Auswahl neu aus dem Zustand. Positionen `(Gruppe, Session)` gelten nur bis zur nächsten
    /// Änderung, deshalb wird nach jeder Änderung neu gebaut.
    pub(crate) fn rebuild(&mut self) {
        self.groups = model::build(&self.state, &self.panes);
        let flat = model::flat_index(&self.groups);
        let at = |key: &str| model::position(&self.groups, key);
        let mut items: Vec<_> = self.state.selected.iter().filter_map(|k| at(k)).collect();
        items.sort_by_key(|id| flat.iter().position(|f| f == id));
        let focus = self.state.focused.as_deref().and_then(at).or_else(|| items.first().copied());
        let anchor = self.state.anchor.as_deref().and_then(at).or(focus);
        self.selected = focus;
        self.selection = match (focus, anchor) {
            (Some(focus), Some(anchor)) => Selection { items, focus, anchor },
            _ => Selection { items: Vec::new(), focus: (0, 0), anchor: (0, 0) },
        };
    }

    /// Schreibt die Auswahl der Ansicht in den Zustand und gleicht ab.
    pub(crate) fn commit_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let key = |id: (usize, usize)| self.groups[id.0].sessions[id.1].key.clone();
        let keys: Vec<String> = self.selection.items.iter().map(|id| key(*id)).collect();
        if let Some(&focus) = self.selection.items.iter().find(|id| **id == self.selection.focus) {
            let (focus, anchor) = (key(focus), key(self.selection.anchor));
            self.state.set_selection(keys, Some(focus));
            self.state.anchor = Some(anchor);
        } else {
            self.state.set_selection(keys, None);
        }
        self.resume_clicked_tile();
        self.sync(window, cx);
    }

    /// Klick auf eine beendete Kachel setzt sie fort („Kachel bleibt, Klick setzt fort“).
    fn resume_clicked_tile(&mut self) {
        let Some(key) = self.state.focused.clone() else { return };
        if matches!(self.state.run(&key), Run::Ended | Run::Stopped) {
            let _ = self.state.request_start(&key);
        }
    }

    /// Nach jeder Änderung des Zustands: vorgemerkte Prozesse starten, Ansicht neu bauen, Tastatur an die
    /// Fokus-Kachel (solange kein Dialog offen ist), neu zeichnen.
    pub(crate) fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reconcile(window, cx);
        self.emit_ext_events();
        self.rebuild();
        self.refresh_auto();
        if self.overlay.is_none() && self.palette.is_none() && self.rename.is_none() && self.settings.is_none() && self.agent_pick.is_none() && self.remote.is_none() && !self.ext_ui.keeps_focus(window, cx) && !self.usage.overlay_open() && !self.misc.overlay_open() && !self.ws.keeps_focus() {
            let focus = self.selected.and_then(|(g, s)| self.groups[g].sessions[s].pane.as_ref().map(|p| p.read(cx).focus.clone()));
            if let Some(focus) = focus {
                focus.focus(window, cx);
            }
        }
        self.refresh_tray(window, cx);
        self.run_pending_overlay(window, cx);
        cx.notify();
    }

    /// Startet, was der Zustand vorgemerkt hat, und räumt Kacheln gelöschter Sessions weg.
    fn reconcile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (key, spec) in self.state.take_starts() {
            match spec {
                Ok(spec) => self.start_pane(key, spec, window, cx),
                Err(message) => eprintln!("kadrell: {message}"),
            }
        }
        self.state.apply_scrollback();
        self.arm_wave_timer(window, cx);
        let state = &self.state;
        self.panes.retain(|key, _| state.session(key).is_some() && state.mounted(key));
    }

    /// Wartet noch eine Welle des Lazy Starts (4 Kacheln je 500 ms), kommt die Oberfläche dann wieder vorbei.
    fn arm_wave_timer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(wait) = self.state.arm_wave_timer() else { return };
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(wait).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.state.wave_timer_fired();
                shell.sync(window, cx);
            });
        })
        .detach();
    }

    fn start_pane(&mut self, key: String, spec: LaunchSpec, window: &mut Window, cx: &mut Context<Self>) {
        let term = TermSpec { program: spec.program, args: spec.args, cwd: PathBuf::from(&spec.cwd), env: env_pairs(&spec.env), scrollback: Some(self.state.scrollback()) };
        let parts = match kadrell_term::start_pty(term) {
            Ok(parts) => parts,
            Err(e) => {
                self.state.mark_ended(&key);
                self.state.error = Some(format!("Start fehlgeschlagen: {e}"));
                eprintln!("kadrell: Start von {key} fehlgeschlagen: {e}");
                return;
            }
        };
        let handle: Rc<dyn Term> = Rc::new(parts.handle());
        let pane = cx.new(|cx| TerminalPane::from_parts(parts, cx));
        let exited_key = key.clone();
        cx.subscribe_in(&pane, window, move |shell, _, ev: &TerminalEvent, window, cx| match ev {
            TerminalEvent::Exited => shell.on_exited(&exited_key, window, cx),
            TerminalEvent::Link(link) => shell.on_link(&exited_key, link, window, cx),
        })
        .detach();
        self.panes.insert(key.clone(), pane);
        self.state.attach(&key, handle);
    }

    fn on_exited(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.on_exit(key) {
            self.sync(window, cx);
        }
    }

    // MARK: Steuerkanal

    /// Führt die Aufträge des Servers auf dem GPUI-Thread aus. Die Antwort geht erst nach dem Abgleich zurück: ein
    /// `kadrell new`, dem sofort ein `send` folgt, findet dann schon das Terminal.
    pub(crate) fn start_jobs(&mut self, jobs: async_channel::Receiver<Job>, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(job) = jobs.recv().await {
                if this.update_in(cx, |shell, window, cx| shell.run_job(job, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn run_job(&mut self, job: Job, window: &mut Window, cx: &mut Context<Self>) {
        let (outcome, reply) = match job {
            Job::Request { req, peer, reply } => {
                let outcome = self.window_command(&req, window, cx).unwrap_or_else(|| exec::handle(&mut self.state, &SystemProcessTree, &req, peer));
                (outcome, reply)
            }
            Job::Finish { followup, reply } => {
                exec::finish(&mut self.state, &followup);
                (Outcome { response: ControlResponse::ok(""), followup: None }, reply)
            }
        };
        self.sync(window, cx);
        let _ = reply.send(outcome);
        if self.state.quit_requested {
            self.quit_soon(cx);
        }
    }

    /// `kill-server`: nach der Antwort beenden.
    fn quit_soon(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(Duration::from_millis(300)).await;
            cx.update(|cx| cx.quit());
        })
        .detach();
    }

    // MARK: Start und Statuspoll

    /// Startreihenfolge wie `AppDelegate.boot`: `claude` prüfen, erster Poll, `app.ready`, Verlauf aufwärmen,
    /// Übernahme anbieten, erst danach das regelmäßige Pollen. Im Hintergrund (Fenster versteckt oder nicht aktiv)
    /// pollt es seltener, beim Zurückkommen sofort (`SessionRegistry.setBackground`).
    pub(crate) fn start_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let Ok(input) = this.update(cx, |shell, _| shell.check_input()) else { return };
            let (health, env) = cx.background_executor().spawn(async move { crate::cli_wire::run_check(input) }).await;
            if this.update(cx, |shell, cx| shell.apply_cli_health(health, env, cx)).is_err() {
                return;
            }
            if crate::cli_wire::dev_loading() {
                cx.background_executor().timer(Duration::from_secs(60)).await;
            }
            if !poll_once(&this, cx).await {
                return;
            }
            let _ = this.update_in(cx, |shell, window, cx| shell.after_first_poll(window, cx));
            poll_loop(&this, cx).await;
        })
        .detach();
    }

    /// Nach dem ersten Poll: `app.ready`, Verlaufssuche vorwärmen, Hintergrund-Sessions zur Übernahme anbieten.
    fn after_first_poll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ext.emit("app.ready", serde_json::json!({}));
        self.refresh_history(cx);
        self.start_adopt(window, cx);
    }

    fn apply_poll(&mut self, change: kadrell_claude::registry::Change, messages: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.state.count_stat(kadrell_core::stats::StatKey::Messages, messages as i64);
        self.state.apply_registry(change);
        self.sync(window, cx);
    }

    // MARK: Beenden

    /// Beim Beenden (Swift: `applicationWillTerminate`): Prozesse enden, der Kanal verschwindet, Extensions und
    /// lokale Modelle enden, ein Wegwerfprofil räumt sich weg. Der Beenden-Marker kommt vor dem Schließen des Kanals,
    /// aber nicht beim Neustart, nicht im Wegwerfprofil. Ganz zuletzt startet beim Neustart der losgelöste Helfer, der
    /// wartet, bis dieser Prozess weg und der Profil-Lock frei ist.
    pub(crate) fn shutdown(&mut self) {
        if !self.quit.relaunch && !self.state.profile.temporary {
            kadrell_core::profile::mark_quit(&self.state.profile.dir, crate::state::now_secs());
        }
        let pids = self.state.shutdown_all();
        kadrell_term::process::terminate_all(&pids, kadrell_term::process::SHUTDOWN_TIMEOUT);
        self.tray.close();
        self.server = None;
        self.ext.shutdown_all();
        self.state.profile.clean_up();
        if self.quit.relaunch {
            crate::instance::relaunch_after_exit(self.state.profile.name.as_deref());
        }
    }
}

/// Ein Poll auf dem Hintergrund-Executor, Ergebnis in den Zustand. `false`: die Shell gibt es nicht mehr.
async fn poll_once(this: &gpui::WeakEntity<Shell>, cx: &mut gpui::AsyncWindowContext) -> bool {
    let Ok(job) = this.update(cx, |shell, _| shell.state.poll_job()) else { return false };
    let (change, messages) = cx.background_executor().spawn(async move { job.run_counted() }).await;
    this.update_in(cx, |shell, window, cx| shell.apply_poll(change, messages, window, cx)).is_ok()
}

/// Das regelmäßige Pollen: alle halbe Sekunde nachsehen, ob der Takt (2 s, im Hintergrund 5 s) abgelaufen ist oder
/// das Fenster eben aus dem Hintergrund kam.
async fn poll_loop(this: &gpui::WeakEntity<Shell>, cx: &mut gpui::AsyncWindowContext) {
    let mut last = std::time::Instant::now();
    loop {
        cx.background_executor().timer(POLL_TICK).await;
        let elapsed = last.elapsed().as_secs_f64();
        let Ok(due) = this.update(cx, |shell, _| shell.state.poll_due(elapsed)) else { return };
        if due {
            if !poll_once(this, cx).await {
                return;
            }
            last = std::time::Instant::now();
        }
    }
}

/// So oft prüft die Pollschleife, ob ein Poll fällig ist.
const POLL_TICK: Duration = Duration::from_millis(500);

/// Zeilen für `KADRELL_DEV_*`: Schlüssel der Sessions an den Positionen `g,s;g,s`.
pub fn keys_at(groups: &[model::Group], spec: &str) -> Vec<String> {
    let parse = |p: &str| -> Option<(usize, usize)> {
        let (g, s) = p.split_once(',')?;
        Some((g.trim().parse().ok()?, s.trim().parse().ok()?))
    };
    spec.split(';').filter_map(parse).filter_map(|(g, s)| groups.get(g)?.sessions.get(s).map(|x| x.key.clone())).collect()
}

#[allow(dead_code)]
type PaneMap = HashMap<String, gpui::Entity<TerminalPane>>;
