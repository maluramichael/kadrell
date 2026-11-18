//! Changelog-Zeilen für „Was ist neu“ und den Auszug im About (Swift: `Services/WhatsNew.swift`). Das CHANGELOG.md des
//! Repos steckt beim Bauen in der Datei; die Version kommt aus dem Cargo-Workspace.

/// CHANGELOG.md des Repos, beim Bauen eingebettet.
pub const BUNDLED: &str = include_str!("../../../../../CHANGELOG.md");

/// Version der App (Cargo-Workspace).
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Der Eintrag einer Zeile `- …` ohne den Aufzählungspunkt.
fn bullet(raw: &str) -> Option<String> {
    raw.trim().strip_prefix("- ").map(str::to_string)
}

/// Gehört die Überschrift `## …` zu dieser Version? `## 1.38.0` oder `## 1.38.0 (Datum)`.
fn is_heading_of(heading: &str, version: &str) -> bool {
    heading == format!("## {version}") || heading.starts_with(&format!("## {version} "))
}

/// Zeilen der Version ohne führendes „- “, leer wenn der Abschnitt fehlt. `## 1.38.0 (…)` startet ihn, die nächste
/// `## `-Überschrift beendet ihn.
pub fn notes(version: &str, changelog: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut inside = false;
    for raw in changelog.lines() {
        if raw.starts_with("## ") {
            if inside {
                break;
            }
            inside = is_heading_of(raw, version);
            continue;
        }
        if inside {
            lines.extend(bullet(raw));
        }
    }
    lines
}

/// Ein Abschnitt des Changelogs: Überschrift ohne `## ` und seine Zeilen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub heading: String,
    pub lines: Vec<String>,
}

/// Der neueste Abschnitt mit Einträgen (ein leeres `## Unreleased` zählt nicht), höchstens `max` Zeilen.
pub fn latest(changelog: &str, max: usize) -> Option<Release> {
    let mut current: Option<Release> = None;
    for raw in changelog.lines() {
        if let Some(heading) = raw.strip_prefix("## ") {
            if let Some(done) = current.take().filter(|r| !r.lines.is_empty()) {
                return Some(done);
            }
            current = Some(Release { heading: heading.trim().to_string(), lines: Vec::new() });
        } else if let Some(r) = current.as_mut()
            && r.lines.len() < max
        {
            r.lines.extend(bullet(raw));
        }
    }
    current.filter(|r| !r.lines.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog\n\n## Unreleased\n\n## 1.2.0 (2026-01-02)\n\n- Feature: zwei\n  - Einrückung zählt\n- Fix: drei\nText ohne Punkt\n\n## 1.1.0\n- Feature: alt\n";

    #[test]
    fn notes_pick_the_section_of_the_version() {
        assert_eq!(notes("1.2.0", LOG), vec!["Feature: zwei", "Einrückung zählt", "Fix: drei"]);
        assert_eq!(notes("1.1.0", LOG), vec!["Feature: alt"]);
    }

    #[test]
    fn notes_do_not_match_prefix_versions() {
        assert!(notes("1.2", LOG).is_empty(), "1.2 ist nicht 1.2.0");
        assert!(notes("9.9.9", LOG).is_empty());
        assert!(notes("1.2.0", "").is_empty());
    }

    #[test]
    fn latest_skips_an_empty_unreleased_section() {
        let r = latest(LOG, 10).unwrap();
        assert_eq!(r.heading, "1.2.0 (2026-01-02)");
        assert!(r.lines.contains(&"Fix: drei".to_string()));
    }

    #[test]
    fn latest_uses_unreleased_when_it_has_entries_and_caps_the_lines() {
        let log = "## Unreleased\n- a\n- b\n- c\n\n## 1.0.0\n- x\n";
        let r = latest(log, 2).unwrap();
        assert_eq!((r.heading.as_str(), r.lines.len()), ("Unreleased", 2));
        assert_eq!(latest("# nichts\n", 5), None);
    }

    #[test]
    fn bundled_changelog_has_a_released_section() {
        assert!(latest(BUNDLED, 3).is_some());
        assert!(!version().is_empty());
    }
}
