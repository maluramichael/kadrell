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
    /// Der Prozess ist beendet oder gestoppt (für den vorgelesenen Status).
    pub ended: bool,
    /// Ein Kommando aus dem Bash-Tool läuft noch (`hasRunningShell`): fertig wird dann zur Shell-Farbe und atmet.
    pub shell_running: bool,
    pub is_remote: bool,
    pub is_pi: bool,
    /// Letzte Antwort von Claude für die Nachrichtenzeile, nur mit der Einstellung „letzte Nachricht zeigen“.
    pub message: Option<String>,
    /// Ohne Gruppierung: Name und Farbe der Gruppe für das Präfix „projekt › “ vor dem Titel.
    pub project: Option<(String, u32)>,
    pub pane: Option<Entity<TerminalPane>>,
    /// Wie die Kachel ihren Prozess zeigt (`ui::tile_look`).
    pub look: SessionLook,
}

/// Prozessseite einer Kachel: was `CellView.State` zusätzlich zur Session kennt.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct SessionLook {
    /// Der Ordner der Session fehlt (kein Prozess gestartet).
    pub missing_folder: bool,
    /// Startfehler: Exit-Code eines Prozesses, der kurz nach dem Start gestorben ist.
    pub exit_code: Option<i32>,
    /// Ein Prozess hängt an der Session (auch wenn sein Terminal in einem anderen Fenster liegt).
    pub attached: bool,
    /// Letzte Zeilen für Kacheln ohne Terminal.
    pub lines: Vec<String>,
    /// Ort der Session für „ORDNER FEHLT · <Ordner>“, mit `~`.
    pub short_cwd: String,
}

fn look_of(s: &CoreSession, state: &AppState) -> SessionLook {
    SessionLook {
        missing_folder: state.missing_folder(&s.id),
        exit_code: state.exit_code(&s.id),
        attached: state.mounted(&s.id),
        lines: state.snapshot(&s.id),
        short_cwd: folders::short_path(&s.cwd, &state.home()),
    }
}

impl Session {
    /// Farbe des Statuspunkts samt Shell-Farbe (`Theme.statusColor`).
    pub fn dot_color(&self, t: &Theme) -> Hsla {
        super::tree_rules::status_color(self.status, self.shell_running, t)
    }

    /// Farbe des atmenden Punkts, `None` = ruhiger Punkt (`Theme.activityColor`).
    pub fn activity_color(&self, t: &Theme) -> Option<Hsla> {
        super::tree_rules::activity_color(self.status, self.shell_running, t)
    }
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
    /// Host einer Remote-Gruppe (`ssh`/`tmux`): statt des Pfads steht dort „ssh · tmux“.
    pub host: Option<String>,
    pub sessions: Vec<Session>,
}

/// `#rrggbb` aus `groups.json` als Zahl, bei Unlesbarem ein neutrales Grau.
pub fn parse_color(text: &str) -> u32 {
    u32::from_str_radix(text.trim_start_matches('#'), 16).ok().filter(|_| text.trim_start_matches('#').len() == 6).unwrap_or(0x6c7086)
}

/// Ansicht ohne Rücksicht auf die Gruppe: Nachricht und Projektpräfix setzt der Aufrufer.
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
        ended: matches!(state.run(&s.id), crate::state::Run::Ended | crate::state::Run::Stopped),
        shell_running: s.has_running_shell,
        is_remote: s.is_remote(),
        is_pi: s.is_pi(),
        message: None,
        project: None,
        pane: panes.get(&s.id).cloned(),
        look: look_of(s, state),
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
        host: g.host.clone(),
        sessions,
    }
}

/// Gruppen mit der Sortierung der Leiste. Ohne Gruppierung steht eine einzige Gruppe mit allen Sessions da.
pub fn build(state: &AppState, panes: &HashMap<String, Entity<TerminalPane>>) -> Vec<Group> {
    let now = crate::state::now_secs();
    let home = state.home();
    let groups = state.sorted_groups();
    let by_key = first_wins(&state.store.sessions.sessions);
    let messages = messages(state);
    if !state.grouped() {
        return flat_view(state, &groups, &by_key, panes, now, &messages);
    }
    groups
        .iter()
        .map(|g| {
            let sessions = g.session_ids.iter().filter_map(|id| by_key.get(id)).map(|s| with_message(session_view(s, state, panes, now), &messages)).collect();
            group_view(g, sessions, state, &home)
        })
        .collect()
}

/// Sessions nach Id, bei doppelter Id gewinnt die erste (`uniquingKeysWith: { a, _ in a }`): eine doppelte Id darf
/// weder abstürzen noch die spätere Zeile bevorzugen.
fn first_wins(sessions: &[CoreSession]) -> HashMap<String, CoreSession> {
    let mut map = HashMap::new();
    for s in sessions {
        map.entry(s.id.clone()).or_insert_with(|| s.clone());
    }
    map
}

/// Letzte Antworten für die Nachrichtenzeile, leer ohne die Einstellung.
fn messages(state: &AppState) -> HashMap<String, String> {
    if !state.settings.show_last_message() {
        return HashMap::new();
    }
    state.registry.lock().unwrap_or_else(std::sync::PoisonError::into_inner).last_messages.clone()
}

fn with_message(mut s: Session, messages: &HashMap<String, String>) -> Session {
    s.message = messages.get(&s.key).filter(|m| !m.trim().is_empty()).cloned();
    s
}

fn flat_view(
    state: &AppState,
    groups: &[CoreGroup],
    by_key: &HashMap<String, CoreSession>,
    panes: &HashMap<String, Entity<TerminalPane>>,
    now: f64,
    messages: &HashMap<String, String>,
) -> Vec<Group> {
    let order = state.settings.sidebar_flat_order();
    let rows = SidebarFlat::rows(groups, by_key, state.sort(), &order);
    let sessions: Vec<Session> = rows
        .iter()
        .map(|r| {
            let mut s = with_message(session_view(r.session, state, panes, now), messages);
            s.project = Some((r.group.name.clone(), parse_color(&r.group.color)));
            s
        })
        .collect();
    vec![Group { id: String::new(), name: trd!("Sessions"), color: 0x6c7086, path: String::new(), cwd: String::new(), favorite: false, collapsed: false, host: None, sessions }]
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

/// Session für Tests anderer Module: fertig, angehängt, ohne Besonderheiten.
#[cfg(test)]
pub fn test_session(key: &str) -> Session {
    Session {
        key: key.into(),
        title: key.into(),
        status: Status::Idle,
        is_new: false,
        age: String::new(),
        branch: None,
        cwd: String::new(),
        is_shell: false,
        ended: false,
        shell_running: false,
        is_remote: false,
        is_pi: false,
        message: None,
        project: None,
        pane: None,
        look: Default::default(),
    }
}

/// Gruppe für Tests anderer Module.
#[cfg(test)]
pub fn test_group(id: &str, keys: &[&str]) -> Group {
    let sessions = keys.iter().map(|k| test_session(k)).collect();
    Group { id: id.into(), name: id.into(), color: 0, path: String::new(), cwd: String::new(), favorite: false, collapsed: false, host: None, sessions }
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

    /// `testDuplicateIdsDoNotCrash`-Verhalten: bei doppelter Id gewinnt die erste Session.
    #[test]
    fn duplicate_session_ids_first_wins() {
        let a = CoreSession::new("x", "/a", 1., "x", "erste");
        let b = CoreSession::new("x", "/b", 2., "x", "zweite");
        let map = first_wins(&[a, b]);
        assert_eq!(map.len(), 1);
        assert_eq!(map["x"].name, "erste");
    }

    #[test]
    fn message_only_when_not_blank() {
        let messages: HashMap<String, String> = [("a".to_string(), "Fertig?".to_string()), ("b".to_string(), "  ".to_string())].into();
        let mk = test_session;
        assert_eq!(with_message(mk("a"), &messages).message.as_deref(), Some("Fertig?"));
        assert_eq!(with_message(mk("b"), &messages).message, None);
        assert_eq!(with_message(mk("c"), &messages).message, None);
    }

    #[test]
    fn status_without_process_is_detached() {
        assert_eq!(Status::of(None), Status::Detached);
        assert_eq!(Status::of(Some(SessionStatus::Waiting)), Status::Waiting);
    }
}
