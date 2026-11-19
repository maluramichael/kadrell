//! Ziele (`-t`) auflösen wie `ControlTarget` auf dem Mac: exakt vor Anfang, Anfang und Name gleichrangig,
//! mehrdeutig ist ein Fehler. Dazu die Freigabeprüfung für Sessions, die andere Sessions steuern.
//! Die Meldungen gehören zum CLI-Vertrag (`docs/cli-contract.md`) und bleiben wortgleich.

use crate::group::Group;
use crate::session::Session;
use std::fmt;

pub use crate::paths::resolve_path;

/// Fehlermeldung eines Ziels, so wie sie als `kadrell: <meldung>` auf stderr erscheint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetError(pub String);

impl TargetError {
    fn new(message: impl Into<String>) -> TargetError {
        TargetError(message.into())
    }
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TargetError {}

/// Ergebnis von [`resolve_session_or_group`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target<'a> {
    Session(&'a Session),
    Group(&'a Group),
}

/// Die ersten acht Zeichen, wie die Meldungen sie zeigen.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Löst ein Session-Ziel auf: Key, sessionId, deren Anfang oder Titel. Ohne `t` gilt die aufrufende Session
/// (`caller`, Key aus `KADRELL_SESSION_KEY`), sonst die fokussierte.
pub fn resolve_session<'a>(t: Option<&str>, sessions: &'a [Session], caller: Option<&str>, focused: Option<&str>) -> Result<&'a Session, TargetError> {
    let Some(t) = t else {
        return [caller, focused]
            .into_iter()
            .flatten()
            .find_map(|key| sessions.iter().find(|s| s.id == key))
            .ok_or_else(|| TargetError::new("keine Session angegeben (-t) und keine fokussiert"));
    };
    if let Some(s) = sessions.iter().find(|s| s.id == t || s.session_id == t) {
        return Ok(s);
    }
    let lower = t.to_lowercase();
    let hits: Vec<&Session> = sessions.iter().filter(|s| s.id.starts_with(t) || s.session_id.starts_with(t) || s.title().to_lowercase() == lower).collect();
    match hits.as_slice() {
        [] => Err(TargetError::new(format!("keine Session „{t}“"))),
        [one] => Ok(one),
        many => Err(TargetError::new(format!("„{t}“ ist mehrdeutig: {}", many.iter().map(|s| short(&s.id)).collect::<Vec<_>>().join(", ")))),
    }
}

/// Löst ein Gruppen-Ziel auf: Id, deren Anfang oder Name. Ohne `t` die Gruppe der aufrufenden oder fokussierten
/// Session.
pub fn resolve_group<'a>(
    t: Option<&str>,
    groups: &'a [Group],
    sessions: &[Session],
    caller: Option<&str>,
    focused: Option<&str>,
) -> Result<&'a Group, TargetError> {
    let Some(t) = t else {
        let s = resolve_session(None, sessions, caller, focused)?;
        return groups.iter().find(|g| g.session_ids.contains(&s.id)).ok_or_else(|| TargetError::new("Session ohne Gruppe"));
    };
    if let Some(g) = groups.iter().find(|g| g.id == t) {
        return Ok(g);
    }
    let lower = t.to_lowercase();
    let hits: Vec<&Group> = groups.iter().filter(|g| g.id.starts_with(t) || g.name.to_lowercase() == lower).collect();
    match hits.as_slice() {
        [] => Err(TargetError::new(format!("keine Gruppe „{t}“"))),
        [one] => Ok(one),
        many => Err(TargetError::new(format!(
            "„{t}“ ist mehrdeutig: {}",
            many.iter().map(|g| format!("{} ({})", g.name, short(&g.id))).collect::<Vec<_>>().join(", ")
        ))),
    }
}

/// `select -t`: Session oder Gruppe. Passt beides, muss der Aufrufer genauer werden.
pub fn resolve_session_or_group<'a>(
    t: Option<&str>,
    groups: &'a [Group],
    sessions: &'a [Session],
    caller: Option<&str>,
    focused: Option<&str>,
) -> Result<Target<'a>, TargetError> {
    let Some(name) = t else { return resolve_session(None, sessions, caller, focused).map(Target::Session) };
    match (resolve_session(t, sessions, caller, focused), resolve_group(t, groups, sessions, caller, focused)) {
        (Ok(s), Err(_)) => Ok(Target::Session(s)),
        (Err(_), Ok(g)) => Ok(Target::Group(g)),
        (Ok(_), Ok(_)) => Err(TargetError::new(format!("„{name}“ passt auf eine Session und eine Gruppe, bitte Key oder Id angeben"))),
        // Immer die Meldung der Session-Suche („mehrdeutig“, „keine Session „x““): Swift wirft sie vor dem letzten throw.
        (Err(e), Err(_)) => Err(e),
    }
}

/// Ohne Freigabe darf eine Kachel nur Sessions ihrer eigenen Gruppe (und sich selbst) steuern und lesen. Aufrufe von
/// außerhalb (`caller` leer) sind nicht eingeschränkt. `others_allowed` ist die Einstellung
/// `control.otherSessions`.
pub fn allowed_session(session: &str, groups: &[Group], caller: Option<&str>, others_allowed: bool) -> bool {
    let Some(caller) = caller.filter(|c| !others_allowed && *c != session) else { return true };
    groups.iter().any(|g| g.session_ids.iter().any(|s| s == caller) && g.session_ids.iter().any(|s| s == session))
}

/// Wie [`allowed_session`] für eine ganze Gruppe: nur die eigene.
pub fn allowed_group(group: &Group, caller: Option<&str>, others_allowed: bool) -> bool {
    let Some(caller) = caller.filter(|_| !others_allowed) else { return true };
    group.session_ids.iter().any(|s| s == caller)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(id: &str, session_id: &str, name: &str) -> Session {
        Session::new(id, "/p", 0.0, session_id, name)
    }

    fn sessions() -> Vec<Session> {
        vec![s("aaaa1111-0000", "aaaa1111-0000", "Refactor"), s("aaaa2222-0000", "cccc", "Tests")]
    }

    fn id_of(r: Result<&Session, TargetError>) -> String {
        r.unwrap().id.clone()
    }

    #[test]
    fn resolve_session_exact_before_prefix() {
        let all = sessions();
        assert_eq!(id_of(resolve_session(Some("aaaa2"), &all, None, None)), "aaaa2222-0000");
        assert_eq!(id_of(resolve_session(Some("cccc"), &all, None, None)), "aaaa2222-0000");
        assert_eq!(id_of(resolve_session(Some("refactor"), &all, None, None)), "aaaa1111-0000");
        assert_eq!(id_of(resolve_session(Some("aaaa1111-0000"), &all, None, None)), "aaaa1111-0000");
        let ambiguous = resolve_session(Some("aaaa"), &all, None, None).unwrap_err();
        assert_eq!(ambiguous.0, "„aaaa“ ist mehrdeutig: aaaa1111, aaaa2222");
        assert_eq!(resolve_session(Some("zzz"), &all, None, None).unwrap_err().0, "keine Session „zzz“");
    }

    #[test]
    fn resolve_session_without_target_uses_caller_then_focus() {
        let all = sessions();
        let (a, b) = (all[0].id.as_str(), all[1].id.as_str());
        assert_eq!(id_of(resolve_session(None, &all, Some(b), Some(a))), b);
        assert_eq!(id_of(resolve_session(None, &all, Some("weg"), Some(a))), a);
        assert_eq!(resolve_session(None, &all, None, None).unwrap_err().0, "keine Session angegeben (-t) und keine fokussiert");
    }

    #[test]
    fn exact_id_wins_over_prefix_of_another() {
        let all = vec![s("ab", "ab", "x"), s("abc", "abc", "y")];
        assert_eq!(id_of(resolve_session(Some("ab"), &all, None, None)), "ab");
    }

    #[test]
    fn resolve_group_and_either() {
        let all = [s("s1", "s1", "api")];
        let groups = [Group::new("g1aa", "API", "#fff", "/p", &["s1"]), Group::new("g2bb", "Web", "#fff", "/w", &[])];
        let g = resolve_group(None, &groups, &all, Some("s1"), None).unwrap();
        assert_eq!(g.id, "g1aa");
        assert_eq!(resolve_group(Some("web"), &groups, &all, None, None).unwrap().id, "g2bb");
        assert_eq!(resolve_group(Some("nix"), &groups, &all, None, None).unwrap_err().0, "keine Gruppe „nix“");
        assert_eq!(resolve_group(Some("g"), &groups, &all, None, None).unwrap_err().0, "„g“ ist mehrdeutig: API (g1aa), Web (g2bb)");
        let lonely = [Group::new("g9", "Leer", "#fff", "/x", &[])];
        assert_eq!(resolve_group(None, &lonely, &all, Some("s1"), None).unwrap_err().0, "Session ohne Gruppe");
        match resolve_session_or_group(Some("g2"), &groups, &all, None, None).unwrap() {
            Target::Group(g) => assert_eq!(g.id, "g2bb"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(resolve_session_or_group(Some("s1"), &groups, &all, None, None), Ok(Target::Session(_))));
        let both = resolve_session_or_group(Some("api"), &groups, &all, None, None).unwrap_err();
        assert!(both.0.contains("passt auf eine Session und eine Gruppe"), "„api“ ist Session-Titel und Gruppenname zugleich");
        assert_eq!(resolve_session_or_group(Some("zzz"), &groups, &all, None, None).unwrap_err().0, "keine Session „zzz“");
        assert!(matches!(resolve_session_or_group(None, &groups, &all, Some("s1"), None), Ok(Target::Session(_))));
        // Mehrdeutig bleibt mehrdeutig (Meldung der Session-Suche), nicht „keine Session“.
        let twice = resolve_session_or_group(Some("s"), &groups, &all, None, None);
        if let Err(e) = twice {
            assert!(e.0.contains("mehrdeutig") || e.0.contains("passt auf"), "{}", e.0);
        }
    }

    #[test]
    fn control_policy_limits_callers_to_their_group() {
        let g1 = Group::new("g1", "A", "", "/a", &["s1", "s2"]);
        let g2 = Group::new("g2", "B", "", "/b", &["s3"]);
        let groups = [g1.clone(), g2.clone()];
        assert!(allowed_session("s3", &groups, Some("s1"), true), "freigegeben");
        assert!(allowed_session("s3", &groups, None, false), "Aufruf von außen");
        assert!(allowed_session("s1", &groups, Some("s1"), false), "sich selbst");
        assert!(allowed_session("s2", &groups, Some("s1"), false), "eigene Gruppe");
        assert!(!allowed_session("s3", &groups, Some("s1"), false));
        assert!(allowed_group(&g2, Some("s1"), true));
        assert!(allowed_group(&g1, Some("s1"), false));
        assert!(!allowed_group(&g2, Some("s1"), false));
        assert!(allowed_group(&g2, None, false));
    }
}
