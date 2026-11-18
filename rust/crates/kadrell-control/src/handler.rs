//! Die Schnittstelle zur App. Der reine Teil (Befehle parsen, Ziele auflösen, Freigaben, `ls`, `send`, `capture`)
//! steckt in [`crate::exec`]; die App liefert nur ihren Zustand und die Handgriffe, die Prozesse, Terminals und
//! Oberfläche berühren. Alle Ziele kommen bei den Handgriffen schon aufgelöst und freigegeben an.

use crate::command::{LayoutMode, Result, Status};
use kadrell_core::{Group, Store};
use std::path::PathBuf;

/// Lebenszustand einer Session für `ls`: `Running` heißt, ihr Prozess läuft in einer Kachel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Running,
    /// Der Prozess hat sich selbst beendet (`/exit`, Absturz), nicht über `stop` oder `kill`.
    Ended,
    /// Gestoppt oder noch nie gestartet.
    Stopped,
}

/// Was `capture` von einer Kachel bekommt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capture {
    /// Sichtbare Zeilen des Terminals. `capture` kürzt Leerzeichen am Ende und leere Zeilen am Schluss.
    Rows(Vec<String>),
    /// Der ganze Puffer mit Verlauf (`--all`), unverändert.
    Buffer(String),
    /// Kein Terminal (Session gestoppt): die zuletzt gemerkten Zeilen, unverändert.
    Stopped(Vec<String>),
}

/// Warum `kadrell new --resume` nicht geht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeBlocker {
    /// Die Konversation gehört schon einer Session in Kadrell.
    InKadrell,
    NoTranscript,
    /// Claude läuft dort noch (pid): dieselbe Konversation in zwei Prozessen schriebe durcheinander.
    Running(i64),
}

/// Hosts für `ssh-hosts`: alle aus `~/.ssh/config` (mit `Include`) und die zuletzt verbundenen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SshHosts {
    pub hosts: Vec<String>,
    pub recent: Vec<String>,
}

/// Eine neue Session, wie `kadrell new` sie verlangt, nach Auflösung von Gruppe und Ordner.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewRequest {
    /// Id der Gruppe, `None`: die mit gleichem Ordner oder eine neue.
    pub group: Option<String>,
    /// Absoluter, vorhandener Ordner (leer bei `host`).
    pub cwd: String,
    /// Auswahl auf die neue Session setzen (kein `-d`).
    pub show: bool,
    pub prompt: Option<String>,
    /// Konversation übernehmen, schon geprüft (UUID, nirgends sonst in Gebrauch).
    pub resume: Option<String>,
    /// Remote-Kachel per ssh zu diesem Host, mit `tmux` als Session dort. Dann sind Gruppe und Ordner leer.
    pub host: Option<String>,
    pub tmux: Option<String>,
    /// Pi statt Claude. Ob Pi und ein Modell vorhanden sind, steht schon fest (`pi_available`).
    pub pi: bool,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// Was die App für den Steuerkanal können muss. Aufgerufen wird immer von genau einem Thread zugleich (siehe
/// [`crate::exec::Controller`]); die Methoden dürfen also nicht blockieren, `send` wartet nicht darin.
pub trait ControlHandler {
    // MARK: Zustand lesen

    /// `false`, solange die App noch bootet: die Antwort ist dann `STARTING_STATUS`, der Client wartet weiter.
    fn ready(&self) -> bool {
        true
    }
    /// Gruppen und Sessions.
    fn store(&self) -> &Store;
    /// Key und pid des Kindprozesses jeder laufenden Kachel, für die Bestimmung des Aufrufers.
    fn tile_pids(&self) -> Vec<(String, u32)>;
    fn focused(&self) -> Option<String>;
    /// Keys der angezeigten Sessions.
    fn selected(&self) -> Vec<String>;
    fn layout(&self) -> LayoutMode;
    /// Einstellung „Sessions dürfen andere Sessions steuern“ (`control.otherSessions`).
    fn others_allowed(&self) -> bool;
    fn run_state(&self, key: &str) -> RunState;
    /// Persönlicher Ordner für `~` in Pfaden.
    fn home(&self) -> PathBuf {
        kadrell_core::paths::home_dir()
    }
    fn ssh_hosts(&self) -> SshHosts;
    fn resume_blocker(&self, session_id: &str) -> Option<ResumeBlocker>;
    /// Pi ist installiert und ein Modell für Pi-Sessions gewählt.
    fn pi_available(&self) -> bool;
    /// `None`: die Session hat kein laufendes Terminal. `Some(an)`: ob das Programm bracketed paste eingeschaltet hat.
    fn bracketed_paste(&self, key: &str) -> Option<bool>;
    fn capture(&self, key: &str, all: bool) -> Capture;

    // MARK: Handgriffe

    /// Neue Gruppe (Favorit), speichern und Ansicht auffrischen.
    fn add_group(&mut self, group: Group);
    /// Gruppe mit gleicher Id ersetzen (`set-group`).
    fn update_group(&mut self, group: Group);
    fn move_session(&mut self, key: &str, group_id: &str);
    /// Eigener Name, leer heißt wieder der Titel von Claude Code.
    fn rename(&mut self, key: &str, name: &str);
    /// Startet die Session und gibt ihren Key zurück.
    fn new_session(&mut self, request: NewRequest) -> Result<String>;
    /// Auswahl setzen oder (`add`) erweitern bzw. umschalten.
    fn select(&mut self, keys: &[String], add: bool);
    fn focus(&mut self, key: &str);
    fn set_layout(&mut self, mode: LayoutMode);
    fn toggle_zoom(&mut self);
    /// Prozess beenden, Kachel bleibt.
    fn stop(&mut self, key: &str);
    /// Prozess (wieder) starten.
    fn resume(&mut self, key: &str) -> Result<()>;
    /// Beenden und entfernen, ohne Rückfrage.
    fn kill(&mut self, key: &str);
    fn kill_group(&mut self, group_id: &str);
    /// Rohe Bytes an das Terminal der Session.
    fn terminal_write(&mut self, key: &str, data: &str);
    /// Zustand von außen setzen (Hooks der Agent-CLIs).
    fn report_status(&mut self, key: &str, status: &Status);
    /// `kadrell kill-server`: nach der Antwort beenden.
    fn kill_server(&mut self);
}
