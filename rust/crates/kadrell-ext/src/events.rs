//! Ereignisse der App an die Extensions (`session.new`, `session.status`, `session.remove`) aus dem Vergleich zweier
//! Session-Stände. `app.ready`, `app.locale`, `session.focus` und `palette.select` schickt der Manager direkt.

use std::collections::HashMap;

/// Was die Extension-Schicht über eine Session wissen muss, um Änderungen zu erkennen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionState {
    pub id: String,
    /// `working`, `waiting` oder `idle`; nur dessen Wechsel löst `session.status` aus.
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEvent {
    /// `session.new`, `session.status` oder `session.remove`.
    pub event: &'static str,
    pub id: String,
}

/// Vergleicht mit dem letzten Stand. `session.status` nur bei geändertem Status, nicht bei jedem neuen Titel oder Branch.
/// Reihenfolge: neue und geänderte in Reihenfolge von `new`, danach die verschwundenen nach Id sortiert.
pub fn session_events(
    old: &HashMap<String, SessionState>,
    new: &[SessionState],
) -> Vec<SessionEvent> {
    let mut events: Vec<SessionEvent> = new
        .iter()
        .filter_map(|s| match old.get(&s.id) {
            None => Some(SessionEvent {
                event: "session.new",
                id: s.id.clone(),
            }),
            Some(before) if before.status == s.status => None,
            Some(_) => Some(SessionEvent {
                event: "session.status",
                id: s.id.clone(),
            }),
        })
        .collect();
    let mut gone: Vec<&String> = old
        .keys()
        .filter(|k| !new.iter().any(|s| &s.id == *k))
        .collect();
    gone.sort();
    events.extend(gone.into_iter().map(|id| SessionEvent {
        event: "session.remove",
        id: id.clone(),
    }));
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str, status: &str) -> SessionState {
        SessionState {
            id: id.into(),
            status: status.into(),
        }
    }

    #[test]
    fn session_events_diff() {
        let old: HashMap<String, SessionState> =
            [s("a", "idle"), s("b", "working"), s("c", "idle")]
                .into_iter()
                .map(|x| (x.id.clone(), x))
                .collect();
        let events = session_events(
            &old,
            &[s("a", "idle"), s("b", "waiting"), s("d", "working")],
        );
        let text: Vec<String> = events
            .iter()
            .map(|e| format!("{} {}", e.event, e.id))
            .collect();
        assert_eq!(
            text,
            ["session.status b", "session.new d", "session.remove c"]
        );
    }
}
