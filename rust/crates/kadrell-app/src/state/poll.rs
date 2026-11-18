//! Statuspoll im Hintergrund: Session-Dateien von Claude Code, Transcripts und Git-Zweige für Kacheln, die sich nicht
//! per Hook melden. Läuft auf dem Hintergrund-Executor, der Zustand holt sich danach das Ergebnis.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use kadrell_claude::cli::SystemRunner;
use kadrell_claude::registry::{Change, FsPollEnv, SessionRegistry};

use super::AppState;

/// Alles, was ein Poll braucht, und `Send`: wird mit dem Zustand der Kacheln auf dem Hauptthread zusammengestellt.
pub struct PollJob {
    registry: Arc<Mutex<SessionRegistry>>,
    config_dir: String,
    pids: HashMap<u32, String>,
}

impl PollJob {
    pub fn run(self) -> Change {
        self.run_counted().0
    }

    /// Wie `run`, dazu die Nachrichten, die seit dem letzten Poll in den Transcripts dazugekommen sind (Statistik).
    pub fn run_counted(self) -> (Change, usize) {
        let runner = SystemRunner;
        let mut env = FsPollEnv::new(&self.config_dir, self.pids, &runner);
        env.running_shells = Box::new(crate::misc::procs::running_shells);
        let change = self.registry.lock().unwrap_or_else(PoisonError::into_inner).poll_now(&env);
        let messages = env.reader.lock().map_or(0, |mut r| r.take_new_messages());
        (change, messages)
    }
}

impl AppState {
    pub fn poll_job(&self) -> PollJob {
        PollJob { registry: self.registry.clone(), config_dir: self.env.config_dir(), pids: self.pids() }
    }

    /// Sekunden bis zum nächsten Poll (im Hintergrund seltener).
    pub fn poll_interval(&self) -> f64 {
        let mut registry = self.registry.lock().unwrap_or_else(PoisonError::into_inner);
        registry.set_background(!self.window_active);
        registry.poll_interval()
    }
}
