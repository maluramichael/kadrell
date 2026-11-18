//! Hintergrund-Sessions aus `claude --bg` beim Start übernehmen (Swift: `offerAdopt` im AppDelegate). Das Angebot
//! kommt nur im Standardprofil. Ablauf: `claude agents` zählt, was anderswo läuft, `claude stop` hält die
//! Hintergrund-Sessions an, danach setzt Kadrell sie als eigenen Prozess mit `--resume` fort. Dieses Modul baut Texte
//! und Sessions; der Aufruf von `claude` läuft über `kadrell_claude::background_shells` (Tests mit Fake-Runner dort).

use std::collections::HashSet;

use kadrell_claude::background_shells::{self, AdoptPlan, AdoptedSession};
use kadrell_claude::cli::{ClaudeCli, CommandRunner};
use kadrell_core::Session;
use kadrell_i18n::trd;

/// Was der Dialog zeigt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub title: String,
    pub text: String,
    /// Eine Session arbeitet gerade: Stoppen bricht Arbeit ab, deshalb gilt nur ⌘⏎ (Strg+⏎), nicht ⏎ allein.
    pub destructive: bool,
}

/// Das Angebot zum Plan, `None` ohne Kandidaten.
pub fn offer(plan: &AdoptPlan) -> Option<Offer> {
    if plan.candidates.is_empty() {
        return None;
    }
    let list: Vec<String> = plan
        .candidates
        .iter()
        .map(|c| format!("· {}{}", c.agent.name, if c.busy { trd!(" (arbeitet gerade)") } else { String::new() }))
        .collect();
    let intro = trd!("Kadrell startet Claude jetzt selbst statt mit claude --bg. Diese Sessions werden mit claude stop angehalten (laufende Arbeit bricht ab) und hier fortgesetzt:");
    Some(Offer { title: trd!("%lld Hintergrund-Session(s) übernehmen?", plan.candidates.len() as i64), text: format!("{intro}\n{}", list.join("\n")), destructive: plan.destructive() })
}

/// Ein Fehlschlag-Bericht für alle: eine Meldung statt einer je Session, die sich sonst gegenseitig verdrängen.
pub fn failure_report(failures: &[String]) -> Option<(String, String)> {
    (!failures.is_empty()).then(|| (trd!("%lld Session(s) konnten nicht übernommen werden", failures.len() as i64), failures.join("\n")))
}

/// Die übernommenen Sessions als Einträge der Session-Liste: die Kurz-Id bleibt Schlüssel (Gruppen und Auswahl passen).
pub fn sessions_of(adopted: &[AdoptedSession]) -> Vec<Session> {
    adopted.iter().map(|a| Session::new(&a.id, &a.cwd, a.started_at, &a.session_id, &a.name)).collect()
}

/// Sessions, die Kadrell schon hat (`sessionId`), damit sie nicht als fremd gelten.
pub fn owned_ids(sessions: &[Session]) -> HashSet<String> {
    sessions.iter().map(|s| s.session_id.clone()).collect()
}

/// Erfasst fremde Claude-Sessions (`claude agents`). `Err`: die Meldung für die Statusleiste.
pub fn scan(cli: &ClaudeCli, runner: &dyn CommandRunner, owned: &HashSet<String>, default_profile: bool) -> Result<AdoptPlan, String> {
    let agents = cli.agents(runner).map_err(|e| format!("{}: {}", cli.binary, kadrell_claude::cli::CliError::first_line(&e.output)))?;
    Ok(background_shells::plan_adoption(&agents, owned, default_profile))
}

/// Hinweis im Leerzustand: wie viele Sessions interaktiv in anderen Terminals laufen. `None` bei null.
pub fn other_terminals_hint(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some(trd!("1 Claude-Session läuft interaktiv in anderen Terminals")),
        n => Some(trd!("%lld Claude-Sessions laufen interaktiv in anderen Terminals", n as i64)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_claude::agent::AgentInfo;
    use kadrell_claude::background_shells::AdoptCandidate;

    fn agent(short: &str, name: &str) -> AgentInfo {
        AgentInfo {
            short_id: Some(short.into()),
            cwd: "C:/dev/x".into(),
            kind: "background".into(),
            started_at: 5.0,
            session_id: format!("sid-{short}"),
            name: name.into(),
            state: None,
            status: None,
            pid: Some(1),
            waiting_for: None,
            status_updated_at: None,
        }
    }

    fn plan(busy: &[bool]) -> AdoptPlan {
        let candidates = busy.iter().enumerate().map(|(i, b)| AdoptCandidate { agent: agent(&format!("a{i}"), &format!("job{i}")), busy: *b }).collect();
        AdoptPlan { other_interactive_count: 0, candidates }
    }

    #[test]
    fn no_candidates_no_offer() {
        assert_eq!(offer(&plan(&[])), None);
    }

    #[test]
    fn the_offer_lists_every_session_and_marks_busy_ones() {
        let o = offer(&plan(&[false, true])).unwrap();
        assert!(o.title.contains('2'), "{}", o.title);
        assert!(o.text.contains("· job0\n"), "{}", o.text);
        assert!(o.text.lines().any(|l| l.starts_with("· job1 (")), "{}", o.text);
        assert!(o.destructive);
        assert!(!offer(&plan(&[false])).unwrap().destructive, "nur wartende Sessions: ⏎ genügt");
    }

    #[test]
    fn adopted_sessions_keep_the_short_id_as_key() {
        let adopted = [AdoptedSession { id: "ab12cd34".into(), cwd: "C:/dev/x".into(), started_at: 7.0, session_id: "sid".into(), name: "job".into() }];
        let sessions = sessions_of(&adopted);
        assert_eq!((sessions[0].id.as_str(), sessions[0].session_id.as_str(), sessions[0].name.as_str()), ("ab12cd34", "sid", "job"));
        assert_eq!(owned_ids(&sessions), HashSet::from(["sid".to_string()]));
    }

    #[test]
    fn failures_become_one_report() {
        assert_eq!(failure_report(&[]), None);
        let (title, body) = failure_report(&["· a: kaputt".into(), "· b: weg".into()]).unwrap();
        assert!(title.contains('2'));
        assert_eq!(body, "· a: kaputt\n· b: weg");
    }

    #[test]
    fn the_hint_counts_interactive_sessions_elsewhere() {
        assert_eq!(other_terminals_hint(0), None);
        assert!(other_terminals_hint(1).unwrap().starts_with('1'));
        assert!(other_terminals_hint(3).unwrap().starts_with('3'));
    }
}
