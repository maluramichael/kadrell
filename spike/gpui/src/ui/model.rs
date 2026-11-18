//! Mock-Daten: Gruppen und Sessions, dazu das Startkommando je Session.

use std::path::PathBuf;

use gpui::{Entity, Hsla};

use crate::terminal::TerminalPane;
use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Running,
    Waiting,
    Idle,
    Error,
    Detached,
}

impl Status {
    pub fn color(self, t: &Theme) -> Hsla {
        match self {
            Status::Running => t.running,
            Status::Waiting => t.waiting,
            Status::Idle => t.idle,
            Status::Error => t.error,
            Status::Detached => t.detached,
        }
    }
}

type Command = (&'static str, &'static [&'static str]);

pub struct Launch {
    pub program: &'static str,
    pub args: &'static [&'static str],
    pub in_spike: bool,
}

impl Launch {
    /// Ordner des Spikes (`spike/gpui`) oder die Repo-Wurzel.
    pub fn cwd(&self) -> PathBuf {
        let spike = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if self.in_spike {
            return spike;
        }
        spike.ancestors().nth(2).map(PathBuf::from).unwrap_or(spike)
    }
}

pub struct Session {
    pub title: &'static str,
    pub status: Status,
    pub is_new: bool,
    pub age: &'static str,
    pub launch: Launch,
    pub pane: Option<Entity<TerminalPane>>,
}

pub struct Group {
    pub name: &'static str,
    pub color: u32,
    pub collapsed: bool,
    pub sessions: Vec<Session>,
}

#[cfg(target_os = "macos")]
const CLAUDE: Command = ("/bin/zsh", &["-lc", "claude"]);
#[cfg(not(target_os = "macos"))]
const CLAUDE: Command = ("claude", &[]);

#[cfg(target_os = "macos")]
const SHELL: Command = ("/bin/zsh", &["-l"]);
#[cfg(not(target_os = "macos"))]
const SHELL: Command = ("pwsh.exe", &[]);

fn session(title: &'static str, status: Status, age: &'static str, cmd: Command) -> Session {
    let launch = Launch { program: cmd.0, args: cmd.1, in_spike: false };
    Session { title, status, is_new: false, age, launch, pane: None }
}

fn in_spike(mut s: Session) -> Session {
    s.launch.in_spike = true;
    s
}

fn marked_new(mut s: Session) -> Session {
    s.is_new = true;
    s
}

pub fn mock_groups() -> Vec<Group> {
    vec![
        Group {
            name: "kadrell",
            color: 0x89b4fa,
            collapsed: false,
            sessions: vec![
                marked_new(session("claude", Status::Running, "3h", CLAUDE)),
                session("shell", Status::Idle, "1d", SHELL),
            ],
        },
        Group {
            name: "spike",
            color: 0xa6e3a1,
            collapsed: false,
            sessions: vec![
                in_spike(session("claude-zwei", Status::Waiting, "3h", CLAUDE)),
                in_spike(session("zweite shell", Status::Detached, "1d", SHELL)),
            ],
        },
    ]
}

/// Alle Sessions in Anzeigereihenfolge als (Gruppe, Session).
pub fn flat_index(groups: &[Group]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        out.extend((0..group.sessions.len()).map(|s| (g, s)));
    }
    out
}
