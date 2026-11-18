//! Texte für Bildschirmleser (`UI/Accessibility.swift`, `SidebarView.accessibilityChildren`,
//! `CellView.accessibilityChildren`): der Baum ist eine Gliederung (`Role::Tree`) mit Zeilen (`TreeItem`, Gruppen auf
//! Ebene 1, Sessions auf Ebene 2 oder 1 ohne Gruppierung), die Kacheln sind Gruppen (`Role::Group`) mit dem Titel als
//! Beschriftung. Status, „neu“ und „ausgewählt“ stehen im Wert, nicht im Namen. GPUI reicht Rolle und Beschriftung über
//! accesskit an UI Automation (Windows) und NSAccessibility (Mac) weiter.

use kadrell_i18n::trd;

use crate::ui::model::Status;

/// Vorgelesener Status einer Session: ohne laufenden Prozess „beendet“ (`ended`) oder „nicht gestartet“, bei fertiger
/// Session mit noch laufendem Kommando aus dem Bash-Tool dieses statt „fertig“ (Wartet und Fehler bleiben stehen).
pub fn spoken(status: Status, ended: bool, shell_running: bool) -> String {
    match status {
        Status::Detached if ended => trd!("beendet"),
        Status::Detached => trd!("nicht gestartet"),
        Status::Idle if shell_running => trd!("Kommando läuft noch"),
        Status::Running => trd!("arbeitet"),
        Status::Waiting => trd!("wartet"),
        Status::Idle => trd!("fertig"),
        Status::Error => trd!("Fehler"),
    }
}

/// „Gruppe Projekt, 2 Sessions“, bei einer „Gruppe Projekt, 1 Session“.
pub fn group_label(name: &str, sessions: usize) -> String {
    if sessions == 1 { trd!("Gruppe %@, 1 Session", name) } else { trd!("Gruppe %@, %lld Sessions", name, sessions) }
}

/// „Session Alpha“: rein identifizierend, der Rest steht im Wert.
pub fn session_label(title: &str) -> String {
    trd!("Session %@", title)
}

/// Wert einer Zeile: Status, dazu „neu“ und „ausgewählt“, mit Komma getrennt.
pub fn session_value(spoken: &str, unread: bool, selected: bool) -> String {
    let mut parts = vec![spoken.to_string()];
    if unread {
        parts.push(trd!("neu"));
    }
    if selected {
        parts.push(trd!("ausgewählt"));
    }
    parts.join(", ")
}

/// Beschriftung der Titelzeile einer Kachel: Titel, Status, Gruppe und Zweig.
pub fn tile_label(title: &str, spoken: &str, group: &str, branch: Option<&str>) -> String {
    let parts = [Some(title), Some(spoken), Some(group).filter(|g| !g.is_empty()), branch];
    parts.into_iter().flatten().collect::<Vec<_>>().join(", ")
}

/// Ebene einer Zeile in der Gliederung: Gruppen oben (1), Sessions darunter (2), ohne Gruppierung stehen sie auf 1.
pub fn level(group_row: bool, grouped: bool) -> usize {
    if group_row || !grouped { 1 } else { 2 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `testSidebarRows`: Beschriftungen und Werte der Zeilen.
    #[test]
    fn tree_rows_read_like_the_mac_app() {
        assert_eq!(group_label("Projekt", 2), "Gruppe Projekt, 2 Sessions");
        assert_eq!(group_label("Projekt", 1), "Gruppe Projekt, 1 Session");
        assert_eq!(session_label("Alpha"), "Session Alpha");
        let status = spoken(Status::Detached, false, false);
        assert_eq!(status, "nicht gestartet");
        assert_eq!(session_value(&status, false, false), "nicht gestartet");
        assert_eq!(session_value(&status, true, false), "nicht gestartet, neu");
        assert_eq!(session_value(&spoken(Status::Waiting, false, false), true, true), "wartet, neu, ausgewählt");
    }

    #[test]
    fn spoken_status_matches_accessibility_swift() {
        assert_eq!(spoken(Status::Running, false, false), "arbeitet");
        assert_eq!(spoken(Status::Waiting, false, false), "wartet");
        assert_eq!(spoken(Status::Idle, false, false), "fertig");
        assert_eq!(spoken(Status::Error, false, false), "Fehler");
        assert_eq!(spoken(Status::Detached, true, false), "beendet");
        assert_eq!(spoken(Status::Idle, false, true), "Kommando läuft noch");
        assert_eq!(spoken(Status::Waiting, false, true), "wartet", "Warten geht vor der laufenden Shell");
        assert_eq!(spoken(Status::Error, false, true), "Fehler");
    }

    #[test]
    fn disclosure_levels() {
        assert_eq!([level(true, true), level(false, true), level(false, false)], [1, 2, 1]);
    }

    /// `testCellHeader`: Titel, Status, Gruppe und Zweig.
    #[test]
    fn tile_header_reads_title_status_group_branch() {
        assert_eq!(tile_label("Alpha", "nicht gestartet", "Projekt", Some("master")), "Alpha, nicht gestartet, Projekt, master");
        assert_eq!(tile_label("Alpha", "fertig", "", None), "Alpha, fertig");
    }
}
