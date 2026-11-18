//! Verlaufssuche (`#`): alle bisherigen Claude-Sessions. Die Daten liefert später `SessionHistory` aus
//! `kadrell-claude` (Welle 2); die Oberfläche kennt nur dieses Trait. `MockHistory` ist der Platzhalter.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::ui::folders::fuzzy;

/// Treffer der Verlaufssuche, Felder wie `HistoryHit` der Mac-App.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryHit {
    pub session_id: String,
    pub title: String,
    pub project: String,
    pub cwd: String,
    /// Letzte Aktivität, Sekunden seit 1970.
    pub last_active: u64,
    /// Fundstelle im Prompt, wenn der Treffer aus dem Text kommt.
    pub snippet: Option<String>,
}

pub trait HistoryProvider {
    /// Erster Aufbau des Index fertig? Sonst zeigt die Palette „Verlauf wird eingelesen …“.
    fn ready(&self) -> bool;
    /// Ohne Begriff die zuletzt benutzten, sonst die Treffer; neueste zuerst.
    fn search(&self, query: &str) -> Vec<HistoryHit>;
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Feste Beispieldaten, bis der echte Index angebunden ist.
pub struct MockHistory {
    entries: Vec<HistoryHit>,
}

impl MockHistory {
    pub fn new() -> Self {
        let now = now_secs();
        let hit = |id: &str, title: &str, project: &str, cwd: &str, ago: u64, snippet: Option<&str>| HistoryHit {
            session_id: id.into(),
            title: title.into(),
            project: project.into(),
            cwd: cwd.into(),
            last_active: now.saturating_sub(ago),
            snippet: snippet.map(String::from),
        };
        let entries = vec![
            hit("h1", "Rust-Palette bauen", "kadrell", "~/development/projects/kadrell", 900, Some("baue die Suche wie in Swift")),
            hit("h2", "Verlaufsindex Transcripts", "kadrell", "~/development/projects/kadrell", 7_200, None),
            hit("h3", "Dockerfile aufräumen", "lulububu", "~/development/projects/lulububu", 86_400, Some("docker compose up schlägt fehl")),
            hit("h4", "Statistik-Seite", "kadrell", "~/development/projects/kadrell", 3 * 86_400, None),
            hit("h5", "Caddy-Konfiguration", "malura-static", "~/development/projects/malura-static", 9 * 86_400, Some("Subdomain anlegen")),
        ];
        Self { entries }
    }
}

impl Default for MockHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl HistoryProvider for MockHistory {
    fn ready(&self) -> bool {
        true
    }

    fn search(&self, query: &str) -> Vec<HistoryHit> {
        let mut hits: Vec<HistoryHit> = self
            .entries
            .iter()
            .filter(|h| {
                let text = format!("{} {} {}", h.title, h.project, h.snippet.as_deref().unwrap_or(""));
                query.is_empty() || fuzzy(query, &text)
            })
            .cloned()
            .collect();
        hits.sort_by_key(|h| std::cmp::Reverse(h.last_active));
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_newest_first() {
        let all = MockHistory::new().search("");
        assert_eq!(all.len(), 5);
        assert!(all.windows(2).all(|w| w[0].last_active >= w[1].last_active));
    }

    #[test]
    fn query_filters_by_title_project_and_snippet() {
        let h = MockHistory::new();
        assert_eq!(h.search("caddy").len(), 1);
        assert_eq!(h.search("lulububu")[0].session_id, "h3");
        assert!(h.search("nichtvorhanden").is_empty());
    }
}
