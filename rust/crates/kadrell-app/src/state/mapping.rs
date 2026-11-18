//! Abbildung zwischen dem Kern (`kadrell_core::Session`) und dem Zustand des Pollers (`SessionRecord` aus
//! `kadrell-claude`). Der Kern speichert, das Register hält den Live-Zustand; nach jedem Poll und jeder Meldung
//! gehen die Live-Felder zurück in die Sessions des Kerns.

use kadrell_claude::registry::{SessionKind, SessionRecord};
use kadrell_core::Session;

/// Art der Kachel aus den Merkmalen der Session.
pub fn kind_of(s: &Session) -> SessionKind {
    if s.is_remote() {
        SessionKind::Remote
    } else if s.is_shell() {
        SessionKind::Shell
    } else if s.is_pi() {
        SessionKind::Pi
    } else {
        SessionKind::Claude
    }
}

/// Eintrag für das Register: gespeicherte Felder und Live-Zustand der Session.
pub fn record_of(s: &Session) -> SessionRecord {
    SessionRecord {
        id: s.id.clone(),
        cwd: s.cwd.clone(),
        started_at: s.started_at,
        session_id: s.session_id.clone(),
        name: s.name.clone(),
        custom_name: s.custom_name.clone(),
        local_title: s.local_title.clone(),
        model: s.model.clone(),
        kind: kind_of(s),
        raw_status: s.raw_status.clone(),
        pid: s.pid.and_then(|p| u32::try_from(p).ok()),
        waiting_for: s.waiting_for.clone(),
        branch: s.branch.clone(),
        active_worktree: s.active_worktree.clone(),
        first_prompt: s.first_prompt.clone(),
        has_running_shell: s.has_running_shell,
    }
}

/// Übernimmt Live-Zustand und die vom Poll geschriebenen gespeicherten Felder (`sessionId`, `name`, Ordner einer
/// Shell) aus dem Register in die Session. Gibt zurück, ob sich ein gespeichertes Feld geändert hat.
pub fn apply_record(s: &mut Session, r: &SessionRecord) -> bool {
    let follow_cwd = s.is_shell() && s.cwd != r.cwd;
    let stored_changed = s.session_id != r.session_id || s.name != r.name || follow_cwd;
    s.session_id.clone_from(&r.session_id);
    s.name.clone_from(&r.name);
    if follow_cwd {
        s.cwd.clone_from(&r.cwd);
    }
    s.raw_status.clone_from(&r.raw_status);
    s.pid = r.pid.map(i64::from);
    s.waiting_for.clone_from(&r.waiting_for);
    s.branch.clone_from(&r.branch);
    s.active_worktree.clone_from(&r.active_worktree);
    s.first_prompt.clone_from(&r.first_prompt);
    s.has_running_shell = r.has_running_shell;
    stored_changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_follows_the_key_and_host() {
        let claude = Session::new("a", "/p", 1.0, "a", "");
        assert_eq!(kind_of(&claude), SessionKind::Claude);
        let shell = Session::new(&Session::new_shell_id(), "/p", 1.0, "s", "");
        assert_eq!(kind_of(&shell), SessionKind::Shell);
        let mut remote = Session::new("ssh-x", "/p", 1.0, "r", "");
        remote.host = Some("box".into());
        assert_eq!(kind_of(&remote), SessionKind::Remote);
        assert_eq!(kind_of(&Session::new(&Session::new_pi_id(), "/p", 1.0, "p", "")), SessionKind::Pi);
    }

    #[test]
    fn record_round_trip_keeps_stored_and_live_fields() {
        let mut s = Session::new("k", "/p", 5.0, "sid", "Titel");
        s.custom_name = Some("Mein".into());
        s.raw_status = Some("busy".into());
        s.pid = Some(42);
        s.branch = Some("master".into());
        let r = record_of(&s);
        assert_eq!((r.pid, r.raw_status.as_deref(), r.custom_name.as_deref()), (Some(42), Some("busy"), Some("Mein")));
        let mut back = Session::new("k", "/p", 5.0, "sid", "Titel");
        assert!(!apply_record(&mut back, &r));
        assert_eq!((back.pid, back.branch.as_deref(), back.raw_status.as_deref()), (Some(42), Some("master"), Some("busy")));
    }

    #[test]
    fn changed_session_id_or_name_counts_as_stored_change() {
        let mut s = Session::new("k", "/p", 5.0, "alt", "");
        let mut r = record_of(&s);
        r.session_id = "neu".into();
        r.name = "Ein Titel".into();
        assert!(apply_record(&mut s, &r));
        assert_eq!((s.session_id.as_str(), s.name.as_str()), ("neu", "Ein Titel"));
    }
}
