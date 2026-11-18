//! Verlaufssuche (`#`, Strg+Umschalt+H): alle bisherigen Claude-Sessions. Die Daten liefert `HistoryIndex` aus
//! `kadrell-claude` (Regeln aus app/README.md „Verlaufssuche“: jedes Wort muss vorkommen, Titeltreffer zuerst, sonst
//! neueste zuerst, ohne Treffer unscharf über Titel und Projekt). Die Oberfläche kennt nur das Trait `HistoryProvider`.
//!
//! `HistoryStore` hält den Index auf dem UI-Thread und baut ihn auf dem Hintergrund-Thread nach: beim Start und bei
//! jedem Öffnen werden nur neue und gewachsene Transcripts ab der zuletzt gelesenen Stelle gelesen, der allererste Lauf
//! liest alles. Transcripts werden nur gelesen, nie geschrieben; der Index liegt im Cache-Ordner, nicht in `~/.claude`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gpui::Context;
use kadrell_claude::history::{self as claude_history, HistoryHit as IndexHit, HistoryIndex, HistorySession};

use crate::ui::folders;

/// Treffer der Verlaufssuche, Felder wie `HistoryHit` der Mac-App.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryHit {
    pub session_id: String,
    pub title: String,
    pub project: String,
    /// Ordner der ersten Eingabe: dort liegt das Transcript, von dort aus findet `claude --resume` es.
    pub cwd: String,
    /// Wie `cwd`, mit `~` für den Home-Ordner (Anzeige).
    pub short: String,
    /// Letzte Aktivität, Sekunden seit 1970.
    pub last_active: u64,
    /// Fundstelle im Prompt, wenn der Treffer aus dem Text kommt.
    pub snippet: Option<String>,
}

pub trait HistoryProvider {
    /// Erster Aufbau des Index fertig? Sonst zeigt die Palette „Verlauf wird eingelesen …“.
    fn ready(&self) -> bool;
    /// Ohne Begriff die zuletzt benutzten, sonst die Treffer in der Reihenfolge des Index.
    fn search(&self, query: &str) -> Vec<HistoryHit>;
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Treffer des Index in die Form der Oberfläche; die Reihenfolge bleibt, wie der Index sie liefert.
pub fn to_hits(found: Vec<IndexHit>, home: &str) -> Vec<HistoryHit> {
    found
        .into_iter()
        .map(|h| {
            let cwd = h.session.cwd.replace('\\', "/");
            HistoryHit {
                session_id: h.session.session_id.clone(),
                title: h.session.title(),
                project: h.session.project().to_string(),
                short: folders::short_path(&cwd, home),
                cwd: h.session.cwd.clone(),
                last_active: h.session.last_active.max(0.0) as u64,
                snippet: h.snippet,
            }
        })
        .collect()
}

/// Wo der Verlaufsindex liegt: `<cache>/de.malura.kadrell/session-history-v3<configDir>.json` (je Claude-Datenordner
/// eine Datei, für alle Profile gemeinsam). Cache-Ordner: `%LOCALAPPDATA%` unter Windows, `~/Library/Caches` auf dem Mac,
/// sonst `$XDG_CACHE_HOME` oder `~/.cache`.
pub fn cache_file(config_dir: &str, home: &str) -> String {
    let cache = if cfg!(windows) {
        std::env::var("LOCALAPPDATA").map(|p| p.replace('\\', "/")).unwrap_or_else(|_| format!("{home}/AppData/Local"))
    } else if cfg!(target_os = "macos") {
        format!("{home}/Library/Caches")
    } else {
        std::env::var("XDG_CACHE_HOME").unwrap_or_else(|_| format!("{home}/.cache"))
    };
    claude_history::cache_path(&cache, config_dir)
}

/// Stand der Dateien, nur auf dem Hintergrund-Thread angefasst.
#[derive(Default)]
struct Entries {
    sessions: HashMap<String, HistorySession>,
    /// Noch nie gelesen: der erste Lauf startet vom Cache auf der Platte.
    first: bool,
}

impl Entries {
    /// Nachlesen. `Some(index)`, wenn es einen neuen Index gibt (erster Lauf oder etwas hat sich geändert).
    fn refresh(&mut self, config_dir: &str, cache: &str) -> Option<HistoryIndex> {
        let base = if self.first { claude_history::load_cache(cache) } else { std::mem::take(&mut self.sessions) };
        let (fresh, changed) = claude_history::refresh(&base, config_dir);
        if changed {
            claude_history::save_cache(&fresh, cache);
        }
        let index = (changed || self.first).then(|| HistoryIndex::new(fresh.values().cloned().collect()));
        self.sessions = fresh;
        self.first = false;
        index
    }
}

/// Der Verlaufsindex der Shell, nur vom UI-Thread benutzt. Klonen teilt den Stand.
#[derive(Clone)]
pub struct HistoryStore {
    index: Rc<RefCell<Option<HistoryIndex>>>,
    /// `None` während ein Lauf unterwegs ist: so laufen nie zwei zugleich.
    entries: Rc<RefCell<Option<Entries>>>,
    home: Rc<str>,
}

impl HistoryStore {
    pub fn new(home: &str) -> Self {
        Self { index: Rc::default(), entries: Rc::new(RefCell::new(Some(Entries { first: true, ..Entries::default() }))), home: home.into() }
    }

    /// Läuft gerade ein Nachlesen?
    pub fn busy(&self) -> bool {
        self.entries.borrow().is_none()
    }

    /// Liest auf dem Hintergrund-Thread nach und ruft danach `done` auf dem UI-Thread auf (auch ohne Änderung), damit eine
    /// offene Suche neu filtert. Läuft schon ein Nachlesen, passiert nichts: dessen Ende meldet sich ohnehin.
    pub fn refresh<T: 'static>(&self, config_dir: String, cache: String, cx: &mut Context<T>, done: fn(&mut T, &mut Context<T>)) {
        let Some(mut entries) = self.entries.borrow_mut().take() else { return };
        let store = self.clone();
        cx.spawn(async move |this, cx| {
            let (entries, index) = cx
                .background_executor()
                .spawn(async move {
                    let index = entries.refresh(&config_dir, &cache);
                    (entries, index)
                })
                .await;
            store.entries.replace(Some(entries));
            if let Some(index) = index {
                store.index.replace(Some(index));
            }
            let _ = this.update(cx, done);
        })
        .detach();
    }

    #[cfg(test)]
    fn set_index(&self, index: HistoryIndex) {
        self.index.replace(Some(index));
    }
}

impl HistoryProvider for HistoryStore {
    fn ready(&self) -> bool {
        self.index.borrow().is_some()
    }

    fn search(&self, query: &str) -> Vec<HistoryHit> {
        let found = self.index.borrow().as_ref().map(|i| i.search_default(query)).unwrap_or_default();
        to_hits(found, &self.home)
    }
}

/// Provider für die Palette: teilt den Stand der Shell.
pub fn provider(store: &HistoryStore) -> Rc<dyn HistoryProvider> {
    Rc::new(store.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, cwd: &str, title: &str, prompts: &[&str], last: f64) -> HistorySession {
        HistorySession {
            session_id: id.into(),
            path: format!("/x/{id}.jsonl"),
            cwd: cwd.into(),
            custom_title: Some(title.into()),
            prompts: prompts.iter().map(|p| p.to_string()).collect(),
            last_active: last,
            ..HistorySession::default()
        }
    }

    fn store() -> HistoryStore {
        let s = HistoryStore::new("C:/Users/m");
        s.set_index(HistoryIndex::new(vec![
            session("old", "C:/Users/m/dev/kadrell", "Palette bauen", &["baue die Suche"], 100.0),
            session("new", "C:/Users/m/dev/kadrell", "Dockerfile", &["die Palette kaputt"], 300.0),
            session("title", "C:/Users/m/dev/web", "Palette Mock ersetzen", &["x"], 200.0),
        ]));
        s
    }

    #[test]
    fn not_ready_until_an_index_exists() {
        let s = HistoryStore::new("/h");
        assert!(!s.ready() && s.search("x").is_empty());
        assert!(!s.busy());
    }

    #[test]
    fn empty_query_returns_newest_first() {
        let ids: Vec<String> = store().search("").into_iter().map(|h| h.session_id).collect();
        assert_eq!(ids, ["new", "title", "old"]);
    }

    #[test]
    fn title_hits_come_before_newer_prompt_hits() {
        // „palette“ steht bei „new“ nur im Prompt (neuer), bei „title“ und „old“ im Titel: Titel zuerst, dann die neueste.
        let ids: Vec<String> = store().search("palette").into_iter().map(|h| h.session_id).collect();
        assert_eq!(ids, ["title", "old", "new"]);
    }

    #[test]
    fn every_word_must_match_and_a_snippet_shows_the_prompt() {
        let hits = store().search("palette kaputt");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].session_id, "new");
        assert!(hits[0].snippet.as_deref().is_some_and(|s| s.contains("kaputt")));
    }

    #[test]
    fn hits_carry_project_folder_and_home_short_path() {
        let h = &store().search("dockerfile")[0];
        assert_eq!((h.project.as_str(), h.cwd.as_str(), h.short.as_str()), ("kadrell", "C:/Users/m/dev/kadrell", "~/dev/kadrell"));
    }

    #[test]
    fn cache_file_is_derived_from_the_config_dir() {
        let f = cache_file("C:/Users/m/.claude", "C:/Users/m");
        assert!(f.contains("de.malura.kadrell.rust/session-history-v3C__Users_m_.claude.json"), "{f}");
    }
}
