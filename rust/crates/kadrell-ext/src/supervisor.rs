//! Reine Zustandsmaschine einer Extension: bekommt Ereignisse, liefert Aktionen, startet und misst selbst nichts.
//! Hänger, fehlendes Ready und Flut erkennt `ExtensionProcess`, killt selbst und meldet sie als unerwartetes Ende.
//! Ein Absturz zählt nur in `Starting` und `Running`; in `Failed`, `Off` und `Reloading` ist der Prozess schon
//! abgeschrieben, ein nachgereichtes Ende darf nicht doppelt zählen.

use std::time::{Duration, Instant};

use crate::i18n::{tr, tr_args};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExtensionState {
    #[default]
    Off,
    Starting,
    Running,
    Failed(String),
    Reloading,
}

impl ExtensionState {
    /// Klartext für Log und Dialog.
    pub fn label(&self) -> String {
        match self {
            Self::Off => tr("Aus", "Off"),
            Self::Starting => tr("Startet", "Starting"),
            Self::Running => tr("Läuft", "Running"),
            Self::Reloading => tr("Lädt neu", "Reloading"),
            Self::Failed(reason) => tr_args("Fehler: {0}", "Error: {0}", &[reason]),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorInput {
    Enable,
    Disable,
    Reload,
    Spawned,
    Ready,
    Exited { expected: bool, reason: String },
    RestartDue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorAction {
    Spawn,
    Terminate,
    ScheduleRestart(Duration),
    ClearUi,
}

pub const BACKOFF: [u64; 3] = [1, 5, 30];
pub const CRASH_WINDOW: Duration = Duration::from_secs(120);
pub const MAX_CRASHES_IN_WINDOW: usize = 3;
pub const STABLE_RUN: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Default)]
pub struct ExtensionSupervisor {
    state: ExtensionState,
    crashes: Vec<Instant>,
    consecutive: usize,
    ready_at: Option<Instant>,
    restart_pending: bool,
}

impl ExtensionSupervisor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> &ExtensionState {
        &self.state
    }

    pub fn handle(&mut self, input: SupervisorInput, now: Instant) -> Vec<SupervisorAction> {
        match input {
            SupervisorInput::Enable => self.enable(),
            SupervisorInput::Disable => self.disable(),
            SupervisorInput::Reload => self.reload(),
            SupervisorInput::Spawned => Vec::new(),
            SupervisorInput::Ready => self.ready(now),
            SupervisorInput::Exited { expected, reason } => self.exited(expected, &reason, now),
            SupervisorInput::RestartDue => self.restart_due(),
        }
    }

    fn is_failed(&self) -> bool {
        matches!(self.state, ExtensionState::Failed(_))
    }

    fn enable(&mut self) -> Vec<SupervisorAction> {
        if self.state != ExtensionState::Off && !self.is_failed() {
            return Vec::new();
        }
        self.crashes.clear();
        self.consecutive = 0;
        self.ready_at = None;
        self.restart_pending = false;
        self.state = ExtensionState::Starting;
        vec![SupervisorAction::Spawn]
    }

    fn disable(&mut self) -> Vec<SupervisorAction> {
        if self.state == ExtensionState::Off {
            return Vec::new();
        }
        let alive = matches!(
            self.state,
            ExtensionState::Starting | ExtensionState::Running | ExtensionState::Reloading
        );
        self.state = ExtensionState::Off;
        self.restart_pending = false;
        self.ready_at = None;
        if alive {
            vec![SupervisorAction::Terminate, SupervisorAction::ClearUi]
        } else {
            vec![SupervisorAction::ClearUi]
        }
    }

    fn reload(&mut self) -> Vec<SupervisorAction> {
        // Neue Dateiversion nach einem Fehler: zählt als neuer Versuch, wie ein erneutes Einschalten.
        if self.is_failed() {
            return self.enable();
        }
        if !matches!(
            self.state,
            ExtensionState::Starting | ExtensionState::Running
        ) {
            return Vec::new();
        }
        self.state = ExtensionState::Reloading;
        self.ready_at = None;
        vec![SupervisorAction::Terminate, SupervisorAction::ClearUi]
    }

    fn ready(&mut self, now: Instant) -> Vec<SupervisorAction> {
        if self.state != ExtensionState::Starting {
            return Vec::new();
        }
        self.state = ExtensionState::Running;
        self.ready_at = Some(now);
        Vec::new()
    }

    fn exited(&mut self, expected: bool, reason: &str, now: Instant) -> Vec<SupervisorAction> {
        if self.state == ExtensionState::Reloading {
            self.state = ExtensionState::Starting;
            return vec![SupervisorAction::Spawn];
        }
        if expected {
            Vec::new()
        } else {
            self.crash(reason, now)
        }
    }

    fn restart_due(&mut self) -> Vec<SupervisorAction> {
        if !self.is_failed() || !self.restart_pending {
            return Vec::new();
        }
        self.restart_pending = false;
        self.state = ExtensionState::Starting;
        vec![SupervisorAction::Spawn]
    }

    fn crash(&mut self, reason: &str, now: Instant) -> Vec<SupervisorAction> {
        if !matches!(
            self.state,
            ExtensionState::Starting | ExtensionState::Running
        ) {
            return Vec::new();
        }
        let ran_stable = self
            .ready_at
            .is_some_and(|at| now.saturating_duration_since(at) >= STABLE_RUN);
        self.ready_at = None;
        self.consecutive = (if ran_stable { 0 } else { self.consecutive }) + 1;
        self.crashes
            .retain(|t| now.saturating_duration_since(*t) < CRASH_WINDOW);
        self.crashes.push(now);
        if self.crashes.len() >= MAX_CRASHES_IN_WINDOW {
            self.state = ExtensionState::Failed(tr(
                "3 Abstürze in 2 min, bleibt aus",
                "3 crashes in 2 min, stays off",
            ));
            return vec![SupervisorAction::ClearUi];
        }
        self.state = ExtensionState::Failed(reason.to_string());
        self.restart_pending = true;
        let delay = Duration::from_secs(BACKOFF[self.consecutive.min(BACKOFF.len()) - 1]);
        vec![
            SupervisorAction::ClearUi,
            SupervisorAction::ScheduleRestart(delay),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SupervisorAction::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn none() -> Vec<SupervisorAction> {
        Vec::new()
    }

    struct Fixture {
        t0: Instant,
    }

    impl Fixture {
        fn at(&self, s: f64) -> Instant {
            self.t0 + Duration::from_secs_f64(s)
        }

        fn running(&self) -> ExtensionSupervisor {
            let mut s = ExtensionSupervisor::new();
            s.handle(SupervisorInput::Enable, self.t0);
            s.handle(SupervisorInput::Spawned, self.t0);
            s.handle(SupervisorInput::Ready, self.t0);
            s
        }

        /// Abstürzen, neu starten lassen und wieder bis `Running` bringen (Neustart 1 s nach dem Absturz).
        fn crash_and_recover(&self, s: &mut ExtensionSupervisor, at: f64) -> Vec<SupervisorAction> {
            let actions = s.handle(boom(), self.at(at));
            s.handle(SupervisorInput::RestartDue, self.at(at + 1.0));
            s.handle(SupervisorInput::Spawned, self.at(at + 1.0));
            s.handle(SupervisorInput::Ready, self.at(at + 1.0));
            actions
        }

        fn failed_permanently(&self) -> ExtensionSupervisor {
            let mut s = self.running();
            self.crash_and_recover(&mut s, 0.0);
            self.crash_and_recover(&mut s, 10.0);
            s.handle(boom(), self.at(20.0));
            s
        }
    }

    fn fx() -> Fixture {
        Fixture { t0: Instant::now() }
    }

    fn boom() -> SupervisorInput {
        SupervisorInput::Exited {
            expected: false,
            reason: "boom".into(),
        }
    }

    fn expected_exit() -> SupervisorInput {
        SupervisorInput::Exited {
            expected: true,
            reason: String::new(),
        }
    }

    #[test]
    fn enable_spawns_and_ready_runs() {
        let f = fx();
        let mut s = ExtensionSupervisor::new();
        assert_eq!(s.state(), &ExtensionState::Off);
        assert_eq!(s.handle(SupervisorInput::Enable, f.t0), [Spawn]);
        assert_eq!(s.state(), &ExtensionState::Starting);
        s.handle(SupervisorInput::Spawned, f.t0);
        s.handle(SupervisorInput::Ready, f.t0);
        assert_eq!(s.state(), &ExtensionState::Running);
    }

    #[test]
    fn crash_backoff_sequence() {
        let f = fx();
        let mut s = f.running();
        assert_eq!(
            f.crash_and_recover(&mut s, 0.0),
            [ClearUi, ScheduleRestart(secs(1))]
        );
        assert_eq!(
            f.crash_and_recover(&mut s, 10.0),
            [ClearUi, ScheduleRestart(secs(5))]
        );
        assert_eq!(s.handle(boom(), f.at(20.0)), [ClearUi]);
        let ExtensionState::Failed(reason) = s.state() else {
            panic!("{:?}", s.state())
        };
        assert!(reason.contains('3'), "{reason}");
        assert_eq!(
            s.handle(SupervisorInput::RestartDue, f.at(21.0)),
            none(),
            "ein dauerhaft ausgeschalteter Neustart darf nichts starten"
        );
    }

    #[test]
    fn crashes_outside_window_reset() {
        let f = fx();
        let mut s = f.running();
        for t in [0.0, 200.0, 400.0] {
            assert_eq!(
                f.crash_and_recover(&mut s, t),
                [ClearUi, ScheduleRestart(secs(1))],
                "t={t}"
            );
        }
    }

    /// Stabile Läufe von unter 120 s setzen den Backoff nicht zurück, die 3 Abstürze liegen aber nicht in einem Fenster.
    #[test]
    fn thirty_second_backoff_reachable() {
        let f = fx();
        let mut s = f.running();
        assert_eq!(
            f.crash_and_recover(&mut s, 0.0),
            [ClearUi, ScheduleRestart(secs(1))]
        );
        assert_eq!(
            f.crash_and_recover(&mut s, 100.0),
            [ClearUi, ScheduleRestart(secs(5))]
        );
        assert_eq!(
            f.crash_and_recover(&mut s, 210.0),
            [ClearUi, ScheduleRestart(secs(30))]
        );
        assert_eq!(s.state(), &ExtensionState::Running);
    }

    #[test]
    fn reload_does_not_count() {
        let f = fx();
        let mut s = f.running();
        for i in 0..5 {
            let t = f.at(i as f64);
            assert_eq!(s.handle(SupervisorInput::Reload, t), [Terminate, ClearUi]);
            assert_eq!(s.state(), &ExtensionState::Reloading);
            assert_eq!(s.handle(expected_exit(), t), [Spawn]);
            s.handle(SupervisorInput::Spawned, t);
            s.handle(SupervisorInput::Ready, t);
            assert_eq!(s.state(), &ExtensionState::Running);
        }
    }

    #[test]
    fn disable_terminates_and_clears() {
        let f = fx();
        let mut s = f.running();
        assert_eq!(
            s.handle(SupervisorInput::Disable, f.t0),
            [Terminate, ClearUi]
        );
        assert_eq!(s.state(), &ExtensionState::Off);
        assert_eq!(s.handle(expected_exit(), f.t0), none());
        assert_eq!(s.state(), &ExtensionState::Off);
    }

    #[test]
    fn enable_from_failed_resets_counter() {
        let f = fx();
        let mut s = f.running();
        f.crash_and_recover(&mut s, 0.0);
        f.crash_and_recover(&mut s, 10.0);
        s.handle(boom(), f.at(20.0));
        assert_eq!(s.handle(SupervisorInput::Enable, f.at(30.0)), [Spawn]);
        s.handle(SupervisorInput::Ready, f.at(30.0));
        assert_eq!(
            s.handle(boom(), f.at(31.0)),
            [ClearUi, ScheduleRestart(secs(1))]
        );
    }

    #[test]
    fn restart_due_spawns() {
        let f = fx();
        let mut s = f.running();
        s.handle(boom(), f.t0);
        assert_eq!(s.handle(SupervisorInput::RestartDue, f.at(1.0)), [Spawn]);
        assert_eq!(s.state(), &ExtensionState::Starting);
    }

    #[test]
    fn disable_while_waiting_for_restart_cancels_it() {
        let f = fx();
        let mut s = f.running();
        s.handle(boom(), f.t0);
        assert_eq!(s.handle(SupervisorInput::Disable, f.at(0.5)), [ClearUi]);
        assert_eq!(s.handle(SupervisorInput::RestartDue, f.at(1.0)), none());
    }

    #[test]
    fn reload_in_failed_with_pending_restart_spawns_and_cancels_restart() {
        let f = fx();
        let mut s = f.running();
        s.handle(boom(), f.t0);
        assert_eq!(s.handle(SupervisorInput::Reload, f.at(0.5)), [Spawn]);
        assert_eq!(s.state(), &ExtensionState::Starting);
        assert_eq!(s.handle(SupervisorInput::RestartDue, f.at(1.0)), none());
    }

    #[test]
    fn reload_in_permanent_failed_spawns() {
        let f = fx();
        let mut s = f.failed_permanently();
        assert_eq!(s.handle(SupervisorInput::Reload, f.at(30.0)), [Spawn]);
        assert_eq!(s.state(), &ExtensionState::Starting);
    }

    #[test]
    fn reload_in_failed_resets_counters() {
        let f = fx();
        let mut s = f.failed_permanently();
        s.handle(SupervisorInput::Reload, f.at(30.0));
        s.handle(SupervisorInput::Spawned, f.at(30.0));
        assert_eq!(
            s.handle(boom(), f.at(31.0)),
            [ClearUi, ScheduleRestart(secs(1))]
        );
    }

    #[test]
    fn reload_in_off_is_noop() {
        let f = fx();
        let mut s = ExtensionSupervisor::new();
        assert_eq!(s.handle(SupervisorInput::Reload, f.t0), none());
        assert_eq!(s.state(), &ExtensionState::Off);
    }

    #[test]
    fn reload_while_starting_behaves_like_running() {
        let f = fx();
        let mut s = ExtensionSupervisor::new();
        s.handle(SupervisorInput::Enable, f.t0);
        assert_eq!(
            s.handle(SupervisorInput::Reload, f.t0),
            [Terminate, ClearUi]
        );
        assert_eq!(s.state(), &ExtensionState::Reloading);
    }

    #[test]
    fn enable_while_running_is_noop() {
        let f = fx();
        let mut s = f.running();
        assert_eq!(s.handle(SupervisorInput::Enable, f.t0), none());
        assert_eq!(s.state(), &ExtensionState::Running);
    }

    #[test]
    fn ready_is_ignored_outside_starting() {
        let f = fx();
        let mut off = ExtensionSupervisor::new();
        assert_eq!(off.handle(SupervisorInput::Ready, f.t0), none());
        assert_eq!(off.state(), &ExtensionState::Off);
        let mut failed = f.failed_permanently();
        let before = failed.state().clone();
        assert_eq!(failed.handle(SupervisorInput::Ready, f.at(30.0)), none());
        assert_eq!(failed.state(), &before);
        let mut reloading = f.running();
        reloading.handle(SupervisorInput::Reload, f.t0);
        assert_eq!(reloading.handle(SupervisorInput::Ready, f.t0), none());
        assert_eq!(reloading.state(), &ExtensionState::Reloading);
    }

    #[test]
    fn labels_are_texts() {
        assert_eq!(
            ExtensionState::Failed("x".into()).label(),
            tr_args("Fehler: {0}", "Error: {0}", &["x"])
        );
    }
}
