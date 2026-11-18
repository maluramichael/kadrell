//! Gruppen und Sessions, gespeichert im Profilordner (`groups.json`, `sessions.json`), und das Auflösen von
//! Zielen (`-t`) wie `ControlTarget` auf dem Mac: exakt vor Anfang, Anfang und Name gleichrangig, mehrdeutig ist
//! ein Fehler. Status und Prozess leben nur zur Laufzeit und werden nicht gespeichert.

use crate::command::{ControlError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const PALETTE: [&str; 14] = [
    "#fab387", "#cba6f7", "#f5c2e7", "#89b4fa", "#a6e3a1", "#94e2d5", "#f9e2af", "#eba0ac", "#b4befe", "#74c7ec",
    "#89dceb", "#f38ba8", "#f2cdcd", "#f5e0dc",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub color: String,
    pub cwd: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub session_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Stabiler Schlüssel, bei neuen Sessions die erste sessionId.
    pub id: String,
    pub cwd: String,
    pub started_at: f64,
    /// Aktuelle Konversation, `/clear` wechselt sie (der Hook meldet die neue).
    pub session_id: String,
    /// Titel von Claude Code, leer bis es einen vergibt.
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_effort: Option<String>,
    #[serde(skip)]
    pub raw_status: Option<String>,
    #[serde(skip)]
    pub waiting_for: Option<String>,
    #[serde(skip)]
    pub first_prompt: Option<String>,
    #[serde(skip)]
    pub message: Option<String>,
}

impl Session {
    pub fn new(cwd: &str, model: Option<String>, effort: Option<String>) -> Session {
        let id = uuid::Uuid::new_v4().to_string();
        let started_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
        Session {
            session_id: id.clone(),
            id,
            cwd: cwd.to_string(),
            started_at,
            name: String::new(),
            custom_name: None,
            claude_model: model,
            claude_effort: effort,
            raw_status: None,
            waiting_for: None,
            first_prompt: None,
            message: None,
        }
    }

    pub fn title(&self) -> String {
        if let Some(c) = &self.custom_name {
            return c.clone();
        }
        if !self.name.is_empty() {
            return self.name.clone();
        }
        self.first_prompt.clone().unwrap_or_else(|| format!("{} · {}", base_name(&self.cwd), &self.id[..4.min(self.id.len())]))
    }

    /// Wie `Session.mapStatus` auf dem Mac: running, waiting, idle, error.
    pub fn status(&self) -> &'static str {
        let Some(raw) = self.raw_status.as_deref().map(str::to_lowercase) else { return "idle" };
        match raw.as_str() {
            "busy" | "working" | "running" | "active" | "starting" => "running",
            "waiting" | "blocked" => "waiting",
            v if v.contains("error") || v.contains("fail") => "error",
            _ => "idle",
        }
    }
}

pub fn base_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let base = trimmed.rsplit(['/', '\\']).next().unwrap_or("");
    if base.is_empty() { path.to_string() } else { base.to_string() }
}

#[derive(Debug, Default)]
pub struct Store {
    pub dir: PathBuf,
    pub groups: Vec<Group>,
    pub sessions: Vec<Session>,
}

impl Store {
    pub fn load(dir: &Path) -> Store {
        let read = |name: &str| std::fs::read(dir.join(name)).ok();
        Store {
            dir: dir.to_path_buf(),
            groups: read("groups.json").and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default(),
            sessions: read("sessions.json").and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default(),
        }
    }

    /// Schreibt über eine Temp-Datei, damit ein Absturz mitten im Schreiben keine halbe Liste hinterlässt.
    pub fn save(&self) {
        let write = |name: &str, data: Vec<u8>| {
            let tmp = self.dir.join(format!("{name}.tmp"));
            if std::fs::write(&tmp, data).is_ok() {
                let _ = std::fs::rename(&tmp, self.dir.join(name));
            }
        };
        write("groups.json", serde_json::to_vec_pretty(&self.groups).unwrap_or_default());
        write("sessions.json", serde_json::to_vec_pretty(&self.sessions).unwrap_or_default());
    }

    pub fn next_color(&self) -> String {
        let used: Vec<&str> = self.groups.iter().map(|g| g.color.as_str()).collect();
        PALETTE.iter().find(|c| !used.contains(c)).unwrap_or(&PALETTE[self.groups.len() % PALETTE.len()]).to_string()
    }

    pub fn make_group(&self, cwd: &str, name: Option<String>) -> Group {
        Group {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.unwrap_or_else(|| base_name(cwd)),
            color: self.next_color(),
            cwd: cwd.to_string(),
            favorite: false,
            session_ids: vec![],
        }
    }

    pub fn group_of(&self, key: &str) -> Option<usize> {
        self.groups.iter().position(|g| g.session_ids.iter().any(|s| s == key))
    }

    /// Neue Session in `group`, sonst in die Gruppe mit gleichem Ordner, sonst in eine neue.
    pub fn add_session(&mut self, session: Session, group: Option<usize>) {
        let index = group.or_else(|| self.groups.iter().position(|g| same_path(&g.cwd, &session.cwd))).unwrap_or_else(|| {
            let g = self.make_group(&session.cwd, None);
            self.groups.push(g);
            self.groups.len() - 1
        });
        self.groups[index].session_ids.push(session.id.clone());
        self.sessions.push(session);
    }

    /// Entfernt die Session; eine dadurch leere Gruppe ohne Herz verschwindet.
    pub fn remove_session(&mut self, key: &str) {
        self.sessions.retain(|s| s.id != key);
        for g in &mut self.groups {
            g.session_ids.retain(|s| s != key);
        }
        self.drop_empty_groups();
    }

    pub fn move_session(&mut self, key: &str, group: usize) {
        for g in &mut self.groups {
            g.session_ids.retain(|s| s != key);
        }
        self.groups[group].session_ids.push(key.to_string());
        self.drop_empty_groups();
    }

    fn drop_empty_groups(&mut self) {
        self.groups.retain(|g| g.favorite || !g.session_ids.is_empty());
    }

    /// Sessions in Baumreihenfolge (Gruppe für Gruppe).
    pub fn ordered_keys(&self) -> Vec<String> {
        self.groups.iter().flat_map(|g| g.session_ids.iter().cloned()).collect()
    }

    pub fn session_mut(&mut self, key: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.id == key)
    }

    pub fn session(&self, key: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.id == key)
    }

    pub fn find_session(&self, t: Option<&str>, caller: Option<&str>, focused: Option<&str>) -> Result<String> {
        let Some(t) = t else {
            return [caller, focused]
                .into_iter()
                .flatten()
                .find(|k| self.session(k).is_some())
                .map(str::to_string)
                .ok_or_else(|| ControlError::new("keine Session angegeben (-t) und keine fokussiert"));
        };
        if let Some(s) = self.sessions.iter().find(|s| s.id == t || s.session_id == t) {
            return Ok(s.id.clone());
        }
        let lower = t.to_lowercase();
        let hits: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| s.id.starts_with(t) || s.session_id.starts_with(t) || s.title().to_lowercase() == lower)
            .collect();
        match hits.as_slice() {
            [] => Err(ControlError::new(format!("keine Session „{t}“"))),
            [one] => Ok(one.id.clone()),
            many => Err(ControlError::new(format!(
                "„{t}“ ist mehrdeutig: {}",
                many.iter().map(|s| &s.id[..8.min(s.id.len())]).collect::<Vec<_>>().join(", ")
            ))),
        }
    }

    pub fn find_group(&self, t: Option<&str>, caller: Option<&str>, focused: Option<&str>) -> Result<usize> {
        let Some(t) = t else {
            let key = self.find_session(None, caller, focused)?;
            return self.group_of(&key).ok_or_else(|| ControlError::new("Session ohne Gruppe"));
        };
        if let Some(i) = self.groups.iter().position(|g| g.id == t) {
            return Ok(i);
        }
        let lower = t.to_lowercase();
        let hits: Vec<usize> =
            (0..self.groups.len()).filter(|&i| self.groups[i].id.starts_with(t) || self.groups[i].name.to_lowercase() == lower).collect();
        match hits.as_slice() {
            [] => Err(ControlError::new(format!("keine Gruppe „{t}“"))),
            [one] => Ok(*one),
            many => Err(ControlError::new(format!(
                "„{t}“ ist mehrdeutig: {}",
                many.iter().map(|&i| format!("{} ({})", self.groups[i].name, &self.groups[i].id[..8])).collect::<Vec<_>>().join(", ")
            ))),
        }
    }
}

/// Windows-Pfade sind unabhängig von Groß-/Kleinschreibung und Schrägstrich-Richtung.
pub fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_lowercase();
    norm(a) == norm(b)
}

/// `~`, relative Pfade gegen den Ordner des Aufrufers, `.` und `..` aufgelöst, mit Schrägstrichen wie auf dem Mac.
pub fn resolve_path(p: &str, cwd: &str, home: &Path) -> String {
    let expanded = match p.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => home.join(rest.trim_start_matches(['/', '\\'])),
        _ => PathBuf::from(p),
    };
    let abs = if expanded.is_absolute() { expanded } else { Path::new(cwd).join(expanded) };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mut s = Store::default();
        let mut a = Session::new("C:/p/alpha", None, None);
        a.id = "aaaa1111".into();
        a.session_id = "s-aaaa".into();
        a.custom_name = Some("_MASTER_".into());
        let mut b = Session::new("C:/p/beta", None, None);
        b.id = "aaab2222".into();
        b.session_id = "s-bbbb".into();
        s.add_session(a, None);
        s.add_session(b, None);
        s
    }

    #[test]
    fn targets_resolve_like_mac() {
        let s = store();
        assert_eq!(s.find_session(Some("aaaa"), None, None).unwrap(), "aaaa1111");
        assert_eq!(s.find_session(Some("_master_"), None, None).unwrap(), "aaaa1111");
        assert_eq!(s.find_session(Some("s-bbbb"), None, None).unwrap(), "aaab2222");
        assert!(s.find_session(Some("aaa"), None, None).unwrap_err().0.contains("mehrdeutig"));
        assert_eq!(s.find_session(None, Some("aaab2222"), Some("aaaa1111")).unwrap(), "aaab2222");
        assert_eq!(s.groups[s.find_group(Some("beta"), None, None).unwrap()].name, "beta");
        assert_eq!(s.find_group(None, Some("aaaa1111"), None).unwrap(), 0);
    }

    #[test]
    fn empty_group_without_heart_vanishes() {
        let mut s = store();
        s.groups[1].favorite = true;
        s.move_session("aaaa1111", 1);
        assert_eq!(s.groups.len(), 1);
        s.remove_session("aaaa1111");
        s.remove_session("aaab2222");
        assert_eq!(s.groups.len(), 1, "Favorit bleibt leer stehen");
    }

    #[test]
    fn same_folder_joins_group() {
        let mut s = store();
        s.add_session(Session::new("c:\\p\\Alpha\\", None, None), None);
        assert_eq!(s.groups[0].session_ids.len(), 2);
    }

    #[test]
    fn paths() {
        let home = Path::new("C:/Users/m");
        assert_eq!(resolve_path("~/d/x", "C:/w", home), "C:/Users/m/d/x");
        assert_eq!(resolve_path("../y/./z", "C:/w/v", home), "C:/w/y/z");
        assert_eq!(resolve_path("D:\\a", "C:/w", home), "D:/a");
        assert_eq!(base_name("C:/development/projects/kadrell/"), "kadrell");
    }

    #[test]
    fn status_mapping() {
        let mut s = Session::new("C:/x", None, None);
        assert_eq!(s.status(), "idle");
        s.raw_status = Some("working".into());
        assert_eq!(s.status(), "running");
        s.raw_status = Some("waiting".into());
        assert_eq!(s.status(), "waiting");
    }
}
