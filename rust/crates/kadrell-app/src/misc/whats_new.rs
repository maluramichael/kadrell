//! Wann „Was ist neu“ von selbst erscheint (Swift: `showWhatsNewIfNeeded` im AppDelegate). Der erste Start zeigt nichts
//! (nichts davon war vorher da), jeder spätere Versionssprung einmal, Wegwerfprofile nie (sie erben den Lesestand des
//! Standardprofils, sonst öffnete jeder Teststart den Dialog).

use kadrell_core::Settings;
use serde_json::json;

use super::changelog;

/// Einstellung: zuletzt gezeigte Version.
pub const LAST_VERSION_KEY: &str = "whatsNew.lastVersion";
/// Einstellung: die App lief schon einmal (Swift: `helpShown`).
pub const SEEN_KEY: &str = "helpShown";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// Nichts zeigen, nichts merken.
    No,
    /// Erster Start: Version merken, nichts zeigen.
    FirstRun,
    /// Versionssprung: Version merken und zeigen.
    Show,
}

pub fn due(started_before: bool, last_seen: &str, version: &str, temporary: bool) -> Due {
    if temporary {
        return Due::No;
    }
    if !started_before {
        return Due::FirstRun;
    }
    if last_seen == version { Due::No } else { Due::Show }
}

/// Entscheidung aus den Einstellungen.
pub fn due_for(settings: &Settings, version: &str, temporary: bool) -> Due {
    due(settings.bool_or(SEEN_KEY, false), &settings.string_or(LAST_VERSION_KEY, ""), version, temporary)
}

/// Merkt Start und Version in den Einstellungen.
pub fn remember(settings: &mut Settings, version: &str) {
    settings.set(SEEN_KEY, json!(true));
    settings.set(LAST_VERSION_KEY, json!(version));
}

/// Was der Dialog zeigt: Titelversion und Zeilen. Die Zeilen der laufenden Version, sonst (nur beim Aufruf aus dem
/// Menü sinnvoll) die des neuesten Abschnitts im Changelog.
pub fn content(version: &str, changelog_text: &str) -> Option<(String, Vec<String>)> {
    let own = changelog::notes(version, changelog_text);
    if !own.is_empty() {
        return Some((version.to_string(), own));
    }
    let latest = changelog::latest(changelog_text, usize::MAX)?;
    let shown = latest.heading.split_whitespace().next().unwrap_or(&latest.heading).to_string();
    Some((shown, latest.lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_start_shows_nothing_but_remembers() {
        assert_eq!(due(false, "", "1.0.0", false), Due::FirstRun);
    }

    #[test]
    fn a_version_jump_shows_once() {
        assert_eq!(due(true, "0.9.0", "1.0.0", false), Due::Show);
        assert_eq!(due(true, "1.0.0", "1.0.0", false), Due::No);
    }

    #[test]
    fn throwaway_profiles_never_show_it() {
        assert_eq!(due(true, "0.9.0", "1.0.0", true), Due::No);
        assert_eq!(due(false, "", "1.0.0", true), Due::No);
    }

    #[test]
    fn remembering_round_trips_through_the_settings() {
        let mut s = Settings::default();
        assert_eq!(due_for(&s, "1.0.0", false), Due::FirstRun);
        remember(&mut s, "1.0.0");
        assert_eq!(due_for(&s, "1.0.0", false), Due::No);
        assert_eq!(due_for(&s, "1.1.0", false), Due::Show);
    }

    #[test]
    fn content_prefers_the_own_version_and_falls_back_to_the_latest() {
        let log = "## Unreleased\n\n## 1.2.0 (2026-01-02)\n- Feature: a\n\n## 1.1.0\n- Fix: b\n";
        assert_eq!(content("1.1.0", log), Some(("1.1.0".into(), vec!["Fix: b".into()])));
        assert_eq!(content("0.1.0", log), Some(("1.2.0".into(), vec!["Feature: a".into()])));
        assert_eq!(content("0.1.0", "# leer"), None);
    }
}
