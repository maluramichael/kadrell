//! Mock-Daten: Gruppen und Sessions, dazu das Startkommando je Session.

use std::path::PathBuf;

use gpui::{Entity, Hsla};

use kadrell_term::TerminalPane;
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
    /// Eigener Startordner (Sessions aus „Neue Session“); sonst gilt `in_spike`.
    pub dir: Option<PathBuf>,
}

impl Launch {
    /// Eigener Ordner, sonst der der App (`rust/crates/kadrell-app`) oder die Repo-Wurzel.
    pub fn cwd(&self) -> PathBuf {
        if let Some(dir) = &self.dir {
            return dir.clone();
        }
        let spike = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if self.in_spike {
            return spike;
        }
        spike.ancestors().nth(3).map(PathBuf::from).unwrap_or(spike)
    }
}

pub struct Session {
    pub title: &'static str,
    pub status: Status,
    pub is_new: bool,
    pub age: &'static str,
    pub branch: Option<String>,
    pub launch: Launch,
    pub pane: Option<Entity<TerminalPane>>,
}

pub struct Group {
    pub name: &'static str,
    pub color: u32,
    pub path: &'static str,
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
    let launch = Launch { program: cmd.0, args: cmd.1, in_spike: false, dir: None };
    Session { title, status, is_new: false, age, branch: None, launch, pane: None }
}

/// Neue Claude-Session in einem beliebigen Ordner („Neue Session“).
pub fn claude_session_in(title: &'static str, dir: PathBuf) -> Session {
    let mut s = session(title, Status::Running, "jetzt", CLAUDE);
    s.is_new = true;
    s.launch.dir = Some(dir);
    s
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
            path: "~/development/projects/kadrell",
            collapsed: false,
            sessions: vec![
                marked_new(session("claude", Status::Running, "3h", CLAUDE)),
                session("shell", Status::Idle, "1d", SHELL),
            ],
        },
        Group {
            name: "spike",
            color: 0xa6e3a1,
            path: "~/development/projects/kadrell/spike/gpui",
            collapsed: false,
            sessions: vec![
                in_spike(session("claude-zwei", Status::Waiting, "3h", CLAUDE)),
                in_spike(session("zweite shell", Status::Detached, "1d", SHELL)),
            ],
        },
    ]
}

/// Aktueller Git-Zweig des Ordners, `None` außerhalb eines Repos (oder ohne Zweig, etwa bei `HEAD`).
/// Blockiert, deshalb nur aus dem Hintergrund aufrufen.
pub fn git_branch(cwd: &std::path::Path) -> Option<String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["rev-parse", "--abbrev-ref", "HEAD"]).current_dir(cwd);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!name.is_empty() && name != "HEAD").then_some(name)
}

/// Alle Sessions in Anzeigereihenfolge als (Gruppe, Session).
pub fn flat_index(groups: &[Group]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        out.extend((0..group.sessions.len()).map(|s| (g, s)));
    }
    out
}
