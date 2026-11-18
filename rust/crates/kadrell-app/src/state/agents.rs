//! Pi-Sessions und Remote-Kacheln im Zustand: neue Sessions anlegen, das Startkommando bauen (`take_starts` ruft
//! [`AppState::start_spec`]) und alles, was vor dem Start im Hintergrund laufen muss (llama-server hochfahren, per ssh
//! nach tmux fragen). Die Oberfläche startet beim Abgleich nach der Anfrage; was noch vorbereitet wird, bleibt vorgemerkt.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use kadrell_claude::cli::{LaunchSpec, tile_env};
use kadrell_claude::pi::{LocalModelRef, LocalProviderKind, PiModel};
use kadrell_control::SshHosts;
use kadrell_core::Session;
use kadrell_i18n::trd;
use serde_json::json;

use super::attention::patch_setting;
use super::{AppState, launch, now_ms};
use crate::models::pi::{self as pi_launch, PiStart};
use crate::remote;

/// Ergebnis einer Vorbereitung im Hintergrund.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prepared {
    /// Pi: der Anbieter ist bereit (oder ließ sich nicht starten, Pi zeigt den Fehler dann selbst).
    Pi,
    /// Remote: gibt es auf dem Host tmux?
    Remote { has_tmux: bool },
}

/// Vorbereitungen je Session-Schlüssel: `None` läuft noch, `Some` ist fertig und wartet auf den Start.
pub type Preparations = Arc<Mutex<HashMap<String, Option<Prepared>>>>;

/// Antwort von [`AppState::start_spec`].
pub enum Start {
    /// Das Kommando (oder der Grund, warum es keins gibt).
    Ready(Result<LaunchSpec, String>),
    /// Die Vorbereitung läuft noch: später wieder fragen.
    Pending,
}

impl AppState {
    // MARK: Pi

    /// Pi, wenn es installiert ist und unter „Modelle für Funktionen › Pi-Sessions“ ein Modell oder Pis Standard gewählt
    /// ist (Swift: `piSessionModel`). `None`: Pi steht nicht zur Wahl.
    pub fn pi_model(&self) -> Option<PiModel> {
        self.pi_binary()?;
        kadrell_models::LocalSettings::from_settings(&self.settings).pi_model()
    }

    pub fn pi_binary(&self) -> Option<String> {
        pi_launch::locate(&self.env.base.environment, &self.home(), &|p| std::path::Path::new(p).is_file())
    }

    /// Neue Pi-Session im Ordner `cwd`, mit dem Modell der Einstellung (Pis Standard ohne lokales Modell).
    pub fn new_pi_session(&mut self, cwd: &str, group: Option<&str>, show: bool, prompt: Option<String>, model: &PiModel) -> String {
        let id = Session::new_pi_id();
        let mut session = Session::new(&id, cwd, now_ms(), &id, "");
        session.model = model.local().map(LocalModelRef::string);
        self.add_session(session, group, show, prompt);
        id
    }

    // MARK: Remote

    /// Zuletzt verbundene Hosts (neueste vorn).
    pub fn recent_hosts(&self) -> Vec<String> {
        self.settings.strings_or(remote::RECENT_KEY)
    }

    /// Hosts für `kadrell ssh-hosts`: alle aus `~/.ssh/config` (mit `Include`) und die zuletzt verbundenen.
    pub fn ssh_host_list(&self) -> SshHosts {
        let home = self.home();
        let hosts = kadrell_claude::ssh_config::hosts(&kadrell_claude::ssh_config::default_config(&home), &home);
        SshHosts { hosts, recent: self.recent_hosts() }
    }

    /// Remote-Kachel: Gruppe je Host (macht der Speicher beim Anlegen), Schlüssel mit `ssh-`. Der Host kommt nach vorn
    /// in die Liste der zuletzt verbundenen.
    pub fn new_remote_session(&mut self, host: &str, tmux: Option<&str>, show: bool) -> String {
        let key = remote::new_key();
        let mut session = Session::new(&key, &self.home(), now_ms(), &key, "");
        session.host = Some(host.to_string());
        session.tmux_session = tmux.map(String::from);
        let recent = remote::record_use(&self.recent_hosts(), host);
        self.settings.set(remote::RECENT_KEY, json!(recent));
        patch_setting(&self.profile.settings_path(), remote::RECENT_KEY, json!(recent));
        self.add_session(session, None, false, None);
        if show {
            // Wie in der Mac-App: zu den gezeigten Kacheln dazu, außer es ist keine gezeigt.
            if self.selected.is_empty() { self.show_only(&key) } else { self.select_keys(std::slice::from_ref(&key), true) }
        }
        key
    }

    /// Zuletzt gewählter Agent der Auswahl „Starten mit“ (`lastAgent`).
    pub fn set_last_agent(&mut self, kind: kadrell_core::AgentKind) {
        self.settings.set_last_agent(kind);
        patch_setting(&self.profile.settings_path(), "lastAgent", json!(kind.raw()));
    }

    // MARK: Start

    /// Startkommando von `key`. Pi und Remote brauchen vorher eine Vorbereitung im Hintergrund: solange sie läuft, ist die
    /// Antwort `Pending` und die Session bleibt vorgemerkt.
    pub fn start_spec(&mut self, key: &str, prompt: Option<&str>) -> Start {
        let Some(session) = self.session(key).cloned() else { return Start::Ready(Err("Session fehlt".into())) };
        if session.is_remote() {
            return self.remote_start(&session);
        }
        if session.is_pi() {
            return self.pi_start(&session, prompt);
        }
        Start::Ready(launch::spec_for(&session, &self.settings, &self.env, prompt))
    }

    fn remote_start(&mut self, session: &Session) -> Start {
        let (host, ssh) = (session.host.clone().unwrap_or_default(), remote::system_ssh());
        let job_host = host.clone();
        let job_ssh = ssh.clone();
        let prepared = self.prepared(&session.id, move || Prepared::Remote { has_tmux: remote::probe_tmux(&job_ssh, &job_host) });
        let Some(Prepared::Remote { has_tmux }) = prepared else { return Start::Pending };
        let message = trd!("Kein tmux auf diesem Host, normale Shell");
        let l = remote::launch(&ssh, &host, session.tmux_session.as_deref(), has_tmux, !cfg!(windows), &message);
        let env = tile_env(&self.env.base, &session.id, &self.env.socket, &self.env.exe);
        Start::Ready(Ok(LaunchSpec { program: l.program, args: l.args, cwd: session.cwd.clone(), env }))
    }

    fn pi_start(&mut self, session: &Session, prompt: Option<&str>) -> Start {
        let model = session.model.as_deref().and_then(LocalModelRef::parse);
        if self.pi_binary().is_some() && let Some(m) = model.filter(|m| m.provider == LocalProviderKind::LlamaCpp) && self.models.base_url_now(m.provider).is_none() {
            // Pi auf llama.cpp braucht den eigenen `llama-server`: erst ihn starten, dann die Session.
            let models = self.models.clone();
            if self.prepared(&session.id, move || {
                let _ = models.llama_base();
                Prepared::Pi
            })
            .is_none()
            {
                return Start::Pending;
            }
        }
        Start::Ready(Ok(self.pi_spec(session, prompt)))
    }

    fn pi_spec(&self, session: &Session, prompt: Option<&str>) -> LaunchSpec {
        let home = self.home();
        let base_url = session.model.as_deref().and_then(LocalModelRef::parse).and_then(|m| self.models.base_url_now(m.provider));
        let start = PiStart { session, env: &self.env.base.environment, home: &home, socket: &self.env.socket, exe: &self.env.exe, base_url: base_url.as_deref(), prompt };
        match self.pi_binary() {
            Some(binary) => {
                kadrell_claude::pi::install_extension(kadrell_claude::pi::EXTENSION_SOURCE, &home);
                pi_launch::launch(&start, &binary)
            }
            None => pi_launch::missing(&start, launch::shell_command(), &trd!("Pi ist nicht installiert (pi im PATH oder unter ~/.local/bin)")),
        }
    }

    /// Läuft `job` im Hintergrund, falls noch nicht geschehen. `Some`, sobald er fertig ist (einmal, danach vergessen).
    fn prepared(&mut self, key: &str, job: impl FnOnce() -> Prepared + Send + 'static) -> Option<Prepared> {
        let mut map = self.prep.lock().unwrap_or_else(PoisonError::into_inner);
        match map.get(key).copied() {
            Some(Some(done)) => {
                map.remove(key);
                Some(done)
            }
            Some(None) => None,
            None => {
                map.insert(key.to_string(), None);
                drop(map);
                let (prep, key) = (self.prep.clone(), key.to_string());
                std::thread::spawn(move || {
                    let done = job();
                    prep.lock().unwrap_or_else(PoisonError::into_inner).insert(key, Some(done));
                });
                None
            }
        }
    }
}
