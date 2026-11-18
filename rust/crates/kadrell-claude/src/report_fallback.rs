//! Abgleich für Kacheln, die sich per Hook melden (Swift: `Services/ReportFallback.swift`). Claude Code feuert bei einem
//! Abbruch durch den Nutzer (Esc, Strg+C, auch mitten in einem Werkzeug) keinen `Stop`-Hook und auch kein späteres
//! `idle_prompt` (mit 2.1.314 geprüft, `docs/kadrell-verifikation.md`): die gemeldete Kachel bliebe ewig „arbeitet“.
//! `sessions/<pid>.json` dagegen springt beim Abbruch sofort auf `idle` und trägt in `statusUpdatedAt`, wann. Gelesen
//! wird die Datei nur für gemeldete Kacheln, die gerade nicht untätig sind.

use crate::agent::{AgentInfo, Status, map_status};

/// So lange darf eine Meldung „arbeitet“ ohne passenden Stand in der Datei bleiben (5 s): Claude schreibt `busy` im
/// selben Moment, in dem es den `UserPromptSubmit`-Hook startet. Steht danach noch das alte `idle` da, kam die Eingabe
/// nie bei Claude an (z. B. Esc, während Hooks die Eingabe noch prüfen).
pub const GRACE_MS: f64 = 5_000.0;

/// Neuer Rohstatus samt `waitingFor` für eine gemeldete Kachel, nil = die Meldung gilt weiter. Alle Zeiten in ms seit
/// 1970. Ohne `statusUpdatedAt` (ältere CLI) bleibt es bei der Meldung.
pub fn correction(current: Status, reported_at: f64, file: Option<&AgentInfo>, now: f64) -> Option<(String, Option<String>)> {
    if current == Status::Idle {
        return None;
    }
    let file = file?;
    let raw = file.status.as_deref()?;
    let written = file.status_updated_at?;
    let status = map_status(None, Some(raw));
    if status == current {
        return None;
    }
    // Die Datei ist neuer als die letzte Meldung: Abbruch (idle), Freigabe erteilt (busy) oder abgelehnt (idle).
    if written > reported_at {
        return Some((raw.to_string(), if status == Status::Waiting { file.waiting_for.clone() } else { None }));
    }
    // Gemeldet „arbeitet“, die Datei steht aber noch auf dem `idle` von davor: die Runde hat nie begonnen.
    if current == Status::Running && status == Status::Idle && now - reported_at > GRACE_MS {
        return Some((raw.to_string(), None));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(status: Option<&str>, at: Option<f64>, waiting_for: Option<&str>) -> AgentInfo {
        AgentInfo {
            short_id: None,
            cwd: "/p".into(),
            kind: "interactive".into(),
            started_at: 1.0,
            session_id: "s".into(),
            name: "n".into(),
            state: None,
            status: status.map(String::from),
            pid: Some(1),
            waiting_for: waiting_for.map(String::from),
            status_updated_at: at,
        }
    }

    fn status_of(fix: Option<(String, Option<String>)>) -> Option<String> {
        fix.map(|f| f.0)
    }

    #[test]
    fn newer_idle_ends_working() {
        let fix = correction(Status::Running, 1_000.0, Some(&file(Some("idle"), Some(2_000.0), None)), 2_100.0).unwrap();
        assert_eq!(fix, ("idle".to_string(), None));
    }

    #[test]
    fn newer_idle_ends_waiting() {
        assert_eq!(status_of(correction(Status::Waiting, 1_000.0, Some(&file(Some("idle"), Some(2_000.0), None)), 2_100.0)).as_deref(), Some("idle"));
    }

    #[test]
    fn newer_busy_ends_waiting_after_approval() {
        assert_eq!(status_of(correction(Status::Waiting, 1_000.0, Some(&file(Some("busy"), Some(2_000.0), None)), 2_100.0)).as_deref(), Some("busy"));
    }

    #[test]
    fn newer_waiting_keeps_its_reason() {
        let fix = correction(Status::Running, 1_000.0, Some(&file(Some("waiting"), Some(2_000.0), Some("input needed"))), 2_100.0).unwrap();
        assert_eq!(fix, ("waiting".to_string(), Some("input needed".to_string())));
    }

    #[test]
    fn older_file_keeps_report_within_grace() {
        assert_eq!(correction(Status::Running, 10_000.0, Some(&file(Some("idle"), Some(9_000.0), None)), 10_000.0 + GRACE_MS - 1.0), None);
        assert_eq!(correction(Status::Running, 10_000.0, Some(&file(Some("busy"), Some(9_999.0), None)), 99_000.0), None, "busy von der Runde selbst");
    }

    #[test]
    fn older_idle_after_grace_means_turn_never_started() {
        let old_idle = file(Some("idle"), Some(9_000.0), None);
        assert_eq!(status_of(correction(Status::Running, 10_000.0, Some(&old_idle), 10_000.0 + GRACE_MS + 1.0)).as_deref(), Some("idle"));
        assert_eq!(correction(Status::Waiting, 10_000.0, Some(&old_idle), 99_000.0), None, "Warten kann ohne Dateistand gemeldet sein (Notification), bleibt");
    }

    #[test]
    fn idle_or_unknown_leaves_report_alone() {
        assert_eq!(correction(Status::Idle, 1_000.0, Some(&file(Some("busy"), Some(2_000.0), None)), 2_100.0), None);
        assert_eq!(correction(Status::Running, 1_000.0, None, 99_000.0), None);
        assert_eq!(correction(Status::Running, 1_000.0, Some(&file(Some("idle"), None, None)), 99_000.0), None, "ältere CLI ohne Zeitstempel");
        assert_eq!(correction(Status::Running, 1_000.0, Some(&file(Some("busy"), Some(2_000.0), None)), 2_100.0), None, "gleicher Status");
    }
}
