//! Ausgabe von `kadrell ls` (Text und `--json`) und `ssh-hosts`, wie `AppDelegate+Control.swift` und
//! `docs/contract/ls.json` sie festlegen. Die Felder stehen in den Strukturen alphabetisch: die Mac-App schreibt
//! sortierte Schlüssel, und das soll nicht davon abhängen, ob irgendwo im Workspace `preserve_order` an ist.
//! Fehlt `focused` oder `branch`, fehlt der Schlüssel (kein `null`).

use crate::handler::{ControlHandler, RunState, SshHosts};
use kadrell_core::Session;
use serde::Serialize;

#[derive(Serialize)]
struct LsOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    focused: Option<String>,
    groups: Vec<LsGroup>,
    layout: &'static str,
}

#[derive(Serialize)]
struct LsGroup {
    color: String,
    cwd: String,
    favorite: bool,
    id: String,
    name: String,
    sessions: Vec<LsSession>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LsSession {
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    cwd: String,
    focused: bool,
    key: String,
    running: bool,
    session_id: String,
    shown: bool,
    status: &'static str,
    title: String,
}

/// `ls` oder `ls --json`.
pub fn list_output(h: &dyn ControlHandler, json: bool) -> String {
    let out = collect(h);
    if json {
        return serde_json::to_string_pretty(&out).unwrap_or_default();
    }
    out.groups.iter().map(text_group).collect::<Vec<_>>().join("\n")
}

fn collect(h: &dyn ControlHandler) -> LsOut {
    let store = h.store();
    let (focused, selected) = (h.focused(), h.selected());
    let session = |s: &Session| {
        let state = h.run_state(&s.id);
        LsSession {
            branch: s.branch.clone(),
            cwd: s.cwd.clone(),
            focused: focused.as_deref() == Some(s.id.as_str()),
            key: s.id.clone(),
            running: state == RunState::Running,
            session_id: s.session_id.clone(),
            shown: selected.contains(&s.id),
            status: status_word(s, state),
            title: s.title(),
        }
    };
    let groups = store
        .groups
        .groups
        .iter()
        .map(|g| LsGroup {
            color: g.color.clone(),
            cwd: g.cwd.clone(),
            favorite: g.favorite,
            id: g.id.clone(),
            name: g.name.clone(),
            sessions: g.session_ids.iter().filter_map(|id| store.sessions.get(id)).map(session).collect(),
        })
        .collect();
    LsOut { focused, groups, layout: h.layout().as_str() }
}

/// Bei laufendem Prozess der Zustand der Session, sonst `ended` (hat sich selbst beendet) oder `stopped`.
fn status_word(s: &Session, state: RunState) -> &'static str {
    match state {
        RunState::Running => s.status().as_str(),
        RunState::Ended => "ended",
        RunState::Stopped => "stopped",
    }
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn text_group(g: &LsGroup) -> String {
    let fav = if g.favorite { " ★" } else { "" };
    let mut lines = vec![format!("{}  {}{}  {}", short(&g.id), g.name, fav, g.cwd)];
    for s in &g.sessions {
        let mark = if s.focused { "*" } else if s.shown { "+" } else { " " };
        lines.push(format!("  {mark} {}  {:<8}  {}", short(&s.key), s.status, s.title));
    }
    lines.join("\n")
}

#[derive(Serialize)]
struct HostsOut<'a> {
    hosts: &'a [String],
    recent: &'a [String],
}

/// `ssh-hosts`: ein Host je Zeile, mit `--json` ein Objekt in einer Zeile.
pub fn ssh_hosts_output(hosts: &SshHosts, json: bool) -> String {
    if json {
        return serde_json::to_string(&HostsOut { hosts: &hosts.hosts, recent: &hosts.recent }).unwrap_or_else(|_| "{}".into());
    }
    hosts.hosts.join("\n")
}
