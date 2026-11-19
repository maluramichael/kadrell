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
        env.process_start = Box::new(crate::misc::procs::process_start);
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

    /// Ist ein Poll fällig? Gleicht dabei den Hintergrundmodus ab: Fenster verdeckt, versteckt oder nicht aktiv pollt
    /// seltener, beim Zurückkommen sofort (`SessionRegistry.setBackground`).
    pub fn poll_due(&self, elapsed_secs: f64) -> bool {
        let mut registry = self.registry.lock().unwrap_or_else(PoisonError::into_inner);
        let returned = registry.set_background(!self.window_active);
        due(elapsed_secs, registry.poll_interval(), returned)
    }
}

/// Fällig, wenn der Takt um ist oder die App eben aus dem Hintergrund kam.
fn due(elapsed_secs: f64, interval: f64, returned: bool) -> bool {
    returned || elapsed_secs >= interval
}

#[cfg(test)]
mod tests {
    use super::due;

    #[test]
    fn a_poll_is_due_after_the_interval_or_when_coming_back() {
        assert!(!due(1.5, 2., false));
        assert!(due(2., 2., false));
        assert!(due(0.1, 5., true), "aus dem Hintergrund zurück: sofort");
    }
}
