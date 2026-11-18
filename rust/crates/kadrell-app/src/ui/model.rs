//! Ansicht der Sessions für den Baum, die Kacheln und die Suche: Gruppen und Sessions in Anzeigereihenfolge, aus dem
//! `AppState` gebaut. Die Oberfläche kennt nur diese Felder; Schlüssel (`key`, `id`) bleiben stabil, die Positionen
//! (Gruppe, Session) gelten nur bis zur nächsten Änderung.

use kadrell_i18n::trd;
use std::collections::HashMap;

use gpui::{Entity, Hsla};

use kadrell_core::group::SidebarFlat;
use kadrell_core::{Group as CoreGroup, Session as CoreSession, SessionStatus};
use kadrell_term::TerminalPane;

use super::folders;
use crate::state::AppState;
use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Status {
    Running,
    Waiting,
    Idle,
    Error,
    /// Kein laufender Prozess (gestoppt, beendet, noch nicht gestartet).
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

    /// Ohne laufenden Prozess (`None`) gilt die Session als getrennt.
    pub fn of(status: Option<SessionStatus>) -> Status {
        match status {
            None => Status::Detached,
            Some(SessionStatus::Running) => Status::Running,
            Some(SessionStatus::Waiting) => Status::Waiting,
            Some(SessionStatus::Idle) => Status::Idle,
            Some(SessionStatus::Error) => Status::Error,
        }
    }
}

pub struct Session {
    pub key: String,
    pub title: String,
    pub status: Status,
    /// Fertig oder wartend, aber noch nicht angesehen.
    pub is_new: bool,
    pub age: String,
    pub branch: Option<String>,
    /// Ordner mit `/`.
    pub cwd: String,
    pub is_shell: bool,
    pub pane: Option<Entity<TerminalPane>>,
}

pub struct Group {
    pub id: String,
    pub name: String,
    pub color: u32,
    /// Ordner mit `~` für die Anzeige.
    pub path: String,
    /// Ordner mit `/`.
    pub cwd: String,
    pub favorite: bool,
    pub collapsed: bool,
    pub sessions: Vec<Session>,
}

/// `#rrggbb` aus `groups.json` als Zahl, bei Unlesbarem ein neutrales Grau.
pub fn parse_color(text: &str) -> u32 {
    u32::from_str_radix(text.trim_start_matches('#'), 16).ok().filter(|_| text.trim_start_matches('#').len() == 6).unwrap_or(0x6c7086)
}

fn session_view(s: &CoreSession, state: &AppState, panes: &HashMap<String, Entity<TerminalPane>>, now: f64) -> Session {
    Session {
        key: s.id.clone(),
        title: s.title(),
        status: Status::of(state.display_status(&s.id)),
        is_new: state.is_unseen(&s.id),
        age: s.elapsed(now),
        branch: s.branch.clone(),
        cwd: s.cwd.clone(),
        is_shell: s.is_shell(),
        pane: panes.get(&s.id).cloned(),
    }
}

fn group_view(g: &CoreGroup, sessions: Vec<Session>, state: &AppState, home: &str) -> Group {
    Group {
        id: g.id.clone(),
        name: g.name.clone(),
        color: parse_color(&g.color),
        path: folders::short_path(&g.cwd, home),
        cwd: g.cwd.clone(),
        favorite: g.favorite,
        collapsed: state.collapsed.contains(&g.id),
        sessions,
    }
}

/// Gruppen mit der Sortierung der Leiste. Ohne Gruppierung steht eine einzige Gruppe mit allen Sessions da.
pub fn build(state: &AppState, panes: &HashMap<String, Entity<TerminalPane>>) -> Vec<Group> {
    let now = crate::state::now_secs();
    let home = state.home();
    let groups = state.sorted_groups();
    let by_key: HashMap<String, CoreSession> = state.store.sessions.sessions.iter().map(|s| (s.id.clone(), s.clone())).collect();
    if !state.grouped() {
        return flat_view(state, &groups, &by_key, panes, now);
    }
    groups
        .iter()
        .map(|g| {
            let sessions = g.session_ids.iter().filter_map(|id| by_key.get(id)).map(|s| session_view(s, state, panes, now)).collect();
            group_view(g, sessions, state, &home)
        })
        .collect()
}

fn flat_view(state: &AppState, groups: &[CoreGroup], by_key: &HashMap<String, CoreSession>, panes: &HashMap<String, Entity<TerminalPane>>, now: f64) -> Vec<Group> {
    let order = state.settings.sidebar_flat_order();
    let rows = SidebarFlat::rows(groups, by_key, state.sort(), &order);
    let sessions: Vec<Session> = rows.iter().map(|r| session_view(r.session, state, panes, now)).collect();
    vec![Group { id: String::new(), name: trd!("Sessions"), color: 0x6c7086, path: String::new(), cwd: String::new(), favorite: false, collapsed: false, sessions }]
}

/// Alle Sessions in Anzeigereihenfolge als (Gruppe, Session).
pub fn flat_index(groups: &[Group]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        out.extend((0..group.sessions.len()).map(|s| (g, s)));
    }
    out
}

/// Position einer Session in der Ansicht.
pub fn position(groups: &[Group], key: &str) -> Option<(usize, usize)> {
    groups.iter().enumerate().find_map(|(g, group)| group.sessions.iter().position(|s| s.key == key).map(|s| (g, s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_with_fallback() {
        assert_eq!(parse_color("#fab387"), 0xfab387);
        assert_eq!(parse_color("fab387"), 0xfab387);
        assert_eq!(parse_color("rot"), 0x6c7086);
        assert_eq!(parse_color("#fff"), 0x6c7086);
    }

    #[test]
    fn status_without_process_is_detached() {
        assert_eq!(Status::of(None), Status::Detached);
        assert_eq!(Status::of(Some(SessionStatus::Waiting)), Status::Waiting);
    }
}
