//! Führt `kadrell <befehl>` aus wie `AppDelegate+Control.swift`: Befehl parsen, Ziele auflösen (`-t`, Aufrufer,
//! Fokus), die Freigabe „Sessions dürfen andere Sessions steuern“ prüfen und dann den passenden Handgriff der App
//! ([`ControlHandler`]) aufrufen. Alle Fehler werden zur Antwort mit Status 1 und der Meldung der Mac-App.

use crate::caller::{ProcessTree, caller_tile_in};
use crate::command::{self, Command, ControlError, NewSession, Result, Status};
use crate::handler::{Capture, ControlHandler, NewRequest, ResumeBlocker, RunState};
use crate::list;
use crate::protocol::{ControlRequest, ControlResponse};
use kadrell_core::targets::{self, Target};
use kadrell_core::{Group, Session};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Pause zwischen eingefügtem Text und Enter: sonst hält die Eingabe das Enter für einen Zeilenumbruch im
/// eingefügten Text.
pub const ENTER_DELAY: Duration = Duration::from_millis(150);

const POLICY_HINT: &str = "Einstellung „Sessions dürfen andere Sessions steuern“";

/// Was nach der Antwort noch zu tun ist: `send` schickt sein Enter erst nach [`ENTER_DELAY`] und antwortet erst
/// danach, damit ein direkt folgendes `kadrell send` sein Enter nicht vor diesem startet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Followup {
    pub delay: Duration,
    pub key: String,
    pub data: String,
}

/// Ergebnis von [`execute`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub response: ControlResponse,
    pub followup: Option<Followup>,
}

impl Outcome {
    fn done(response: ControlResponse) -> Outcome {
        Outcome { response, followup: None }
    }
}

/// Wie [`execute`], aber mit dem Aufrufer aus der pid des Clients: ein mitgeschicktes `caller` wird verworfen
/// und durch die Kachel aus dem Prozessbaum ersetzt (`None`: kein Terminal der Kachel, also von außen). Das ist
/// der Weg jeder Anfrage über den Kanal.
pub fn handle(h: &mut dyn ControlHandler, tree: &dyn ProcessTree, req: &ControlRequest, peer_pid: Option<u32>) -> Outcome {
    let caller = peer_pid.and_then(|pid| caller_tile_in(tree, pid, &h.tile_pids()));
    execute(h, &ControlRequest { caller, ..req.clone() })
}

/// Parsen und ausführen, jeder Fehler wird zur Antwort mit Status 1. `req.caller` gilt als vertrauenswürdig: der
/// Weg für Aufrufe aus der App selbst (`kadrell.run` aus Extensions).
pub fn execute(h: &mut dyn ControlHandler, req: &ControlRequest) -> Outcome {
    if !h.ready() {
        return Outcome::done(ControlResponse::starting());
    }
    let mut ctx = Ctx { h, req };
    match command::parse(&req.argv).and_then(|cmd| ctx.run(cmd)) {
        Ok(outcome) => outcome,
        Err(e) => Outcome::done(ControlResponse::fail(&e.0)),
    }
}

/// Erledigt das Nachspiel (Enter von `send`).
pub fn finish(h: &mut dyn ControlHandler, followup: &Followup) {
    h.terminal_write(&followup.key, &followup.data);
}

/// Alles in einem Thread: wie [`handle`], wartet das Nachspiel ab und gibt die Antwort zurück.
pub fn handle_blocking(h: &mut dyn ControlHandler, tree: &dyn ProcessTree, req: &ControlRequest, peer_pid: Option<u32>) -> ControlResponse {
    let outcome = handle(h, tree, req, peer_pid);
    if let Some(f) = &outcome.followup {
        std::thread::sleep(f.delay);
        finish(h, f);
    }
    outcome.response
}

/// Verbindet einen geteilten [`ControlHandler`] mit dem Server: eine Anfrage nach der anderen, der Handler ist
/// nur während der Ausführung gesperrt, nicht während des Wartens auf das Enter.
pub struct Controller<H> {
    handler: Arc<Mutex<H>>,
    tree: Box<dyn ProcessTree + Send + Sync>,
}

impl<H: ControlHandler> Controller<H> {
    /// Mit dem Prozessbaum des Systems.
    pub fn new(handler: Arc<Mutex<H>>) -> Controller<H> {
        Controller::with_tree(handler, Box::new(crate::caller::SystemProcessTree))
    }

    pub fn with_tree(handler: Arc<Mutex<H>>, tree: Box<dyn ProcessTree + Send + Sync>) -> Controller<H> {
        Controller { handler, tree }
    }

    /// Eine Anfrage beantworten. Blockiert höchstens für die Dauer des Handlers plus das Warten auf das Enter.
    pub fn call(&self, req: &ControlRequest, peer_pid: Option<u32>) -> ControlResponse {
        let outcome = {
            let mut h = self.handler.lock().unwrap_or_else(PoisonError::into_inner);
            handle(&mut *h, self.tree.as_ref(), req, peer_pid)
        };
        if let Some(f) = &outcome.followup {
            std::thread::sleep(f.delay);
            finish(&mut *self.handler.lock().unwrap_or_else(PoisonError::into_inner), f);
        }
        outcome.response
    }
}

impl<H: ControlHandler + Send + 'static> Controller<H> {
    /// Als Dienst für [`crate::server::ControlServer::start`].
    pub fn into_service(self) -> crate::server::Service {
        Arc::new(move |req, peer| self.call(&req, peer))
    }
}

struct Ctx<'a> {
    h: &'a mut dyn ControlHandler,
    req: &'a ControlRequest,
}

impl Ctx<'_> {
    fn run(&mut self, cmd: Command) -> Result<Outcome> {
        match cmd {
            Command::Help => Ok(ok(command::USAGE)),
            Command::List { json } => Ok(ok(list::list_output(&*self.h, json))),
            Command::SshHosts { json } => Ok(ok(list::ssh_hosts_output(&self.h.ssh_hosts(), json))),
            Command::NewGroup { dir, name, color } => self.new_group(&dir, name, color).map(ok),
            Command::New(n) => self.new_session(n).map(ok),
            Command::Capture { target, all } => self.capture(target.as_deref(), all).map(ok),
            Command::Send { target, text, enter, keys } => self.send(target.as_deref(), &text, enter, keys),
            other => self.perform(other).map(|()| ok("")),
        }
    }

    /// Befehle ohne Ausgabe.
    fn perform(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::Select { target, add } => self.select(target.as_deref(), add),
            Command::Layout(mode) => {
                self.h.set_layout(mode);
                Ok(())
            }
            Command::Zoom { target } => self.zoom(target.as_deref()),
            Command::Rename { target, name } => {
                let s = self.session(target.as_deref())?;
                self.h.rename(&s.id, &name);
                Ok(())
            }
            Command::Move { target, group } => self.move_session(target.as_deref(), &group),
            Command::SetGroup { target, name, color, favorite } => self.set_group(target.as_deref(), name, color, favorite),
            Command::Status(status) => self.status(&status),
            Command::KillServer => {
                self.h.kill_server();
                Ok(())
            }
            other => self.perform_process(other),
        }
    }

    /// Befehle, die einen Prozess anfassen.
    fn perform_process(&mut self, cmd: Command) -> Result<()> {
        match cmd {
            Command::Stop { target } => self.stop(target.as_deref()),
            Command::Resume { target } => {
                let s = self.session(target.as_deref())?;
                self.h.resume(&s.id)
            }
            Command::Kill { target } => {
                let s = self.session(target.as_deref())?;
                self.h.kill(&s.id);
                Ok(())
            }
            Command::KillGroup { target } => {
                let g = self.group(target.as_deref(), true)?;
                self.h.kill_group(&g.id);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    // MARK: Ziele

    fn caller(&self) -> Option<&str> {
        self.req.caller.as_deref()
    }

    /// Ziel einer Session-Aktion, aus einer Kachel heraus nur im erlaubten Bereich.
    fn session(&self, t: Option<&str>) -> Result<Session> {
        let store = self.h.store();
        let s = targets::resolve_session(t, &store.sessions.sessions, self.caller(), self.h.focused().as_deref())?;
        if !targets::allowed_session(&s.id, &store.groups.groups, self.caller(), self.h.others_allowed()) {
            return Err(ControlError::new(format!("„{}“ gehört nicht zur Gruppe dieser Session ({POLICY_HINT})", s.title())));
        }
        Ok(s.clone())
    }

    fn group(&self, t: Option<&str>, restricted: bool) -> Result<Group> {
        let store = self.h.store();
        let g = targets::resolve_group(t, &store.groups.groups, &store.sessions.sessions, self.caller(), self.h.focused().as_deref())?;
        if restricted && !targets::allowed_group(g, self.caller(), self.h.others_allowed()) {
            return Err(ControlError::new(format!("Gruppe „{}“ ist nicht die dieser Session ({POLICY_HINT})", g.name)));
        }
        Ok(g.clone())
    }

    fn existing_dir(&self, p: &str) -> Result<String> {
        let path = targets::resolve_path(p, &self.req.cwd, &self.h.home());
        if Path::new(&path).is_dir() { Ok(path) } else { Err(ControlError::new(format!("kein Ordner: {path}"))) }
    }

    // MARK: Befehle

    fn select(&mut self, t: Option<&str>, add: bool) -> Result<()> {
        let store = self.h.store();
        let keys = match targets::resolve_session_or_group(t, &store.groups.groups, &store.sessions.sessions, self.caller(), self.h.focused().as_deref())? {
            Target::Session(s) => vec![s.id.clone()],
            Target::Group(g) => g.session_ids.clone(),
        };
        self.h.select(&keys, add);
        Ok(())
    }

    fn zoom(&mut self, t: Option<&str>) -> Result<()> {
        if t.is_some() {
            let s = self.session(t)?;
            if !self.h.selected().contains(&s.id) {
                self.h.select(std::slice::from_ref(&s.id), true);
            }
            self.h.focus(&s.id);
        }
        if self.h.focused().is_none() {
            return Err(ControlError::new("keine Kachel fokussiert"));
        }
        self.h.toggle_zoom();
        Ok(())
    }

    fn move_session(&mut self, t: Option<&str>, group: &str) -> Result<()> {
        let s = self.session(t)?;
        let g = self.group(Some(group), true)?;
        self.h.move_session(&s.id, &g.id);
        Ok(())
    }

    fn set_group(&mut self, t: Option<&str>, name: Option<String>, color: Option<String>, favorite: Option<bool>) -> Result<()> {
        let mut g = self.group(t, true)?;
        g.name = name.unwrap_or(g.name);
        g.color = color.unwrap_or(g.color);
        g.favorite = favorite.unwrap_or(g.favorite);
        self.h.update_group(g);
        Ok(())
    }

    fn stop(&mut self, t: Option<&str>) -> Result<()> {
        let s = self.session(t)?;
        if self.h.run_state(&s.id) != RunState::Running {
            return Err(ControlError::new(format!("„{}“ läuft nicht", s.title())));
        }
        self.h.stop(&s.id);
        Ok(())
    }

    /// Ohne -t meldet sich die aufrufende Kachel selbst; von außen muss das Ziel genannt sein.
    fn status(&mut self, st: &Status) -> Result<()> {
        if st.target.is_none() && self.caller().is_none() {
            return Err(ControlError::new("status braucht -t, wenn es nicht aus einer Session kommt"));
        }
        let s = self.session(st.target.as_deref())?;
        self.h.report_status(&s.id, st);
        Ok(())
    }

    /// Favorit, sonst räumt der Abgleich die leere Gruppe beim nächsten Mal wieder weg.
    fn new_group(&mut self, dir: &str, name: Option<String>, color: Option<String>) -> Result<String> {
        let cwd = self.existing_dir(dir)?;
        let mut g = self.h.store().groups.make_group(&cwd, name.as_deref());
        if let Some(color) = color {
            g.color = color;
        }
        g.favorite = true;
        let id = g.id.clone();
        self.h.add_group(g);
        Ok(id)
    }

    /// Text geht als Einfügen (bracketed paste), wenn das Programm es eingeschaltet hat; Enter kommt getrennt
    /// hinterher (siehe [`Followup`]).
    fn send(&mut self, t: Option<&str>, text: &str, enter: bool, keys: bool) -> Result<Outcome> {
        let s = self.session(t)?;
        let Some(paste) = self.h.bracketed_paste(&s.id) else {
            return Err(ControlError::new(format!("„{}“ läuft nicht, erst kadrell resume -t {}", s.title(), s.id.chars().take(8).collect::<String>())));
        };
        if keys {
            self.h.terminal_write(&s.id, &command::keys_to_bytes(text));
            return Ok(Outcome::done(ControlResponse::ok("")));
        }
        let clean = text.replace("\u{1b}[201~", "");
        let data = if paste { format!("\u{1b}[200~{clean}\u{1b}[201~") } else { clean };
        self.h.terminal_write(&s.id, &data);
        let followup = enter.then(|| Followup { delay: ENTER_DELAY, key: s.id.clone(), data: "\r".into() });
        Ok(Outcome { response: ControlResponse::ok(""), followup })
    }

    /// Sichtbarer Bildschirm ohne Leerzeichen am Zeilenende und ohne leere Zeilen am Schluss; `--all` und gestoppte
    /// Sessions geben ihren Text unverändert zurück.
    fn capture(&self, t: Option<&str>, all: bool) -> Result<String> {
        let s = self.session(t)?;
        Ok(match self.h.capture(&s.id, all) {
            Capture::Rows(rows) => trim_rows(rows),
            Capture::Buffer(text) => text,
            Capture::Stopped(lines) => lines.join("\n"),
        })
    }

    /// Ohne -t und -c landet die Session bei der aufrufenden: gleiche Gruppe, gleicher Ordner (wie tmux new-window).
    /// Mit --host stattdessen eine Remote-Kachel per ssh mit eigener Gruppe je Host.
    fn new_session(&mut self, n: NewSession) -> Result<String> {
        let request = match n.host.clone() {
            Some(host) => NewRequest { host: Some(host), tmux: n.tmux.clone(), show: !n.detached, ..NewRequest::default() },
            None => self.local_request(&n)?,
        };
        let key = self.h.new_session(request)?;
        if let Some(name) = &n.name {
            self.h.rename(&key, name);
        }
        Ok(key)
    }

    fn local_request(&self, n: &NewSession) -> Result<NewRequest> {
        if let Some(resume) = &n.resume {
            self.check_resumable(resume)?;
        }
        if n.pi && !self.h.pi_available() {
            return Err(ControlError::new("--pi braucht pi und ein Modell unter Einstellungen › Modelle für Funktionen › Pi-Sessions"));
        }
        let (group, cwd) = self.new_target(n)?;
        Ok(NewRequest {
            group: group.map(|g| g.id),
            cwd,
            show: !n.detached,
            prompt: n.prompt.clone(),
            resume: n.resume.clone(),
            pi: n.pi,
            model: n.model.clone(),
            effort: n.effort.clone(),
            ..NewRequest::default()
        })
    }

    /// Gruppe und Ordner der neuen Session: `-c` geht vor, dann der Ordner der aufrufenden Session (nur ohne `-t`),
    /// dann der der Gruppe, zuletzt der Ordner des Aufrufs.
    fn new_target(&self, n: &NewSession) -> Result<(Option<Group>, String)> {
        let mut group = n.target.as_deref().map(|t| self.group(Some(t), false)).transpose()?;
        let store = self.h.store();
        let caller = self.caller().and_then(|c| store.sessions.get(c));
        if let (None, None, Some(caller)) = (&n.target, &n.dir, caller) {
            group = store.groups.group_for_session(&caller.id).cloned();
        }
        let cwd = match (&n.dir, &n.target, caller, &group) {
            (Some(dir), ..) => self.existing_dir(dir)?,
            (None, None, Some(caller), _) => caller.cwd.clone(),
            (None, _, _, Some(g)) => g.cwd.clone(),
            _ => self.existing_dir(&self.req.cwd)?,
        };
        Ok((group, cwd))
    }

    fn check_resumable(&self, session_id: &str) -> Result<()> {
        match self.h.resume_blocker(session_id) {
            Some(ResumeBlocker::InKadrell) => Err(ControlError::new(format!("{session_id} ist schon in Kadrell"))),
            Some(ResumeBlocker::NoTranscript) => Err(ControlError::new(format!("kein Transcript für {session_id}"))),
            Some(ResumeBlocker::Running(pid)) => Err(ControlError::new(format!("{session_id} läuft noch (pid {pid}), erst dort beenden"))),
            None => Ok(()),
        }
    }
}

fn ok(out: impl Into<String>) -> Outcome {
    Outcome::done(ControlResponse::ok(out))
}

fn trim_rows(rows: Vec<String>) -> String {
    let mut rows: Vec<&str> = rows.iter().map(|r| r.trim_end()).collect();
    while rows.last().is_some_and(|r| r.is_empty()) {
        rows.pop();
    }
    rows.join("\n")
}

#[cfg(test)]
mod tests;
