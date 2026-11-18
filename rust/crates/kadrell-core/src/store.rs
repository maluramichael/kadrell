//! Speicher der Sessions (`sessions.json`, `closed.json`) und der gemeinsame Speicher mit den Gruppen.
//! Quelle: der Persistenzteil von `Services/SessionRegistry.swift`. Status, pid und Titel von Claude Code kommen
//! zur Laufzeit von anderen Teilen und werden hier nicht gespeichert.

use crate::group::GroupStore;
use crate::json_file::{LoadResult, load_array, save_array};
use crate::session::{ClosedSession, Session};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: i64 = 1;
/// So viele zuletzt geschlossene Sessions merkt sich `closed.json` (neueste zuerst).
pub const MAX_CLOSED: usize = 50;

/// Die Sessions, die Kadrell verwaltet, und die zuletzt geschlossenen.
#[derive(Debug, Clone)]
pub struct SessionStore {
    pub sessions: Vec<Session>,
    /// Zuletzt geschlossene Sessions, neueste zuerst, höchstens [`MAX_CLOSED`]: der Schließen-Dialog verspricht
    /// „Konversation bleibt erhalten“, die sessionId muss also auffindbar bleiben.
    pub closed: Vec<ClosedSession>,
    path: PathBuf,
    closed_path: PathBuf,
    last_error: Option<String>,
    /// Der Umschlag nennt eine höhere Version, als diese Kadrell-Version kennt: nur lesen, nie überschreiben.
    read_only: bool,
}

impl SessionStore {
    /// Lädt `sessions.json` (und `closed.json` im selben Ordner). Doppelte Ids bereinigt der Start, die erste gewinnt.
    pub fn open(path: impl Into<PathBuf>) -> SessionStore {
        let path = path.into();
        let closed_path = path.with_file_name("closed.json");
        let LoadResult { items, newer_than_known, message, .. } = load_array::<Session>(&path, SCHEMA_VERSION);
        SessionStore {
            sessions: deduped_by_id(items),
            closed: load_array::<ClosedSession>(&closed_path, SCHEMA_VERSION).items,
            path,
            closed_path,
            last_error: message,
            read_only: newer_than_known,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Letzter Lade- oder Speicherfehler. Kein Abgleich räumt ihn ab.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Von außen melden (Binary nicht gefunden, `claude agents` schlägt fehl).
    pub fn fail(&mut self, message: &str) {
        self.last_error = Some(message.to_string());
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == id)
    }

    /// Eine schon vorhandene Id wird ignoriert (`false`), statt zu duplizieren.
    pub fn add(&mut self, session: Session) -> bool {
        if self.get(&session.id).is_some() {
            return false;
        }
        self.sessions.push(session);
        self.save();
        true
    }

    /// Eigener Name (F2); leer oder nur Leerraum heißt wieder der Titel von Claude Code.
    pub fn rename(&mut self, id: &str, name: &str) {
        let trimmed = name.trim();
        let Some(s) = self.get_mut(id) else { return };
        s.custom_name = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
        self.save();
    }

    /// Titel vom lokalen Modell. Ein vorhandener bleibt, ein eigener Name geht ohnehin vor.
    pub fn set_local_title(&mut self, id: &str, title: &str) {
        let Some(s) = self.get_mut(id).filter(|s| s.local_title.is_none()) else { return };
        s.local_title = Some(title.to_string());
        self.save();
    }

    /// Entfernt Sessions und merkt sie in `closed.json` (`now` in Sekunden seit 1970). Gibt die entfernten zurück.
    pub fn remove(&mut self, ids: &HashSet<String>, now: f64) -> Vec<Session> {
        let (removed, kept): (Vec<Session>, Vec<Session>) = std::mem::take(&mut self.sessions).into_iter().partition(|s| ids.contains(&s.id));
        self.sessions = kept;
        self.save();
        self.archive(&removed, now);
        removed
    }

    fn archive(&mut self, removed: &[Session], now: f64) {
        if removed.is_empty() {
            return;
        }
        let new = removed.iter().map(|s| ClosedSession { closed_at: now, session: s.clone() });
        self.closed.splice(0..0, new);
        self.closed.truncate(MAX_CLOSED);
        if self.read_only {
            return;
        }
        if let Err(message) = save_array(&self.closed, &self.closed_path, SCHEMA_VERSION) {
            self.last_error = Some(message);
        }
    }

    /// Schreibt `sessions.json`. Eine Datei mit neuerer Schema-Version bleibt unberührt.
    pub fn save(&mut self) {
        if self.read_only {
            return;
        }
        if let Err(message) = save_array(&self.sessions, &self.path, SCHEMA_VERSION) {
            self.last_error = Some(message);
        }
    }
}

/// Erste gewinnt: eine doppelte Id (Handbearbeitung, Übernahme einer schon vorhandenen Kurz-Id) würde die
/// Oberfläche sonst bei jedem Start durcheinanderbringen.
fn deduped_by_id(sessions: Vec<Session>) -> Vec<Session> {
    let mut seen = HashSet::new();
    sessions.into_iter().filter(|s| seen.insert(s.id.clone())).collect()
}

/// Sessions und Gruppen eines Profils zusammen, mit den Abläufen, die beide anfassen.
#[derive(Debug, Clone)]
pub struct Store {
    pub groups: GroupStore,
    pub sessions: SessionStore,
    home: String,
}

impl Store {
    /// Öffnet `groups.json`, `sessions.json` und `closed.json` im Profilordner `dir`. `home` ist der
    /// Platzhalter-Ordner neuer Remote-Gruppen.
    pub fn open(dir: &Path, home: &str) -> Store {
        Store { groups: GroupStore::open(dir.join("groups.json")), sessions: SessionStore::open(dir.join("sessions.json")), home: home.to_string() }
    }

    /// Neue Session: in die Gruppe `group` (Id), sonst in die mit gleichem Ordner, sonst in eine neue. Gibt zurück,
    /// ob die Session neu war.
    pub fn add_session(&mut self, session: Session, group: Option<&str>) -> bool {
        let id = session.id.clone();
        if !self.sessions.add(session) {
            return false;
        }
        match group.filter(|g| self.groups.group(g).is_some()) {
            Some(g) => self.groups.attach(&id, g),
            None => {
                self.groups.assign(&self.sessions.sessions, &self.home);
            }
        }
        true
    }

    /// Echtes Schließen: raus aus den Gruppen (eine leere ohne Herz verschwindet beim nächsten Abgleich), raus aus
    /// den Sessions, in `closed.json` gemerkt. Gibt die entfernten Sessions zurück.
    pub fn remove_sessions(&mut self, ids: &HashSet<String>, now: f64) -> Vec<Session> {
        for id in ids {
            self.groups.remove_session(id);
        }
        self.sessions.remove(ids, now)
    }

    /// Ordnet Sessions ohne Gruppe zu, siehe [`GroupStore::assign`].
    pub fn assign(&mut self) -> bool {
        self.groups.assign(&self.sessions.sessions, &self.home)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testutil::TempDir;

    fn s(id: &str, cwd: &str, name: &str) -> Session {
        Session::new(id, cwd, 1.0, id, name)
    }

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn quarantined(d: &TempDir) -> bool {
        std::fs::read_dir(d.path()).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("sessions.corrupt-"))
    }

    #[test]
    fn persists_add_and_remove() {
        let d = TempDir::new();
        let path = d.path().join("sessions.json");
        let mut r = SessionStore::open(&path);
        let mut first = s("s1", "/p", "");
        first.raw_status = Some("busy".into());
        first.pid = Some(3);
        r.add(first);
        r.add(s("s2", "/p", "Zwei"));
        r.remove(&set(&["s1"]), 10.0);
        let reloaded = SessionStore::open(&path).sessions;
        assert_eq!(reloaded.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["s2"]);
        assert_eq!(reloaded[0].name, "Zwei");
        assert_eq!(reloaded[0].pid, None);
    }

    #[test]
    fn corrupt_sessions_file_is_quarantined_not_overwritten() {
        let d = TempDir::new();
        let path = d.path().join("sessions.json");
        std::fs::write(&path, "{kaputt").unwrap();
        let mut r = SessionStore::open(&path);
        assert!(r.sessions.is_empty());
        assert!(r.last_error().is_some());
        assert!(quarantined(&d));
        r.add(s("s1", "/p", ""));
        assert!(path.exists());
        assert!(quarantined(&d));
    }

    #[test]
    fn legacy_bare_array_sessions_file_loads() {
        let d = TempDir::new();
        let path = d.path().join("sessions.json");
        std::fs::write(&path, r#"[{"id":"s1","cwd":"/p","startedAt":1,"sessionId":"s1","name":"alt"}]"#).unwrap();
        assert_eq!(SessionStore::open(&path).sessions[0].id, "s1");
    }

    /// #775: eine doppelte Id in `sessions.json`: erste gewinnt, `add` ignoriert eine schon vorhandene Id.
    #[test]
    fn duplicate_id_is_deduped_on_load_and_rejected_on_add() {
        let d = TempDir::new();
        let path = d.path().join("sessions.json");
        std::fs::write(&path, r#"[{"id":"s1","cwd":"/p","startedAt":1,"sessionId":"s1","name":"eins"},{"id":"s1","cwd":"/q","startedAt":2,"sessionId":"s1","name":"zwei"}]"#).unwrap();
        let mut r = SessionStore::open(&path);
        assert_eq!((r.sessions.len(), r.sessions[0].name.as_str()), (1, "eins"));
        assert!(!r.add(s("s1", "/x", "drei")));
        assert_eq!((r.sessions.len(), r.sessions[0].name.as_str()), (1, "eins"));
    }

    #[test]
    fn remove_archives_closed_sessions_capped_and_persisted() {
        let d = TempDir::new();
        let path = d.path().join("sessions.json");
        let mut r = SessionStore::open(&path);
        r.add(Session::new("s1", "/p", 1.0, "conv-1", "Eins"));
        r.remove(&set(&["s1"]), 100.0);
        assert_eq!(r.closed.iter().map(|c| c.session.session_id.as_str()).collect::<Vec<_>>(), ["conv-1"]);
        assert_eq!(r.closed[0].closed_at, 100.0);
        assert_eq!(SessionStore::open(&path).closed[0].session.session_id, "conv-1");

        for i in 2..=61 {
            r.add(Session::new(&format!("s{i}"), "/p", 1.0, &format!("conv-{i}"), ""));
            r.remove(&set(&[&format!("s{i}")]), 100.0 + f64::from(i));
        }
        assert_eq!(r.closed.len(), MAX_CLOSED);
        assert_eq!(r.closed[0].session.session_id, "conv-61");
        assert_eq!(SessionStore::open(&path).closed.len(), MAX_CLOSED);
    }

    #[test]
    fn rename_trims_and_empty_resets_and_local_title_is_set_once() {
        let d = TempDir::new();
        let mut r = SessionStore::open(d.path().join("sessions.json"));
        r.add(s("s1", "/p", "Claude"));
        r.rename("s1", "  Meins  ");
        assert_eq!(r.get("s1").unwrap().title(), "Meins");
        r.rename("s1", "   ");
        assert_eq!(r.get("s1").unwrap().title(), "Claude");
        r.set_local_title("s1", "Kurz");
        r.set_local_title("s1", "Anders");
        assert_eq!(r.get("s1").unwrap().local_title.as_deref(), Some("Kurz"));
        assert_eq!(SessionStore::open(r.path()).get("s1").unwrap().local_title.as_deref(), Some("Kurz"));
    }

    #[test]
    fn newer_schema_version_is_not_overwritten() {
        let d = TempDir::new();
        let path = d.path().join("sessions.json");
        let text = r#"{"version":9,"items":[{"id":"s1","cwd":"/p","startedAt":1,"sessionId":"s1","name":""}]}"#;
        std::fs::write(&path, text).unwrap();
        let mut r = SessionStore::open(&path);
        r.add(s("s2", "/p", ""));
        r.remove(&set(&["s1"]), 1.0);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(!d.path().join("closed.json").exists());
    }

    #[test]
    fn store_adds_sessions_to_groups_and_closes_them() {
        let d = TempDir::new();
        let mut store = Store::open(d.path(), "/home/t");
        assert!(store.add_session(s("a", "/p/alpha", ""), None));
        assert!(store.add_session(s("b", "/p/beta", ""), None));
        assert!(store.add_session(s("c", "/p/alpha", ""), None));
        assert!(!store.add_session(s("c", "/p/other", ""), None), "doppelte Id");
        assert_eq!(store.groups.groups.len(), 2);
        assert_eq!(store.groups.groups[0].session_ids, ["a", "c"]);
        let beta = store.groups.groups[1].id.clone();
        store.add_session(s("d", "/p/alpha", ""), Some(&beta));
        assert_eq!(store.groups.group(&beta).unwrap().session_ids, ["b", "d"], "ausdrücklich gewählte Gruppe geht vor dem Ordner");
        store.remove_sessions(&set(&["b", "d"]), 5.0);
        assert_eq!(store.sessions.closed.len(), 2);
        store.assign();
        assert_eq!(store.groups.groups.len(), 1, "leere Gruppe ohne Herz ist weg");
        let reopened = Store::open(d.path(), "/home/t");
        assert_eq!(reopened.sessions.sessions.len(), 2);
        assert_eq!(reopened.groups.groups[0].session_ids, ["a", "c"]);
        assert_eq!(reopened.groups.path().file_name().unwrap(), "groups.json");
    }
}
