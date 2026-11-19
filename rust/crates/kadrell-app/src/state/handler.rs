//! Der Steuerkanal auf dem Zustand der App: `kadrell <befehl>` ruft diese Handgriffe auf (`ControlHandler`). Alle
//! Ziele kommen schon aufgelöst und freigegeben an. Was einen Prozess starten muss, merkt der Zustand nur vor, die
//! Oberfläche startet beim Abgleich nach der Anfrage.

use kadrell_claude::hook::HookReport;
use kadrell_claude::transcript;
use kadrell_control::command::{ControlError, Result, Status};
use kadrell_control::{Capture, ControlHandler, LayoutMode, NewRequest, ResumeBlocker, RunState, SshHosts};
use kadrell_core::{Group, Session, Store};

use super::tiles::Run;
use super::{AppState, now_ms};

impl ControlHandler for AppState {
    fn ready(&self) -> bool {
        self.ready
    }

    fn store(&self) -> &Store {
        &self.store
    }

    fn tile_pids(&self) -> Vec<(String, u32)> {
        self.pids().into_iter().map(|(pid, key)| (key, pid)).collect()
    }

    fn focused(&self) -> Option<String> {
        self.cur_focus()
    }

    fn selected(&self) -> Vec<String> {
        self.cur_selected()
    }

    fn layout(&self) -> LayoutMode {
        self.cur_layout()
    }

    fn others_allowed(&self) -> bool {
        self.settings.control_other_sessions()
    }

    fn run_state(&self, key: &str) -> RunState {
        match self.run(key) {
            Run::Running => RunState::Running,
            Run::Ended => RunState::Ended,
            Run::NotStarted | Run::Stopped => RunState::Stopped,
        }
    }

    fn home(&self) -> std::path::PathBuf {
        self.paths.home.clone()
    }

    fn ssh_hosts(&self) -> SshHosts {
        self.ssh_host_list()
    }

    fn resume_blocker(&self, session_id: &str) -> Option<ResumeBlocker> {
        if self.store.sessions.sessions.iter().any(|s| s.session_id == session_id || s.id == session_id) {
            return Some(ResumeBlocker::InKadrell);
        }
        transcript::path(session_id, &self.env.config_dir()).is_none().then_some(ResumeBlocker::NoTranscript)
    }

    /// Pi ist installiert und unter „Pi-Sessions“ ist ein Modell (oder Pis Standard) gewählt.
    fn pi_available(&self) -> bool {
        self.pi_model().is_some()
    }

    fn bracketed_paste(&self, key: &str) -> Option<bool> {
        self.term(key).map(|t| t.bracketed_paste())
    }

    fn capture(&self, key: &str, all: bool) -> Capture {
        match self.term(key) {
            Some(t) if all => Capture::Buffer(t.buffer_text()),
            Some(t) => Capture::Rows(t.visible_rows()),
            None => Capture::Stopped(self.last_rows(key)),
        }
    }

    fn add_group(&mut self, group: Group) {
        AppState::add_group(self, group);
    }

    fn update_group(&mut self, group: Group) {
        self.store.groups.update(group);
    }

    fn move_session(&mut self, key: &str, group_id: &str) {
        self.move_sessions(&[key.to_string()], group_id);
    }

    fn rename(&mut self, key: &str, name: &str) {
        self.rename_session(key, name);
    }

    fn new_session(&mut self, request: NewRequest) -> Result<String> {
        if let Some(host) = &request.host {
            return Ok(self.new_remote_session(host, request.tmux.as_deref(), request.show));
        }
        if request.pi {
            let model = self.pi_model().ok_or_else(|| ControlError::new("--pi braucht pi und ein Modell unter Einstellungen › Modelle für Funktionen › Pi-Sessions"))?;
            return Ok(self.new_pi_session(&request.cwd, request.group.as_deref(), request.show, request.prompt, &model));
        }
        let mut session = match &request.resume {
            Some(id) => Session::new(id, &request.cwd, now_ms(), id, ""),
            None => Session::fresh(&request.cwd, now_ms()),
        };
        session.claude_model = request.model.clone();
        session.claude_effort = request.effort.clone();
        let key = session.id.clone();
        if !self.add_session(session, request.group.as_deref(), request.show, request.prompt) {
            return Err(ControlError::new(format!("{key} ist schon in Kadrell")));
        }
        Ok(key)
    }

    fn select(&mut self, keys: &[String], add: bool) {
        self.cur_select(keys, add);
    }

    fn focus(&mut self, key: &str) {
        self.cur_focus_key(key);
    }

    fn set_layout(&mut self, mode: LayoutMode) {
        self.cur_set_layout(mode);
    }

    fn toggle_zoom(&mut self) {
        self.zoom = !self.zoom;
    }

    fn stop(&mut self, key: &str) {
        self.stop_tile(key);
    }

    fn resume(&mut self, key: &str) -> Result<()> {
        self.request_start(key).map_err(ControlError::new)
    }

    fn kill(&mut self, key: &str) {
        self.remove_sessions(&[key.to_string()]);
    }

    fn kill_group(&mut self, group_id: &str) {
        self.close_group(group_id);
    }

    fn terminal_write(&mut self, key: &str, data: &str) {
        if let Some(t) = self.term(key) {
            t.write(data.as_bytes());
        }
    }

    fn report_status(&mut self, key: &str, status: &Status) {
        let report = HookReport {
            state: status.state.clone(),
            session_id: status.session_id.clone(),
            title: status.title.clone(),
            waiting_for: status.waiting_for.clone(),
            message: status.message.clone(),
            first_prompt: status.first_prompt.clone(),
        };
        self.report(key, &report);
    }

    fn kill_server(&mut self) {
        self.quit_requested = true;
    }
}
