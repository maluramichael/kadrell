//! Prozessliste für die Erkennung laufender Bash-Werkzeug-Kommandos (`kadrell_claude::background_shells`). Dort steht
//! `ps` als Quelle, das es unter Windows nicht gibt; hier liefert `sysinfo` dieselbe Liste auf jedem System.

use std::collections::{HashMap, HashSet};

use kadrell_claude::background_shells::{self, ProcessTable};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Eine Momentaufnahme der Prozesse: Eltern, Programmname und Kommandozeile je pid.
#[derive(Debug, Default)]
pub struct ProcessSnapshot {
    rows: HashMap<u32, Row>,
}

#[derive(Debug, Clone)]
struct Row {
    parent: Option<u32>,
    name: String,
    args: String,
}

impl ProcessSnapshot {
    /// Aus Zeilen `(pid, ppid, Programm, Kommandozeile)`, für Tests.
    pub fn from_rows(rows: &[(u32, u32, &str, &str)]) -> ProcessSnapshot {
        let rows = rows.iter().map(|(pid, ppid, name, args)| (*pid, Row { parent: Some(*ppid), name: (*name).into(), args: (*args).into() })).collect();
        ProcessSnapshot { rows }
    }

    /// Die Prozesse des Systems jetzt. Leer, wenn das System sie nicht herausgibt.
    pub fn take() -> ProcessSnapshot {
        let Ok(mut system) = System::new() else { return ProcessSnapshot::default() };
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet));
        let rows = system
            .processes()
            .iter()
            .map(|(pid, p)| {
                let args: Vec<String> = p.cmd().iter().map(|a| a.to_string_lossy().into_owned()).collect();
                (pid.as_u32(), Row { parent: p.parent().map(|pp| pp.as_u32()), name: p.name().to_string_lossy().into_owned(), args: args.join(" ") })
            })
            .collect();
        ProcessSnapshot { rows }
    }
}

impl ProcessTable for ProcessSnapshot {
    fn children(&self, pid: u32) -> Vec<u32> {
        self.rows.iter().filter(|(_, r)| r.parent == Some(pid)).map(|(p, _)| *p).collect()
    }

    fn image_name(&self, pid: u32) -> Option<String> {
        let name = &self.rows.get(&pid)?.name;
        Some(name.strip_suffix(".exe").unwrap_or(name).to_string())
    }

    fn arguments(&self, pid: u32) -> Option<String> {
        self.rows.get(&pid).map(|r| r.args.clone()).filter(|a| !a.is_empty())
    }
}

/// pids unter `pids`, in deren Prozessbaum noch ein Bash-Werkzeug-Kommando läuft („1 shell still running“). Ohne
/// Prozesse zum Prüfen kostet es nichts: die Liste wird nur gelesen, wenn es etwas zu fragen gibt.
pub fn running_shells(pids: &[u32]) -> HashSet<u32> {
    if pids.is_empty() {
        return HashSet::new();
    }
    background_shells::running(pids, &ProcessSnapshot::take())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAPSHOT: &[(u32, u32, &str, &str)] = &[
        (100, 1, "claude.exe", "claude --resume abc"),
        (101, 100, "bash.exe", "bash -c source C:/Users/x/.claude/shell-snapshots/snapshot-bash-1.sh && sleep 30"),
        (102, 100, "node.exe", "node mcp.js C:/Users/x/.claude/shell-snapshots/snapshot-fake"),
        (200, 1, "claude.exe", "claude --resume def"),
        (201, 200, "pwsh.exe", "pwsh -NoProfile"),
    ];

    #[test]
    fn a_tool_shell_under_claude_counts() {
        let table = ProcessSnapshot::from_rows(SNAPSHOT);
        assert_eq!(background_shells::running(&[100, 200], &table), HashSet::from([100]));
    }

    #[test]
    fn image_names_lose_the_exe_suffix_and_empty_args_are_unreadable() {
        let table = ProcessSnapshot::from_rows(&[(1, 0, "bash.exe", "")]);
        assert_eq!(table.image_name(1).as_deref(), Some("bash"));
        assert_eq!(table.arguments(1), None);
        assert_eq!(table.image_name(9), None);
    }

    #[test]
    fn children_come_from_the_parent_column() {
        let mut kids = ProcessSnapshot::from_rows(SNAPSHOT).children(100);
        kids.sort_unstable();
        assert_eq!(kids, [101, 102]);
    }

    #[test]
    fn nothing_to_ask_reads_no_process_list() {
        assert!(running_shells(&[]).is_empty());
    }

    #[test]
    fn the_real_process_list_contains_this_process() {
        let me = std::process::id();
        let snapshot = ProcessSnapshot::take();
        assert!(snapshot.image_name(me).is_some(), "eigener Prozess fehlt in der Liste");
    }
}
