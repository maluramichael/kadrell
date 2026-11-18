//! Logik des Dialogs „Gruppe bearbeiten“ / „Neue Gruppe“ ohne Oberfläche (`EditGroupModel` in `EditGroupSheet.swift`):
//! Ordner auflösen (Kürzel `~/d/p/kad`), Name folgt dem Ordner, Hinweiszeile, Speichern. Alles Reine ist testbar,
//! das Dateisystem steht hinter [`Fs`].

use kadrell_core::group::{Group, GroupStore};

use crate::ui::folders;

/// Dateisystem-Zugriff des Modells (Tests ersetzen ihn).
pub trait Fs {
    fn is_dir(&self, path: &str) -> bool;
    fn list_dirs(&self, dir: &str) -> Vec<String>;
}

/// Das echte Dateisystem.
pub struct RealFs;

impl Fs for RealFs {
    fn is_dir(&self, path: &str) -> bool {
        std::path::Path::new(path).is_dir()
    }
    fn list_dirs(&self, dir: &str) -> Vec<String> {
        folders::list_dirs(dir)
    }
}

/// Die Zeile unter dem Ordnerfeld, als Schlüssel samt Werten (die Oberfläche übersetzt sie).
#[derive(Clone, Debug, PartialEq)]
pub enum Hint {
    /// Remote-Gruppe: `ssh · host`.
    Remote(String),
    /// Ordner nicht gefunden (Fehler).
    NotFound,
    /// Hilfe zur Eingabe.
    Help,
    /// Zeilen mit dem, was der neue Ordner bewirkt.
    Lines(Vec<HintLine>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum HintLine {
    /// `→ ~/pfad`: ausgeschriebener Ordner hinter einem Kürzel.
    Resolved(String),
    /// Neue Sessions starten im neuen Ordner, laufende bleiben.
    Moved,
    /// Eine andere Gruppe hat denselben Ordner.
    SharedWith(String),
}

pub struct EditGroupModel {
    pub group: Group,
    pub is_new: bool,
    others: Vec<Group>,
    pub name: String,
    /// Farbe als `#rrggbb`.
    pub color: String,
    /// Ordner wie getippt, Kürzel erlaubt.
    folder: String,
    resolved: Option<String>,
    /// Zuletzt gültiger Ordner: daran erkennt der Name, ob er noch der automatische ist.
    last_folder: String,
    home: String,
}

impl EditGroupModel {
    pub fn new(group: Group, others: Vec<Group>, is_new: bool, home: &str) -> Self {
        Self {
            name: group.name.clone(),
            color: group.color.clone(),
            folder: folders::short_path(&group.cwd, home),
            resolved: Some(group.cwd.clone()),
            last_folder: group.cwd.clone(),
            group,
            is_new,
            others,
            home: home.to_string(),
        }
    }

    pub fn is_remote(&self) -> bool {
        self.group.host.is_some()
    }

    pub fn folder(&self) -> &str {
        &self.folder
    }

    pub fn resolved(&self) -> Option<&str> {
        self.resolved.as_deref()
    }

    /// Vorhandener Ordner gilt direkt, sonst ein eindeutiges Kürzel. Mehrdeutig: erst per Tab wählen.
    pub fn resolve(typed: &str, home: &str, fs: &dyn Fs) -> Option<String> {
        let p = folders::normalize(typed, home);
        if p.is_empty() {
            return None;
        }
        if fs.is_dir(&p) {
            return Some(p);
        }
        let hits = folders::expand_abbreviated(typed, home, 2, &|d| fs.list_dirs(d));
        (hits.len() == 1).then(|| folders::normalize(&hits[0], home))
    }

    /// Neuer Ordnertext: löst auf und zieht den Namen mit, solange er noch der automatische war.
    pub fn set_folder(&mut self, typed: &str, fs: &dyn Fs) {
        if typed == self.folder {
            return;
        }
        self.folder = typed.to_string();
        self.resolved = Self::resolve(typed, &self.home, fs);
        let Some(now) = self.resolved.clone() else { return };
        if now == self.last_folder {
            return;
        }
        if self.name == GroupStore::default_name(&self.last_folder, None) {
            self.name = GroupStore::default_name(&now, None);
        }
        self.last_folder = now;
    }

    /// Tab: erster Treffer des Kürzels ausgeschrieben, `None` ohne Treffer (Piepton).
    pub fn completion(&self, fs: &dyn Fs) -> Option<String> {
        let first = folders::expand_abbreviated(&self.folder, &self.home, 1, &|d| fs.list_dirs(d)).into_iter().next()?;
        Some(folders::short_path(&folders::normalize(&first, &self.home), &self.home))
    }

    /// Ein Ordner (aus Explorer/Finder oder Ordnerdialog) ersetzt den Text.
    pub fn take_dir(&mut self, dir: &str, fs: &dyn Fs) {
        let short = folders::short_path(&folders::normalize(dir, &self.home), &self.home);
        self.set_folder(&short, fs);
    }

    /// Neuer Ordner gewählt: laufende Sessions bleiben, wo sie sind, neue starten dort.
    pub fn folder_moved(&self) -> bool {
        !self.is_new && self.resolved.as_deref().is_some_and(|r| r != self.group.cwd)
    }

    /// Eine andere lokale Gruppe hat schon diesen Ordner: erlaubt, nur ein Hinweis.
    pub fn shared_with(&self) -> Option<&Group> {
        let dir = self.resolved.as_deref()?;
        self.others.iter().find(|g| g.host.is_none() && kadrell_core::paths::same_folder(&g.cwd, dir))
    }

    /// Zeile unter dem Ordner: Fehler, sonst was der neue Ordner bewirkt, sonst die Eingabe-Hilfen.
    pub fn hint(&self) -> Hint {
        if let Some(host) = &self.group.host {
            return Hint::Remote(host.clone());
        }
        let Some(dir) = &self.resolved else { return Hint::NotFound };
        let mut lines = Vec::new();
        if folders::normalize(&self.folder, &self.home) != *dir {
            lines.push(HintLine::Resolved(folders::short_path(dir, &self.home)));
        }
        if self.folder_moved() {
            lines.push(HintLine::Moved);
        }
        if let Some(other) = self.shared_with() {
            lines.push(HintLine::SharedWith(other.name.clone()));
        }
        if lines.is_empty() || !(self.is_new || self.folder_moved()) {
            return Hint::Help;
        }
        Hint::Lines(lines)
    }

    /// Ohne gültigen Ordner kein Speichern: sonst stünde die Gruppe auf einem Pfad, in dem keine Session starten kann.
    pub fn save(&self) -> Option<Group> {
        let mut g = self.group.clone();
        let n = self.name.trim();
        if !n.is_empty() {
            g.name = n.to_string();
        }
        g.color = self.color.clone();
        if !self.is_remote() {
            g.cwd = self.resolved.clone()?;
        }
        Some(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake;

    impl Fake {
        const DIRS: [&'static str; 4] = ["C:/Users/m", "C:/Users/m/dev", "C:/Users/m/dev/proj", "C:/Users/m/dev/proj/kadrell"];
    }

    impl Fs for Fake {
        fn is_dir(&self, path: &str) -> bool {
            Self::DIRS.contains(&path)
        }
        fn list_dirs(&self, dir: &str) -> Vec<String> {
            let prefix = format!("{}/", dir.trim_end_matches('/'));
            Self::DIRS.iter().filter_map(|d| d.strip_prefix(&prefix)).filter(|r| !r.contains('/')).map(str::to_string).collect()
        }
    }

    const HOME: &str = "C:/Users/m";

    fn model(cwd: &str, is_new: bool) -> EditGroupModel {
        let g = Group::new("g1", &GroupStore::default_name(cwd, None), "#fab387", cwd, &[]);
        let other = Group::new("g2", "andere", "#89b4fa", "C:/Users/m/dev/proj", &[]);
        EditGroupModel::new(g, vec![other], is_new, HOME)
    }

    #[test]
    fn abbreviation_resolves_only_when_unique() {
        let fs = Fake;
        assert_eq!(EditGroupModel::resolve("~/d/p/kad", HOME, &fs).as_deref(), Some("C:/Users/m/dev/proj/kadrell"));
        assert_eq!(EditGroupModel::resolve("~/dev/proj", HOME, &fs).as_deref(), Some("C:/Users/m/dev/proj"));
        assert_eq!(EditGroupModel::resolve("~/zzz", HOME, &fs), None);
        assert_eq!(EditGroupModel::resolve("", HOME, &fs), None);
    }

    #[test]
    fn name_follows_folder_only_while_automatic() {
        let fs = Fake;
        let mut m = model("C:/Users/m/dev/proj", false);
        m.set_folder("~/dev/proj/kadrell", &fs);
        assert_eq!(m.name, "kadrell");
        m.name = "Mein Name".into();
        m.set_folder("~/dev", &fs);
        assert_eq!(m.name, "Mein Name");
    }

    #[test]
    fn unresolved_folder_blocks_save_and_hints_error() {
        let fs = Fake;
        let mut m = model("C:/Users/m/dev/proj", false);
        m.set_folder("~/gibtsnicht", &fs);
        assert_eq!(m.resolved(), None);
        assert_eq!(m.hint(), Hint::NotFound);
        assert!(m.save().is_none());
    }

    #[test]
    fn save_trims_name_and_keeps_old_name_when_empty() {
        let fs = Fake;
        let mut m = model("C:/Users/m/dev/proj", false);
        m.name = "  Neu  ".into();
        m.color = "#a6e3a1".into();
        m.set_folder("~/d/p/kad", &fs);
        let g = m.save().unwrap();
        assert_eq!((g.name.as_str(), g.color.as_str(), g.cwd.as_str()), ("Neu", "#a6e3a1", "C:/Users/m/dev/proj/kadrell"));
        m.name = "   ".into();
        assert_eq!(m.save().unwrap().name, "proj");
    }

    #[test]
    fn hint_lines_follow_the_rules() {
        let fs = Fake;
        let mut m = model("C:/Users/m/dev/proj", false);
        assert_eq!(m.hint(), Hint::Help);
        m.set_folder("~/d/p/kad", &fs);
        assert_eq!(m.hint(), Hint::Lines(vec![HintLine::Resolved("~/dev/proj/kadrell".into()), HintLine::Moved]));
        let mut n = model("C:/Users/m/dev", true);
        n.set_folder("~/dev/proj", &fs);
        assert_eq!(n.hint(), Hint::Lines(vec![HintLine::SharedWith("andere".into())]));
        assert!(!n.folder_moved());
    }

    #[test]
    fn remote_group_keeps_its_folder_and_shows_host() {
        let mut g = Group::new("r", "box", "#fab387", HOME, &[]);
        g.host = Some("box".into());
        let mut m = EditGroupModel::new(g, vec![], false, HOME);
        assert_eq!(m.hint(), Hint::Remote("box".into()));
        m.name = "Server".into();
        let saved = m.save().unwrap();
        assert_eq!((saved.name.as_str(), saved.cwd.as_str()), ("Server", HOME));
    }

    #[test]
    fn completion_and_dropped_folder() {
        let fs = Fake;
        let mut m = model("C:/Users/m/dev/proj", false);
        m.set_folder("~/d/p/k", &fs);
        assert_eq!(m.completion(&fs).as_deref(), Some("~/dev/proj/kadrell"));
        m.take_dir("C:\\Users\\m\\dev", &fs);
        assert_eq!(m.folder(), "~/dev");
        assert_eq!(m.resolved(), Some("C:/Users/m/dev"));
    }
}
