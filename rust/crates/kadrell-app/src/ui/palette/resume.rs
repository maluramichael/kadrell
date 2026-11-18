//! ⏎ auf einem Treffer der Verlaufssuche: läuft die Session schon in Kadrell, dorthin springen, sonst
//! `claude --resume` im Ordner ihrer ersten Eingabe starten (derselbe Weg wie `kadrell new --resume`). Die Entscheidung
//! ist eine reine Funktion (`AppDelegate+History.swift`: `resumeFromHistory`), die Shell führt sie aus.

use kadrell_control::ResumeBlocker;
use kadrell_i18n::trd;

/// Eine Session des Kerns, wie die Entscheidung sie braucht.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Known {
    /// Schlüssel in Kadrell.
    pub key: String,
    /// Aktuelle Konversation (kann nach `/clear` von der ursprünglichen abweichen).
    pub session_id: String,
}

/// Was die Shell tun soll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryAction {
    /// Die Session läuft in Kadrell: zu diesem Schlüssel springen.
    Jump(String),
    /// Neue Kachel mit `claude --resume <id>` in `cwd`.
    Resume { session_id: String, cwd: String },
    /// Geht nicht, Meldung für den Nutzer (schon übersetzt).
    Problem(String),
}

/// Treffer, wie die Palette ihn weitergibt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    pub session_id: String,
    pub cwd: String,
    pub title: String,
}

/// `folder_ok`: der Ordner existiert. `blocker`: Antwort von `ControlHandler::resume_blocker` für diese Konversation.
pub fn decide(pick: &Pick, known: &[Known], folder_ok: bool, blocker: Option<ResumeBlocker>, home: &str) -> HistoryAction {
    if let Some(k) = known.iter().find(|k| k.session_id == pick.session_id || k.key == pick.session_id) {
        return HistoryAction::Jump(k.key.clone());
    }
    if pick.cwd.is_empty() || !folder_ok {
        let short = crate::ui::folders::short_path(&pick.cwd.replace('\\', "/"), home);
        return HistoryAction::Problem(trd!("Den Ordner %@ gibt es nicht mehr.", short));
    }
    match blocker {
        Some(ResumeBlocker::Running(pid)) => HistoryAction::Problem(trd!("„%@“ läuft noch außerhalb dieses Kadrell (pid %lld). Erst dort beenden.", pick.title.clone(), pid)),
        Some(ResumeBlocker::NoTranscript) => HistoryAction::Problem(trd!("Das Transcript von „%@“ ist nicht mehr da.", pick.title.clone())),
        Some(ResumeBlocker::InKadrell) | None => HistoryAction::Resume { session_id: pick.session_id.clone(), cwd: pick.cwd.clone() },
    }
}

/// Überschrift der Meldung.
pub fn problem_title() -> String {
    trd!("Session lässt sich nicht fortsetzen")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick() -> Pick {
        kadrell_i18n::set_thread_language(kadrell_i18n::Language::De);
        Pick { session_id: "abc".into(), cwd: "C:/dev/kadrell".into(), title: "Palette".into() }
    }

    fn known(key: &str, sid: &str) -> Known {
        Known { key: key.into(), session_id: sid.into() }
    }

    #[test]
    fn running_in_kadrell_jumps_by_session_id_or_key() {
        let by_sid = [known("k1", "other"), known("k2", "abc")];
        assert_eq!(decide(&pick(), &by_sid, true, None, "C:/Users/m"), HistoryAction::Jump("k2".into()));
        // Nach `kadrell new --resume` ist der Schlüssel die Konversation selbst.
        assert_eq!(decide(&pick(), &[known("abc", "zzz")], true, None, "C:/Users/m"), HistoryAction::Jump("abc".into()));
    }

    #[test]
    fn jump_wins_over_a_missing_folder() {
        assert_eq!(decide(&pick(), &[known("k", "abc")], false, Some(ResumeBlocker::NoTranscript), ""), HistoryAction::Jump("k".into()));
    }

    #[test]
    fn otherwise_resumes_in_the_folder_of_the_first_input() {
        let want = HistoryAction::Resume { session_id: "abc".into(), cwd: "C:/dev/kadrell".into() };
        assert_eq!(decide(&pick(), &[known("k", "x")], true, None, ""), want);
        assert_eq!(decide(&pick(), &[], true, Some(ResumeBlocker::InKadrell), ""), want);
    }

    #[test]
    fn missing_folder_transcript_or_foreign_process_are_problems() {
        let problem = |a: HistoryAction| match a {
            HistoryAction::Problem(m) => m,
            other => panic!("kein Problem: {other:?}"),
        };
        assert!(problem(decide(&pick(), &[], false, None, "")).contains("C:/dev/kadrell"));
        let empty = Pick { cwd: String::new(), ..pick() };
        assert!(problem(decide(&empty, &[], true, None, "")).contains("gibt es nicht mehr"));
        assert!(problem(decide(&pick(), &[], true, Some(ResumeBlocker::NoTranscript), "")).contains("Palette"));
        let running = problem(decide(&pick(), &[], true, Some(ResumeBlocker::Running(4711)), ""));
        assert!(running.contains("4711") && running.contains("Palette"), "{running}");
    }
}
