//! Zustand von „Neue Session“ ohne Oberfläche (Swift: `NewSessionModel` in `NewSessionSheet.swift`): Kandidaten aus
//! Kontext (Explorer/Finder, Zwischenablage), Gruppen, benutzten Ordnern und Git-Repos, getippt als Wortsuche oder als
//! Pfadkürzel; dazu „Ordner anlegen“ mit Nachfrage und die Extension-Einträge (Repos zum Klonen). Die Oberfläche setzt
//! die Ergebnisse von `start`, `pick` und `create_and_start` um (Start, Piepton, Extension).

use std::collections::{HashMap, HashSet};

use kadrell_claude::folder_index::{self as fi, FolderIndex};
use kadrell_i18n::trd;

use crate::ui::folders::{is_path_query, short_path};

pub const MAX_ROWS: usize = 60;
/// Treffer eines Pfadkürzels (Swift: `expandAbbreviated`, Standardlimit).
const PATH_LIMIT: usize = 20;

/// Gruppe, soweit der Dialog sie braucht.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub cwd: String,
    /// Remote-Gruppe: hat keinen lokalen Ordner.
    pub host: Option<String>,
}

/// Was ⏎ auf einem Eintrag tut, der kein gewöhnlicher Ordner ist.
#[derive(Clone, Debug, PartialEq)]
pub enum Select {
    /// „Ordner anlegen“: erstes ⏎ fragt nach, das zweite legt an.
    Create,
    /// Von einer Extension gelieferter Eintrag (Pfad `ext:<name>:<id>`).
    Extension { name: String, id: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub path: String,
    pub group: Option<GroupInfo>,
    /// Herkunft rechts in der Zeile, wenn es keine Gruppe ist: „Finder“, „Zwischenablage“, „Git“.
    pub tag: String,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub select: Option<Select>,
}

impl Candidate {
    fn folder(path: &str, group: Option<GroupInfo>, tag: &str) -> Candidate {
        Candidate { path: path.to_string(), group, tag: tag.to_string(), title: None, detail: None, select: None }
    }

    pub fn is_create(&self) -> bool {
        self.select == Some(Select::Create)
    }
}

/// Wirkung eines Befehls auf die Oberfläche.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Nothing,
    Beep,
    Start { group: Option<String>, path: String },
    Extension { name: String, id: String },
}

/// Eingaben des Dialogs, damit Tests ohne Fenstersystem auskommen.
pub struct Inputs {
    pub groups: Vec<GroupInfo>,
    /// Lebende Sessions je Gruppen-Id.
    pub counts: HashMap<String, usize>,
    pub clones: Vec<Candidate>,
    pub start_folder: String,
    pub home: String,
    /// Sekunden seit 1970 für die Frecency.
    pub now: f64,
}

pub struct Model {
    inputs: Inputs,
    index: FolderIndex,
    /// Pfad und Herkunft (Explorer vor Zwischenablage).
    context: Vec<(String, String)>,
    query: String,
    pub candidates: Vec<Candidate>,
    pub selected: usize,
    /// ⏎ auf einem getippten Pfad ohne Treffer, der auch nicht existiert.
    pub not_found_path: Option<String>,
    /// Ordner, den die Eingabe neu anlegen würde (nur unter Home).
    pub create_target: Option<String>,
    /// Das erste ⏎ auf „Ordner anlegen“ hat nachgefragt: das nächste ⏎ auf diesem Pfad legt an.
    pub confirm_path: Option<String>,
    /// Anlegen gescheitert: der Grund steht sichtbar in der Liste.
    pub create_error: Option<String>,
    pub scanning: bool,
}

/// Vergleichsschlüssel eines Pfads: unter Windows ohne Rücksicht auf Groß/Klein.
fn key(path: &str) -> String {
    if cfg!(windows) { path.to_lowercase() } else { path.to_string() }
}

fn last_component(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

impl Model {
    pub fn new(inputs: Inputs, index: FolderIndex, clipboard: Option<String>) -> Model {
        let context = clipboard.map(|c| (c, trd!("Zwischenablage"))).into_iter().collect();
        let mut model = Model {
            inputs,
            index,
            context,
            query: String::new(),
            candidates: Vec::new(),
            selected: 0,
            not_found_path: None,
            create_target: None,
            confirm_path: None,
            create_error: None,
            scanning: false,
        };
        model.update();
        model
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Tippen hebt Nachfrage, Fehler und „existiert nicht“ auf und sucht neu.
    pub fn set_query(&mut self, query: &str) {
        if query == self.query {
            return;
        }
        self.query = query.to_string();
        self.not_found_path = None;
        self.confirm_path = None;
        self.create_error = None;
        self.update();
    }

    /// Ordner des vordersten Dateimanager-Fensters ist da: nach vorn, aber nicht unter den Fingern umsortieren, nur solange
    /// noch nichts getippt oder gewählt ist.
    pub fn set_finder(&mut self, path: &str) {
        self.context.retain(|(p, _)| p != path);
        self.context.insert(0, (path.to_string(), crate::lang::file_manager().to_string()));
        if self.query.is_empty() && self.selected == 0 {
            self.update();
        }
    }

    /// Repo-Scan fertig: neuer Stand, ohne eine schon getroffene Auswahl umzuwerfen.
    pub fn set_repos(&mut self, repos: Vec<String>) {
        self.index.repos = repos;
        self.scanning = false;
        if self.selected == 0 {
            self.update();
        }
    }

    /// Stand des Index für den Scan im Hintergrund.
    pub fn index(&self) -> &FolderIndex {
        &self.index
    }

    // MARK: Kandidaten

    fn by_cwd(&self) -> HashMap<String, &GroupInfo> {
        let mut map = HashMap::new();
        for g in &self.inputs.groups {
            map.entry(key(&g.cwd)).or_insert(g);
        }
        map
    }

    /// Mit „/“ oder „~“: Pfad, jedes Stück darf abgekürzt sein (`~/d/p/kad`). Sonst Wortfetzen gegen den Index.
    pub fn update(&mut self) {
        let q = self.query.trim().to_string();
        self.candidates = if is_path_query(&q) { self.path_candidates(&q) } else { self.word_candidates(&q) };
        self.offer_create(&q);
        self.selected = 0;
    }

    fn path_candidates(&self, q: &str) -> Vec<Candidate> {
        let home = &self.inputs.home;
        let by_cwd = self.by_cwd();
        let found = fi::expand_abbreviated(&q.replace('\\', "/"), home, home, PATH_LIMIT);
        found
            .iter()
            .map(|p| {
                let p = fi::normalize(p, false, home);
                Candidate::folder(&p, by_cwd.get(&key(&p)).map(|g| (*g).clone()), "")
            })
            .collect()
    }

    /// Alle Quellen in Reihenfolge, je Pfad einmal mit der Herkunft der ersten Quelle.
    fn sources(&self) -> Vec<(String, String)> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        let mut add = |p: &str, tag: &str| {
            if seen.insert(key(p)) {
                out.push((p.to_string(), tag.to_string()));
            }
        };
        for (p, tag) in &self.context {
            add(p, tag);
        }
        // Host-Gruppen haben keinen lokalen Ordner, alle anderen nur, solange es ihn gibt.
        for g in &self.inputs.groups {
            if g.host.is_some() || fi::is_directory(&g.cwd) {
                add(&g.cwd, "");
            }
        }
        let mut used: Vec<&String> = self.index.uses.keys().collect();
        used.sort();
        for p in used.into_iter().filter(|p| fi::is_directory(p)) {
            add(p, "");
        }
        for p in &self.index.repos {
            add(p, "Git");
        }
        out
    }

    fn word_candidates(&self, q: &str) -> Vec<Candidate> {
        let by_cwd = self.by_cwd();
        let context: HashSet<String> = self.context.iter().map(|(p, _)| key(p)).collect();
        let mut scored: Vec<Scored> = self
            .sources()
            .into_iter()
            .filter_map(|(path, tag)| self.score(q, path, tag, &by_cwd, &context))
            .collect();
        scored.sort_by(|a, b| {
            b.ctx
                .cmp(&a.ctx)
                .then(a.rank.cmp(&b.rank))
                .then(b.score.total_cmp(&a.score))
                .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
        });
        let folders = scored.into_iter().take(MAX_ROWS).map(|s| {
            let group = by_cwd.get(&key(&s.path)).map(|g| (*g).clone());
            Candidate::folder(&s.path, group, &s.tag)
        });
        // Klon-Einträge der Extensions: bei leerer Eingabe hinter den Ordnern, beim Tippen davor.
        let clones = self.clone_matches(q);
        if q.is_empty() { folders.chain(clones).collect() } else { clones.into_iter().chain(folders).collect() }
    }

    fn score(&self, q: &str, path: String, tag: String, by_cwd: &HashMap<String, &GroupInfo>, context: &HashSet<String>) -> Option<Scored> {
        let group = by_cwd.get(&key(&path));
        // Gruppenname zählt wie ein Ordnername: „kad“ findet die Gruppe Kadrell auch in claude-agent-overview.
        let by_name = group.and_then(|g| fi::rank(q, &format!("{path}/{}", g.name)));
        let rank = [fi::rank(q, &path), by_name].into_iter().flatten().min()?;
        let bonus = group.map_or(0.0, |g| 0.5 + self.inputs.counts.get(&g.id).copied().unwrap_or(0) as f64 * 0.1);
        let score = self.index.frecency(&path, self.inputs.now) + bonus;
        Some(Scored { ctx: context.contains(&key(&path)), rank, score, path, tag })
    }

    /// Von Extensions gelieferte Einträge, bei Eingabe nach Titel und Gruppe gefiltert und sortiert.
    fn clone_matches(&self, q: &str) -> Vec<Candidate> {
        if q.is_empty() {
            return self.inputs.clones.clone();
        }
        let mut hits: Vec<(u8, &Candidate)> = self
            .inputs
            .clones
            .iter()
            .filter_map(|c| Some((fi::rank(q, &format!("{} {}", c.title.as_deref().unwrap_or(""), c.tag))?, c)))
            .collect();
        hits.sort_by_key(|(rank, _)| *rank);
        hits.into_iter().map(|(_, c)| c.clone()).collect()
    }

    /// Gibt es den getippten Ordner noch nicht, steht „Ordner anlegen“ in der Liste: ganz oben, wenn kein Treffer den neuen
    /// Namen enthält (dann tippt man offensichtlich etwas Neues), sonst ganz unten, damit ⏎ wie gewohnt den besten Treffer
    /// startet.
    fn offer_create(&mut self, q: &str) {
        self.create_target = if q.is_empty() { None } else { fi::creation_target(&q.replace('\\', "/"), &self.inputs.start_folder, &self.inputs.home) };
        let Some(target) = self.create_target.clone() else { return };
        let name = last_component(&target).to_string();
        let row = Candidate { select: Some(Select::Create), tag: trd!("neu"), ..Candidate::folder(&target, None, "") };
        let matched = self.candidates.iter().any(|c| c.select.is_none() && fi::rank(&name, &c.path).unwrap_or(3) <= 2);
        if matched {
            self.candidates.push(row);
        } else {
            self.candidates.insert(0, row);
        }
    }

    // MARK: Bedienung

    pub fn move_by(&mut self, delta: isize) {
        let last = self.candidates.len().saturating_sub(1) as isize;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }

    /// Tab: markierten Pfad ins Feld übernehmen, ein „/“ danach zeigt seine Unterordner.
    pub fn tab(&self) -> Option<String> {
        self.candidates.get(self.selected).map(|c| short_path(&c.path, &self.inputs.home))
    }

    pub fn pick(&mut self, i: usize) -> Outcome {
        let Some(c) = self.candidates.get(i).cloned() else { return Outcome::Nothing };
        match c.select {
            Some(Select::Create) => self.confirm_create(&c.path),
            Some(Select::Extension { name, id }) => Outcome::Extension { name, id },
            None => Outcome::Start { group: c.group.map(|g| g.id), path: c.path },
        }
    }

    /// ⏎: markierten Treffer starten, ohne Treffer einen ausgeschrieben getippten Ordner; existiert der nicht, Hinweis statt
    /// stillem No-op (⌘⏎ legt ihn an).
    pub fn start(&mut self) -> Outcome {
        if !self.candidates.is_empty() {
            return self.pick(self.selected);
        }
        let dir = fi::normalize(&self.query.replace('\\', "/"), false, &self.inputs.home);
        if dir.is_empty() {
            return Outcome::Nothing;
        }
        if fi::is_directory(&dir) {
            return Outcome::Start { group: None, path: dir };
        }
        self.not_found_path = Some(dir);
        Outcome::Beep
    }

    /// Erstes ⏎ auf „Ordner anlegen“ fragt nach, das zweite legt an und startet.
    fn confirm_create(&mut self, path: &str) -> Outcome {
        if self.confirm_path.as_deref() != Some(path) {
            self.confirm_path = Some(path.to_string());
            self.create_error = None;
            return Outcome::Nothing;
        }
        self.create(path)
    }

    fn create(&mut self, dir: &str) -> Outcome {
        if let Err(e) = std::fs::create_dir_all(dir) {
            self.create_error = Some(e.to_string());
            return Outcome::Beep;
        }
        self.not_found_path = None;
        self.confirm_path = None;
        Outcome::Start { group: None, path: dir.to_string() }
    }

    /// ⌘⏎: den getippten, noch fehlenden Ordner ohne Nachfrage anlegen und dort starten (nur unter Home).
    pub fn create_and_start(&mut self) -> Outcome {
        match self.create_target.clone() {
            Some(dir) => self.create(&dir),
            None => Outcome::Beep,
        }
    }

    /// Ausgangsordner von ⌘O: der markierte Eintrag, sonst der Startordner.
    pub fn browse_start(&self) -> String {
        self.candidates.get(self.selected).map_or_else(|| self.inputs.start_folder.clone(), |c| c.path.clone())
    }

    // MARK: Anzeige

    /// Titel der Zeile: Extension-Titel, Gruppenname, letzte Pfadkomponente; bei „Ordner anlegen“ der Hinweistext.
    pub fn title(&self, c: &Candidate) -> String {
        if c.is_create() {
            let name = last_component(&c.path);
            return if self.confirm_path.as_deref() == Some(c.path.as_str()) {
                trd!("⏎ noch einmal: „%@“ anlegen und starten", name)
            } else {
                trd!("Neuen Ordner „%@“ anlegen", name)
            };
        }
        c.title.clone().or_else(|| c.group.as_ref().map(|g| g.name.clone())).unwrap_or_else(|| last_component(&c.path).to_string())
    }

    pub fn detail(&self, c: &Candidate) -> String {
        c.detail.clone().unwrap_or_else(|| short_path(&c.path, &self.inputs.home))
    }

    /// Rechts in der Zeile: „<n> Sessions“ bei einer Gruppe, sonst die Herkunft.
    pub fn right(&self, c: &Candidate) -> String {
        match &c.group {
            Some(g) => trd!("%lld Sessions", self.inputs.counts.get(&g.id).copied().unwrap_or(0)),
            None => c.tag.clone(),
        }
    }

    pub fn short(&self, path: &str) -> String {
        short_path(path, &self.inputs.home)
    }
}

struct Scored {
    ctx: bool,
    rank: u8,
    score: f64,
    path: String,
    tag: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const NOW: f64 = 1_800_000_000.0;

    /// Wegwerf-Ordner, spielt Home und Startordner, damit nichts unter dem echten Home entsteht.
    struct Root(String);

    impl Root {
        fn new() -> Root {
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!("kadrell-newsession-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(dir.join("projects/kadrell")).unwrap();
            Root(dir.to_string_lossy().replace('\\', "/"))
        }

        fn sub(&self, rel: &str) -> String {
            format!("{}/{rel}", self.0)
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn group(id: &str, name: &str, cwd: &str) -> GroupInfo {
        GroupInfo { id: id.into(), name: name.into(), cwd: cwd.into(), host: None }
    }

    fn model_with(root: &Root, groups: Vec<GroupInfo>, counts: &[(&str, usize)]) -> Model {
        let inputs = Inputs {
            groups,
            counts: counts.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            clones: vec![],
            start_folder: root.sub("projects/"),
            home: root.0.clone(),
            now: NOW,
        };
        Model::new(inputs, FolderIndex::load(&root.0, &root.0), None)
    }

    fn model(root: &Root) -> Model {
        model_with(root, vec![], &[])
    }

    /// Neuer Name ohne passenden Treffer: „Ordner anlegen“ steht oben, erstes ⏎ fragt nach, zweites legt an und startet.
    #[test]
    fn create_row_needs_second_return() {
        let root = Root::new();
        let mut m = model(&root);
        m.set_query("wetterbot");
        let target = root.sub("projects/wetterbot");
        assert!(m.candidates[0].is_create());
        assert_eq!(m.candidates[0].path, target);

        assert_eq!(m.start(), Outcome::Nothing);
        assert_eq!(m.confirm_path.as_deref(), Some(target.as_str()));
        assert!(!fi::is_directory(&target));

        assert_eq!(m.start(), Outcome::Start { group: None, path: target.clone() });
        assert!(fi::is_directory(&target));
    }

    /// Passt ein vorhandener Ordner zum Namen, startet ⏎ weiter dort; „Ordner anlegen“ steht ganz unten.
    #[test]
    fn existing_match_stays_first() {
        let root = Root::new();
        let mut m = model(&root);
        m.set_query(&root.sub("projects/kad"));
        assert_eq!(m.candidates.first().map(|c| c.path.clone()), Some(root.sub("projects/kadrell")));
        assert!(m.candidates.last().unwrap().is_create());
    }

    /// Tippen nach der Nachfrage hebt sie wieder auf.
    #[test]
    fn confirm_resets_on_query_change() {
        let root = Root::new();
        let mut m = model(&root);
        m.set_query("wetterbot");
        m.start();
        assert!(m.confirm_path.is_some());
        m.set_query("wetterbot2");
        assert!(m.confirm_path.is_none());
    }

    /// ⌘⏎ legt ohne Nachfrage an und startet.
    #[test]
    fn command_return_creates_directly() {
        let root = Root::new();
        let mut m = model(&root);
        m.set_query(&root.sub("neu/tief"));
        let dir = root.sub("neu/tief");
        assert_eq!(m.create_and_start(), Outcome::Start { group: None, path: dir.clone() });
        assert!(fi::is_directory(&dir));
    }

    /// Außerhalb von Home: kein Angebot, ⏎ meldet den fehlenden Ordner, ⌘⏎ legt nichts an.
    #[test]
    fn outside_home_only_reports_missing() {
        let root = Root::new();
        let mut m = model(&root);
        let outside = format!("{}/kadrell-test-gibt-es-nicht", fi::normalize(&std::env::temp_dir().parent().unwrap().to_string_lossy().replace('\\', "/"), false, ""));
        m.set_query(&outside);
        assert!(m.candidates.is_empty());
        assert_eq!(m.start(), Outcome::Beep);
        assert_eq!(m.not_found_path.as_deref(), Some(outside.as_str()));
        assert_eq!(m.create_and_start(), Outcome::Beep);
        assert!(!std::path::Path::new(&outside).exists());
        m.set_query(&format!("{outside}-auch"));
        assert!(m.not_found_path.is_none(), "Hinweis verschwindet beim Tippen");
    }

    #[test]
    fn creation_failure_keeps_the_error_and_beeps() {
        let root = Root::new();
        std::fs::write(root.sub("projects/datei"), "x").unwrap();
        let mut m = model(&root);
        m.set_query("neu");
        m.create_target = Some(root.sub("projects/datei/unter"));
        assert_eq!(m.create_and_start(), Outcome::Beep);
        assert!(m.create_error.is_some());
        m.set_query("neu2");
        assert!(m.create_error.is_none());
    }

    #[test]
    fn groups_rank_by_name_and_sessions_and_show_counts() {
        let root = Root::new();
        let a = group("a", "Kadrell", &root.sub("projects/kadrell"));
        let b = group("b", "Anderes", &root.sub("projects"));
        let mut m = model_with(&root, vec![a.clone(), b], &[("a", 3), ("b", 0)]);
        m.set_query("kad");
        let first = &m.candidates[0];
        assert_eq!(first.path, root.sub("projects/kadrell"));
        assert_eq!(first.group.as_ref(), Some(&a));
        assert_eq!(m.title(first), "Kadrell");
        assert_eq!(m.right(first), "3 Sessions");
        m.set_query("");
        assert_eq!(m.candidates[0].path, root.sub("projects/kadrell"), "mehr Sessions zuerst");
    }

    #[test]
    fn context_comes_first_and_finder_only_while_untouched() {
        let root = Root::new();
        let mut m = model_with(&root, vec![group("a", "Kadrell", &root.sub("projects/kadrell"))], &[]);
        m.set_finder(&root.0);
        assert_eq!(m.candidates[0].path, root.0);
        assert_eq!(m.candidates[0].tag, crate::lang::file_manager());
        m.set_query("kad");
        m.set_finder(&root.sub("projects"));
        assert_ne!(m.candidates[0].path, root.sub("projects"), "während getippt wird, nicht umsortieren");
    }

    #[test]
    fn extension_clones_sit_after_folders_when_empty_and_before_when_typed() {
        let root = Root::new();
        let clone = Candidate {
            title: Some("acme/kadrell-fork".into()),
            tag: "GitHub".into(),
            select: Some(Select::Extension { name: "gh".into(), id: "1".into() }),
            ..Candidate::folder("ext:gh:1", None, "")
        };
        let inputs = Inputs { clones: vec![clone], groups: vec![group("a", "Kadrell", &root.sub("projects/kadrell"))], counts: HashMap::new(), start_folder: root.sub("projects/"), home: root.0.clone(), now: NOW };
        let mut m = Model::new(inputs, FolderIndex::load(&root.0, &root.0), None);
        assert_eq!(m.candidates.last().unwrap().path, "ext:gh:1");
        m.set_query("kad");
        assert_eq!(m.candidates[0].path, "ext:gh:1");
        assert_eq!(m.pick(0), Outcome::Extension { name: "gh".into(), id: "1".into() });
    }

    #[test]
    fn typed_existing_folder_without_hit_starts_there_and_empty_does_nothing() {
        let root = Root::new();
        let mut m = model(&root);
        assert_eq!(m.start(), Outcome::Nothing);
        m.set_query("   ");
        assert_eq!(m.start(), Outcome::Nothing);
    }

    #[test]
    fn arrows_stay_inside_and_tab_takes_the_short_path() {
        let root = Root::new();
        let mut m = model_with(&root, vec![group("a", "Kadrell", &root.sub("projects/kadrell"))], &[]);
        m.move_by(-1);
        assert_eq!(m.selected, 0);
        m.move_by(5);
        assert_eq!(m.selected, m.candidates.len() - 1);
        m.selected = 0;
        assert_eq!(m.tab().as_deref(), Some("~/projects/kadrell"));
    }

    #[test]
    fn path_syntax_expands_abbreviations_and_maps_groups() {
        let root = Root::new();
        let g = group("a", "Kadrell", &root.sub("projects/kadrell"));
        let mut m = model_with(&root, vec![g.clone()], &[]);
        m.set_query("~/p/kad");
        assert_eq!(m.candidates[0].path, root.sub("projects/kadrell"));
        assert_eq!(m.candidates[0].group.as_ref(), Some(&g));
    }
}
