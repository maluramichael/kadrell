//! Gruppen: Modell, Speicher (`groups.json`), Zuordnung der Sessions nach Ordner, Favoriten, Reihenfolge und
//! Sortierung des Baums. Quelle: `Model/Group.swift`.

use crate::json_file::{LoadResult, load_array, save_array};
use crate::paths::{base_name, same_folder};
use crate::session::{Session, SessionStatus};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Farbpaletten für Gruppen (Auswahl im Dialog „Gruppe bearbeiten“), aus den offiziellen Paletten übernommen.
pub const PALETTES: [(&str, &[&str]); 7] = [
    (
        "Catppuccin",
        &["#fab387", "#cba6f7", "#f5c2e7", "#89b4fa", "#a6e3a1", "#94e2d5", "#f9e2af", "#eba0ac", "#b4befe", "#74c7ec", "#89dceb", "#f38ba8", "#f2cdcd", "#f5e0dc"],
    ),
    (
        "Tailwind",
        &[
            "#f87171", "#fb923c", "#fbbf24", "#facc15", "#a3e635", "#4ade80", "#34d399", "#2dd4bf", "#22d3ee", "#38bdf8", "#60a5fa", "#818cf8", "#a78bfa", "#c084fc",
            "#e879f9", "#f472b6", "#fb7185",
        ],
    ),
    ("Nord", &["#8fbcbb", "#88c0d0", "#81a1c1", "#5e81ac", "#bf616a", "#d08770", "#ebcb8b", "#a3be8c", "#b48ead"]),
    ("Dracula", &["#8be9fd", "#50fa7b", "#ffb86c", "#ff79c6", "#bd93f9", "#ff5555", "#f1fa8c"]),
    ("Gruvbox", &["#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b", "#8ec07c", "#fe8019", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a", "#d65d0e"]),
    ("Rosé Pine", &["#eb6f92", "#f6c177", "#ebbcba", "#31748f", "#9ccfd8", "#c4a7e7"]),
    ("Okabe-Ito", &["#e69f00", "#56b4e9", "#009e73", "#f0e442", "#0072b2", "#d55e00", "#cc79a7"]),
];

/// Die Palette, aus der neue Gruppen ihre Farbe bekommen (Catppuccin, `Theme.palette`).
pub const PALETTE: &[&str] = PALETTES[0].1;

/// Eine Gruppe von Sessions (meist ein Projektordner). Die Felder stehen alphabetisch wie in der Datei der
/// Mac-App. Fehlen `sessionIds`, `favorite` oder `host` in einer alten Datei, gelten leer, `false` und lokal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub color: String,
    pub cwd: String,
    /// Das Herz: hält als Einziges eine Gruppe ohne Sessions am Leben.
    #[serde(default)]
    pub favorite: bool,
    /// Remote-Gruppe: ssh-Host, alle Sessions darin hängen an dessen tmux. Fehlt in alten Dateien, dann lokal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub session_ids: Vec<String>,
}

impl Group {
    pub fn new(id: &str, name: &str, color: &str, cwd: &str, session_ids: &[&str]) -> Group {
        Group {
            id: id.into(),
            name: name.into(),
            color: color.into(),
            cwd: cwd.into(),
            session_ids: session_ids.iter().map(|s| s.to_string()).collect(),
            favorite: false,
            host: None,
        }
    }

    pub fn is_favorite(&self) -> bool {
        self.favorite
    }
}

const SCHEMA_VERSION: i64 = 1;

/// Die Gruppen, persistiert als `groups.json`. Jede ändernde Methode speichert sofort, Fehler landen in
/// [`GroupStore::last_error`] statt zu verschwinden.
#[derive(Debug, Clone)]
pub struct GroupStore {
    pub groups: Vec<Group>,
    path: PathBuf,
    last_error: Option<String>,
    /// Der Umschlag nennt eine höhere Version, als diese Kadrell-Version kennt: nur lesen, nie überschreiben.
    read_only: bool,
}

impl GroupStore {
    /// Lädt `path`. Fehlt die Datei, startet der Speicher leer; eine kaputte wird gesichert (siehe `json_file`).
    pub fn open(path: impl Into<PathBuf>) -> GroupStore {
        let path = path.into();
        let LoadResult { items, newer_than_known, message, .. } = load_array::<Group>(&path, SCHEMA_VERSION);
        let mut store = GroupStore { groups: items, path, last_error: message, read_only: newer_than_known };
        store.dedupe_sessions_across_groups();
        store
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Letzter Lade- oder Speicherfehler (kaputte Datei, Schreibfehler). Kein Abgleich räumt ihn ab.
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Dieselbe sessionId darf in höchstens einer Gruppe stehen: eine kaputte Datei oder ein Schreibfehler könnte
    /// sie doppelt eingetragen haben. Die erste Gruppe behält die Id, aus den anderen fliegt sie raus.
    fn dedupe_sessions_across_groups(&mut self) {
        let mut seen = HashSet::new();
        let mut changed = false;
        for g in &mut self.groups {
            let before = g.session_ids.len();
            g.session_ids.retain(|id| seen.insert(id.clone()));
            changed |= g.session_ids.len() != before;
        }
        if changed {
            self.save();
        }
    }

    /// Schreibt `groups.json`. Eine Datei mit neuerer Schema-Version bleibt unberührt.
    pub fn save(&mut self) {
        if self.read_only {
            return;
        }
        if let Err(message) = save_array(&self.groups, &self.path, SCHEMA_VERSION) {
            self.last_error = Some(message);
        }
    }

    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn group_for_session(&self, session_id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.session_ids.iter().any(|s| s == session_id))
    }

    /// Index der Gruppe, die diese Session enthält.
    pub fn index_for_session(&self, session_id: &str) -> Option<usize> {
        self.groups.iter().position(|g| g.session_ids.iter().any(|s| s == session_id))
    }

    /// Lokale Gruppe für einen Ordner.
    pub fn group_for_cwd(&self, cwd: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.host.is_none() && same_folder(&g.cwd, cwd))
    }

    pub fn group_for_host(&self, host: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.host.as_deref() == Some(host))
    }

    /// Erste Farbe der Palette, die noch keine Gruppe hat, sonst reihum.
    pub fn next_color(&self) -> String {
        let free = PALETTE.iter().find(|c| !self.groups.iter().any(|g| g.color == **c));
        free.unwrap_or(&PALETTE[self.groups.len() % PALETTE.len()]).to_string()
    }

    /// Name, den eine neu angelegte Gruppe für `cwd` ungefragt bekommt (auch zum Erkennen unveränderter Gruppen).
    pub fn default_name(cwd: &str, host: Option<&str>) -> String {
        host.map(str::to_string).unwrap_or_else(|| base_name(cwd))
    }

    /// Neue Gruppe für `cwd` mit der nächsten freien Farbe, noch nicht gespeichert.
    pub fn make_group(&self, cwd: &str, name: Option<&str>) -> Group {
        Group {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.map(str::to_string).unwrap_or_else(|| GroupStore::default_name(cwd, None)),
            color: self.next_color(),
            cwd: cwd.into(),
            session_ids: Vec::new(),
            favorite: false,
            host: None,
        }
    }

    /// Gruppe für einen ssh-Host, heißt wie der Host; `home` ist nur Platzhalter-Ordner für lokale Aktionen.
    pub fn make_group_for_host(&self, host: &str, home: &str) -> Group {
        Group { host: Some(host.into()), ..self.make_group(home, Some(host)) }
    }

    /// Ordnet Sessions ohne Gruppe der Gruppe mit gleichem Ordner (Remote: gleichem Host) zu, legt sonst eine neue
    /// an und räumt leere Gruppen ohne Herz weg. `home` ist der Platzhalter-Ordner neuer Remote-Gruppen. Gibt
    /// zurück, ob sich etwas geändert hat.
    ///
    /// Prunt absichtlich keine `session_ids` gegen `sessions`: das übernehmen [`GroupStore::remove_session`] und
    /// [`GroupStore::remove`] beim echten Schließen. Ein Abgleich mit unvollständiger Liste (Ladefehler, Teilverlust
    /// von `sessions.json`) hätte sonst ganze Gruppen leergeräumt. Eine leere Liste ist eher ein Aussetzer als
    /// „alle Sessions weg“: dann passiert nichts.
    pub fn assign(&mut self, sessions: &[Session], home: &str) -> bool {
        if sessions.is_empty() {
            return false;
        }
        let before = self.groups.clone();
        for s in sessions {
            if self.group_for_session(&s.id).is_none() {
                self.place(s, home);
            }
        }
        self.remove_empty_groups();
        let changed = self.groups != before;
        if changed {
            self.save();
        }
        changed
    }

    fn place(&mut self, s: &Session, home: &str) {
        let existing = self.groups.iter().position(|g| match &s.host {
            Some(h) => g.host.as_ref() == Some(h),
            None => g.host.is_none() && same_folder(&g.cwd, &s.cwd),
        });
        match existing {
            Some(i) => self.groups[i].session_ids.push(s.id.clone()),
            None => {
                let mut g = match &s.host {
                    Some(h) => self.make_group_for_host(h, home),
                    None => self.make_group(&s.cwd, None),
                };
                g.session_ids = vec![s.id.clone()];
                self.groups.push(g);
            }
        }
    }

    /// Ohne Sessions hat eine Gruppe keinen Zweck, sonst sammelt die Datei Ordner von längst beendeten Sessions.
    /// Das Herz ist das Einzige, was eine leere Gruppe hält: ein eigener Name oder eine eigene Farbe reicht nicht.
    fn remove_empty_groups(&mut self) {
        self.groups.retain(|g| !g.session_ids.is_empty() || g.favorite);
    }

    /// Sessions in eine andere Gruppe verschieben (Baum, Kontextmenü, `kadrell move`): sie hängen hinten an und
    /// behalten ihren Ordner, der Prozess läuft weiter. Eine dadurch leere Gruppe ohne Herz geht sofort weg.
    /// Doppelte Ids zählen einmal, Sessions, die schon dort stehen, bleiben. Gibt zurück, ob sich etwas geändert hat.
    pub fn move_sessions(&mut self, ids: &[String], group_id: &str) -> bool {
        let Some(target) = self.groups.iter().position(|g| g.id == group_id) else { return false };
        let mut seen = HashSet::new();
        let moving: Vec<String> = ids.iter().filter(|id| !self.groups[target].session_ids.contains(id) && seen.insert(id.as_str())).cloned().collect();
        if moving.is_empty() {
            return false;
        }
        for g in &mut self.groups {
            g.session_ids.retain(|s| !moving.contains(s));
        }
        self.groups[target].session_ids.extend(moving);
        self.remove_empty_groups();
        self.save();
        true
    }

    pub fn add(&mut self, group: Group) {
        self.groups.push(group);
        self.save();
    }

    /// Von Hand angelegte Gruppe (Menü, Palette, `kadrell new-group`): sie bekommt das Herz, damit sie auch ohne
    /// Session stehen bleibt, bis man sie schließt. Neue Sessions in ihrem Ordner landen von selbst darin.
    pub fn add_kept(&mut self, mut group: Group) {
        group.favorite = true;
        self.add(group);
    }

    /// Ersetzt die Gruppe mit gleicher Id (Name, Farbe, Ordner, Herz).
    pub fn update(&mut self, group: Group) {
        let Some(i) = self.groups.iter().position(|g| g.id == group.id) else { return };
        self.groups[i] = group;
        self.save();
    }

    /// Benennt eine Gruppe um.
    pub fn rename(&mut self, id: &str, name: &str) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.name = name.to_string();
            self.save();
        }
    }

    /// Setzt die Farbe einer Gruppe.
    pub fn set_color(&mut self, id: &str, color: &str) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.color = color.to_string();
            self.save();
        }
    }

    pub fn remove(&mut self, id: &str) {
        self.groups.retain(|g| g.id != id);
        self.save();
    }

    pub fn toggle_favorite(&mut self, id: &str) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
            g.favorite = !g.favorite;
            self.save();
        }
    }

    /// Echtes Schließen: die Session verschwindet aus allen Gruppen. Eine leere Gruppe bleibt bis zum nächsten
    /// Abgleich stehen ([`GroupStore::assign`] räumt sie dann weg).
    pub fn remove_session(&mut self, session_id: &str) {
        for g in &mut self.groups {
            g.session_ids.retain(|s| s != session_id);
        }
        self.save();
    }

    /// Hängt die Session hinten an die Gruppe, vorher raus aus allen anderen.
    pub fn attach(&mut self, session_id: &str, group_id: &str) {
        for g in &mut self.groups {
            g.session_ids.retain(|s| s != session_id);
        }
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == group_id) {
            g.session_ids.push(session_id.to_string());
        }
        self.save();
    }

    /// Sortieren per Ziehen: die Session nimmt den Platz von `target` in derselben Gruppe ein. Über Gruppengrenzen
    /// passiert nichts.
    pub fn move_session(&mut self, id: &str, target: &str) {
        let Some(g) = self.groups.iter().position(|g| g.session_ids.iter().any(|s| s == id)) else { return };
        let ids = &mut self.groups[g].session_ids;
        let (Some(from), Some(to)) = (ids.iter().position(|s| s == id), ids.iter().position(|s| s == target)) else { return };
        if from == to {
            return;
        }
        let moved = ids.remove(from);
        ids.insert(to, moved);
        self.save();
    }

    /// Die Gruppe nimmt den Platz von `target` ein.
    pub fn move_group(&mut self, id: &str, target: &str) {
        let (Some(from), Some(to)) = (self.groups.iter().position(|g| g.id == id), self.groups.iter().position(|g| g.id == target)) else { return };
        if from == to {
            return;
        }
        let moved = self.groups.remove(from);
        self.groups.insert(to, moved);
        self.save();
    }

    /// Sessions in Baumreihenfolge, Gruppe für Gruppe.
    pub fn ordered_keys(&self) -> Vec<String> {
        self.groups.iter().flat_map(|g| g.session_ids.iter().cloned()).collect()
    }
}

raw_enum! {
    /// Sortierung im Baum, umschaltbar in der Leiste. `Off` zeigt die von Hand gezogene Reihenfolge.
    SidebarSort { Off = "off", Alpha = "alpha", Status = "status" }
}

impl SidebarSort {
    /// Nächster Modus beim Umschalten: aus, A-Z, Status, wieder aus.
    pub fn next(self) -> SidebarSort {
        let all = SidebarSort::ALL;
        all[(all.iter().position(|s| *s == self).unwrap_or(0) + 1) % all.len()]
    }

    /// Wartet auf Antwort vor Fehler vor arbeitet vor fertig.
    fn rank(status: SessionStatus) -> i32 {
        match status {
            SessionStatus::Waiting => 0,
            SessionStatus::Error => 1,
            SessionStatus::Running => 2,
            SessionStatus::Idle => 3,
        }
    }

    /// Zwei Sessions nach Status, bei gleichem Status die neuere zuerst: frisch Gestartetes taucht oben auf und
    /// rutscht nach unten, sobald es fertig ist. `Alpha` vergleicht die Titel.
    pub fn less(x: &Session, y: &Session, sort: SidebarSort) -> bool {
        if sort == SidebarSort::Alpha {
            return localized_standard_compare(&x.title(), &y.title()).is_lt();
        }
        let (rx, ry) = (Self::rank(x.status()), Self::rank(y.status()));
        if rx == ry { x.started_at > y.started_at } else { rx < ry }
    }

    /// Sortiert Gruppen und die Sessions darin, ohne `groups.json` anzufassen. Gleichstand behält die bisherige
    /// Reihenfolge. Eine Gruppe erbt den Rang ihrer dringendsten Session, bei Gleichstand zählt ihre jüngste.
    pub fn apply(self, groups: &[Group], sessions: &HashMap<String, Session>) -> Vec<Group> {
        if self == SidebarSort::Off {
            return groups.to_vec();
        }
        let sorted: Vec<Group> = groups
            .iter()
            .map(|g| {
                let ids = stable_sorted(g.session_ids.clone(), |a, b| match (sessions.get(a), sessions.get(b)) {
                    (Some(x), Some(y)) => Self::less(x, y, self),
                    _ => false,
                });
                Group { session_ids: ids, ..g.clone() }
            })
            .collect();
        stable_sorted(sorted, |a, b| {
            if self == SidebarSort::Alpha {
                return localized_standard_compare(&a.name, &b.name).is_lt();
            }
            let (x, y) = (Self::group_key(a, sessions), Self::group_key(b, sessions));
            if x.0 == y.0 { x.1 > y.1 } else { x.0 < y.0 }
        })
    }

    fn group_key(g: &Group, sessions: &HashMap<String, Session>) -> (i32, f64) {
        let members: Vec<&Session> = g.session_ids.iter().filter_map(|id| sessions.get(id)).collect();
        let rank = members.iter().map(|s| Self::rank(s.status())).min().unwrap_or(4);
        let newest = members.iter().map(|s| s.started_at).fold(0.0, f64::max);
        (rank, newest)
    }
}

/// Stabil sortieren mit einer „kommt vor“-Relation: bei Gleichstand entscheidet die bisherige Position. Anders als
/// `slice::sort_by` darf die Relation unvollständig sein (fehlende Sessions), ohne dass etwas abbricht.
pub fn stable_sorted<T>(mut items: Vec<T>, before: impl Fn(&T, &T) -> bool) -> Vec<T> {
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 && before(&items[j], &items[j - 1]) {
            items.swap(j, j - 1);
            j -= 1;
        }
    }
    items
}

/// Eine Zeile der flachen Liste: die Session und die Gruppe, die ihr Farbe und Projektnamen liefert.
#[derive(Debug, Clone, Copy)]
pub struct FlatRow<'a> {
    pub session: &'a Session,
    pub group: &'a Group,
}

/// Gruppierung aus: alle Sessions in einer flachen Liste, quer über die Gruppen.
pub struct SidebarFlat;

impl SidebarFlat {
    /// `order` ist die von Hand gezogene Reihenfolge (`Settings::sidebar_flat_order`), nur bei `Off` maßgeblich.
    /// Sessions, die noch nicht darin stehen, kommen oben dazu, die neueste zuerst.
    pub fn rows<'a>(groups: &'a [Group], sessions: &'a HashMap<String, Session>, sort: SidebarSort, order: &[String]) -> Vec<FlatRow<'a>> {
        let all: Vec<FlatRow<'a>> =
            groups.iter().flat_map(|g| g.session_ids.iter().filter_map(move |id| sessions.get(id).map(|session| FlatRow { session, group: g }))).collect();
        if sort != SidebarSort::Off {
            return stable_sorted(all, |a, b| SidebarSort::less(a.session, b.session, sort));
        }
        let rank: HashMap<&str, usize> = order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
        stable_sorted(all, |a, b| match (rank.get(a.session.id.as_str()), rank.get(b.session.id.as_str())) {
            (Some(x), Some(y)) => x < y,
            (None, None) => a.session.started_at > b.session.started_at,
            (None, Some(_)) => true,
            (Some(_), None) => false,
        })
    }
}

/// Vergleich wie `localizedStandardCompare` im Finder: ohne Rücksicht auf Groß-/Kleinschreibung und Akzente, Zahlen
/// nach ihrem Wert (`a2` vor `a10`). Eine Näherung für lateinische Schrift, kein vollständiges Unicode-Collation.
pub fn localized_standard_compare(a: &str, b: &str) -> std::cmp::Ordering {
    let (x, y) = (fold(a), fold(b));
    let (mut i, mut j) = (0, 0);
    while i < x.len() && j < y.len() {
        if x[i].is_ascii_digit() && y[j].is_ascii_digit() {
            let (ni, nj) = (digits_end(&x, i), digits_end(&y, j));
            let ord = compare_numbers(&x[i..ni], &y[j..nj]);
            if ord.is_ne() {
                return ord;
            }
            (i, j) = (ni, nj);
        } else {
            let ord = x[i].cmp(&y[j]);
            if ord.is_ne() {
                return ord;
            }
            (i, j) = (i + 1, j + 1);
        }
    }
    (x.len() - i).cmp(&(y.len() - j))
}

fn digits_end(chars: &[char], start: usize) -> usize {
    start + chars[start..].iter().take_while(|c| c.is_ascii_digit()).count()
}

fn compare_numbers(a: &[char], b: &[char]) -> std::cmp::Ordering {
    let trim = |s: &[char]| s.iter().skip_while(|c| **c == '0').copied().collect::<Vec<char>>();
    let (a, b) = (trim(a), trim(b));
    a.len().cmp(&b.len()).then_with(|| a.cmp(&b))
}

/// Kleinbuchstaben ohne Akzente (Latin-1), `ß` wird `ss`.
fn fold(s: &str) -> Vec<char> {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .flat_map(|c| match c {
            'à'..='å' => vec!['a'],
            'ç' => vec!['c'],
            'è'..='ë' => vec!['e'],
            'ì'..='ï' => vec!['i'],
            'ñ' => vec!['n'],
            'ò'..='ö' | 'ø' => vec!['o'],
            'ù'..='ü' => vec!['u'],
            'ý' | 'ÿ' => vec!['y'],
            'ß' => vec!['s', 's'],
            other => vec![other],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testutil::TempDir;

    const HOME: &str = "/home/test";

    fn session(id: &str, cwd: &str) -> Session {
        Session::new(id, cwd, 0.0, id, id)
    }

    fn url(d: &TempDir) -> PathBuf {
        d.path().join("groups.json")
    }

    fn reload(store: &GroupStore) -> GroupStore {
        GroupStore::open(store.path())
    }

    fn cwds(store: &GroupStore) -> Vec<&str> {
        store.groups.iter().map(|g| g.cwd.as_str()).collect()
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn auto_assign_by_cwd_and_persistence() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        let all = [session("s1", "/p/homelab"), session("s2", "/p/homelab"), session("s3", "/p/malura")];
        assert!(store.assign(&all, HOME));
        assert_eq!(store.groups.len(), 2);
        assert_eq!(store.groups[0].name, "homelab");
        assert_eq!(store.groups[0].session_ids, ["s1", "s2"]);
        assert_eq!(store.groups[1].name, "malura");
        assert_eq!(store.groups[0].color, PALETTE[0]);
        assert_eq!(store.groups[1].color, PALETTE[1]);
        assert!(!store.assign(&all, HOME));
        assert_eq!(reload(&store).groups, store.groups);
    }

    #[test]
    fn edit_survives_reload_and_session_keeps_group() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("s1", "/p/homelab")], HOME);
        let mut g = store.groups[0].clone();
        g.name = "Heim".into();
        g.color = "#cba6f7".into();
        g.cwd = "/p/other".into();
        store.update(g);
        assert_eq!(reload(&store).groups[0].name, "Heim");
        assert!(!store.assign(&[session("s1", "/p/homelab")], HOME), "Session bleibt trotz geändertem Gruppen-cwd in ihrer Gruppe");
        assert_eq!(store.groups.len(), 1);
    }

    #[test]
    fn move_session_and_group_persists() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        let all = [session("s1", "/p/a"), session("s2", "/p/a"), session("s3", "/p/a"), session("t1", "/p/b")];
        store.assign(&all, HOME);
        store.move_session("s1", "s3");
        assert_eq!(store.groups[0].session_ids, ["s2", "s3", "s1"]);
        store.move_session("s1", "s2");
        assert_eq!(store.groups[0].session_ids, ["s1", "s2", "s3"]);
        store.move_session("s1", "t1");
        assert_eq!(store.groups[0].session_ids, ["s1", "s2", "s3"], "über Gruppengrenzen passiert nichts");
        let (first, second) = (store.groups[0].id.clone(), store.groups[1].id.clone());
        store.move_group(&second, &first);
        assert_eq!(cwds(&store), ["/p/b", "/p/a"]);
        assert_eq!(reload(&store).groups, store.groups);
        assert!(!store.assign(&all, HOME), "Reihenfolge übersteht den nächsten Abgleich");
    }

    /// #747/#775: assign() prunt `session_ids` nicht gegen die übergebene Liste.
    #[test]
    fn assign_does_not_prune_missing_session_ids() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("s1", "/p/a"), session("s2", "/p/a"), session("t1", "/p/b")], HOME);
        assert_eq!(store.groups[0].session_ids, ["s1", "s2"]);
        assert!(!store.assign(&[session("s2", "/p/a"), session("t1", "/p/b")], HOME));
        assert_eq!(store.groups[0].session_ids, ["s1", "s2"]);
        assert!(!store.assign(&[session("t1", "/p/b")], HOME));
        assert_eq!(cwds(&store), ["/p/a", "/p/b"]);
        assert_eq!(reload(&store).groups[0].session_ids, ["s1", "s2"]);
    }

    #[test]
    fn remove_session_then_assign_removes_empty_unmodified_group() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("s1", "/p/a"), session("s2", "/p/a"), session("t1", "/p/b")], HOME);
        store.remove_session("s1");
        assert_eq!(store.groups[0].session_ids, ["s2"]);
        store.remove_session("s2");
        assert!(store.groups[0].session_ids.is_empty());
        assert!(store.assign(&[session("t1", "/p/b")], HOME));
        assert_eq!(cwds(&store), ["/p/b"]);
        assert_eq!(cwds(&reload(&store)), ["/p/b"]);
    }

    #[test]
    fn kept_empty_group_survives_and_collects_sessions() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("t1", "/p/b")], HOME);
        let neu = store.make_group("/p/neu", Some("Neu"));
        store.add_kept(neu);
        assert!(store.groups[1].is_favorite());
        store.assign(&[session("t1", "/p/b")], HOME);
        assert_eq!(reload(&store).groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["b", "Neu"]);

        store.assign(&[session("t1", "/p/b"), session("n1", "/p/neu")], HOME);
        assert_eq!(store.group_for_session("n1").map(|g| g.name.as_str()), Some("Neu"));
        assert_eq!(store.groups.len(), 2);

        let neu_id = store.groups[1].id.clone();
        let first = store.groups[0].id.clone();
        store.move_sessions(&ids(&["n1"]), &first);
        assert!(store.group(&neu_id).is_some(), "leer geworden bleibt sie stehen");
    }

    #[test]
    fn empty_poll_neither_prunes_nor_removes_groups() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("s1", "/p/a")], HOME);
        let before = store.groups.clone();
        assert!(!store.assign(&[], HOME), "eine leere Liste ist eher ein Aussetzer");
        assert_eq!(store.groups, before);
        assert_eq!(reload(&store).groups, before);
    }

    /// Ein eigener Name hält eine leere Gruppe nicht: nur das Herz tut das.
    #[test]
    fn manually_edited_empty_group_is_pruned_without_favorite() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("s1", "/p/a"), session("t1", "/p/b")], HOME);
        let mut g = store.groups.iter().find(|g| g.cwd == "/p/a").unwrap().clone();
        g.name = "Mein Projekt".into();
        g.color = "#123456".into();
        store.update(g);
        store.remove_session("s1");
        let emptied = store.groups.iter().find(|g| g.cwd == "/p/a").unwrap();
        assert!(emptied.session_ids.is_empty());
        assert_eq!(emptied.name, "Mein Projekt");
        assert!(store.assign(&[session("t1", "/p/b")], HOME));
        assert_eq!(cwds(&store), ["/p/b"]);
        assert_eq!(cwds(&reload(&store)), ["/p/b"]);

        store.assign(&[session("s2", "/p/a")], HOME);
        let mut kept = store.groups.iter().find(|g| g.cwd == "/p/a").unwrap().clone();
        kept.name = "Mein Projekt".into();
        store.update(kept.clone());
        store.toggle_favorite(&kept.id);
        store.remove_session("s2");
        assert!(!store.assign(&[session("t1", "/p/b")], HOME));
        assert_eq!(store.groups.iter().find(|g| g.cwd == "/p/a").unwrap().name, "Mein Projekt");
    }

    #[test]
    fn favorite_survives_empty_group() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("s1", "/p/a"), session("t1", "/p/b")], HOME);
        let a = store.groups[0].id.clone();
        store.toggle_favorite(&a);
        store.remove_session("s1");
        store.remove_session("t1");
        assert!(store.assign(&[session("x1", "/p/c")], HOME));
        let mut sorted = cwds(&store);
        sorted.sort();
        assert_eq!(sorted, ["/p/a", "/p/c"]);
        assert!(store.group_for_cwd("/p/a").unwrap().session_ids.is_empty());
        assert!(reload(&store).group_for_cwd("/p/a").unwrap().is_favorite());
        store.toggle_favorite(&a);
        assert!(store.assign(&[session("x1", "/p/c")], HOME));
        assert_eq!(cwds(&store), ["/p/c"]);
    }

    fn sort_fixture() -> (HashMap<String, Session>, Vec<Group>) {
        let mut s: HashMap<String, Session> = [("b", "/x"), ("a", "/x"), ("c", "/y"), ("d", "/y")].iter().map(|(id, cwd)| (id.to_string(), session(id, cwd))).collect();
        for (id, raw) in [("a", "idle"), ("b", "busy"), ("c", "idle"), ("d", "idle")] {
            s.get_mut(id).unwrap().raw_status = Some(raw.into());
        }
        let groups = vec![Group::new("1", "zeta", "", "/x", &["b", "a"]), Group::new("2", "Alpha", "", "/y", &["d", "c"])];
        (s, groups)
    }

    #[test]
    fn sidebar_sort_alpha_and_status_keep_hand_order_on_ties() {
        let (mut s, groups) = sort_fixture();
        assert_eq!(SidebarSort::Off.apply(&groups, &s), groups);
        let alpha = SidebarSort::Alpha.apply(&groups, &s);
        assert_eq!(alpha.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["2", "1"]);
        assert_eq!(alpha.iter().map(|g| g.session_ids.clone()).collect::<Vec<_>>(), [ids(&["c", "d"]), ids(&["a", "b"])]);
        s.get_mut("c").unwrap().raw_status = Some("waiting".into());
        let status = SidebarSort::Status.apply(&groups, &s);
        assert_eq!(status.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["2", "1"]);
        assert_eq!(status.iter().map(|g| g.session_ids.clone()).collect::<Vec<_>>(), [ids(&["c", "d"]), ids(&["b", "a"])]);
    }

    #[test]
    fn status_sort_order_is_waiting_error_running_idle_newer_first() {
        let mut s: HashMap<String, Session> = HashMap::new();
        for (id, raw, started) in [("i", "idle", 1.0), ("r1", "busy", 1.0), ("r2", "busy", 5.0), ("e", "failed", 1.0), ("w", "waiting", 1.0)] {
            let mut x = session(id, "/x");
            x.raw_status = Some(raw.into());
            x.started_at = started;
            s.insert(id.into(), x);
        }
        let g = vec![Group::new("g", "G", "", "/x", &["i", "r1", "r2", "e", "w"])];
        assert_eq!(SidebarSort::Status.apply(&g, &s)[0].session_ids, ids(&["w", "e", "r2", "r1", "i"]));
    }

    #[test]
    fn sort_cycles_and_unknown_sessions_do_not_break_it() {
        assert_eq!((SidebarSort::Off.next(), SidebarSort::Alpha.next(), SidebarSort::Status.next()), (SidebarSort::Alpha, SidebarSort::Status, SidebarSort::Off));
        let (s, mut groups) = sort_fixture();
        groups[0].session_ids.push("weg".into());
        let out = SidebarSort::Status.apply(&groups, &s);
        assert_eq!(out.iter().map(|g| g.session_ids.len()).sum::<usize>(), 5);
    }

    #[test]
    fn flat_rows_follow_manual_order_then_newest_first() {
        let mut s: HashMap<String, Session> = HashMap::new();
        for (id, started) in [("a", 1.0), ("b", 2.0), ("c", 3.0), ("d", 4.0)] {
            let mut x = session(id, "/p");
            x.started_at = started;
            s.insert(id.into(), x);
        }
        let groups = vec![Group::new("g", "G", "", "/p", &["a", "b"]), Group::new("h", "H", "", "/q", &["c", "d"])];
        let order = ids(&["b", "a"]);
        let rows = SidebarFlat::rows(&groups, &s, SidebarSort::Off, &order);
        assert_eq!(rows.iter().map(|r| r.session.id.as_str()).collect::<Vec<_>>(), ["d", "c", "b", "a"], "Neue oben, dann Handreihenfolge");
        assert_eq!(rows[0].group.id, "h");
        let by_title = SidebarFlat::rows(&groups, &s, SidebarSort::Alpha, &order);
        assert_eq!(by_title.iter().map(|r| r.session.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c", "d"]);
    }

    #[test]
    fn natural_compare_like_finder() {
        use std::cmp::Ordering::*;
        assert_eq!(localized_standard_compare("a2", "a10"), Less);
        assert_eq!(localized_standard_compare("Alpha", "beta"), Less);
        assert_eq!(localized_standard_compare("Äpfel", "Zebra"), Less);
        assert_eq!(localized_standard_compare("ABC", "abc"), Equal);
        assert_eq!(localized_standard_compare("x007", "x7"), Equal);
        assert_eq!(localized_standard_compare("ab", "abc"), Less);
    }

    #[test]
    fn corrupt_file_is_quarantined_not_overwritten() {
        let d = TempDir::new();
        std::fs::write(url(&d), "{kaputt").unwrap();
        let mut store = GroupStore::open(url(&d));
        assert!(store.groups.is_empty());
        assert!(store.last_error().is_some());
        let quarantined = |d: &TempDir| std::fs::read_dir(d.path()).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with("groups.corrupt-"));
        assert!(quarantined(&d));
        store.add(Group::new("g", "n", "#fff", "/p", &[]));
        assert!(url(&d).exists());
        assert!(quarantined(&d));
    }

    #[test]
    fn single_broken_entry_does_not_lose_the_rest() {
        let d = TempDir::new();
        std::fs::write(url(&d), r##"[{"id":"g1","name":"gut","color":"#fff","cwd":"/p","sessionIds":[]},{"id":123}]"##).unwrap();
        let store = GroupStore::open(url(&d));
        assert_eq!(store.groups.iter().map(|g| g.id.as_str()).collect::<Vec<_>>(), ["g1"]);
        assert!(store.last_error().is_some());
    }

    #[test]
    fn legacy_bare_array_format_loads() {
        let d = TempDir::new();
        std::fs::write(url(&d), r##"[{"id":"g1","name":"alt","color":"#fff","cwd":"/p","sessionIds":["s1"]}]"##).unwrap();
        let store = GroupStore::open(url(&d));
        assert_eq!(store.groups[0].id, "g1");
        assert!(!store.groups[0].is_favorite());
        assert_eq!(store.groups[0].host, None);
    }

    #[test]
    fn favorite_defaults_to_false_and_toggles_cleanly() {
        let d = TempDir::new();
        std::fs::write(url(&d), r##"[{"id":"g1","name":"n","color":"#fff","cwd":"/p","sessionIds":[]}]"##).unwrap();
        let mut store = GroupStore::open(url(&d));
        assert!(!store.groups[0].is_favorite());
        store.toggle_favorite("g1");
        assert!(store.groups[0].is_favorite());
        store.toggle_favorite("g1");
        assert!(!store.groups[0].is_favorite());
        assert!(std::fs::read_to_string(url(&d)).unwrap().contains(r#""favorite": false"#), "der Schlüssel steht immer in der Datei");
    }

    #[test]
    fn duplicate_session_id_across_groups_is_deduped() {
        let d = TempDir::new();
        std::fs::write(
            url(&d),
            r##"[{"id":"g1","name":"eins","color":"#fff","cwd":"/p","sessionIds":["s1","s2"]},
                 {"id":"g2","name":"zwei","color":"#000","cwd":"/q","sessionIds":["s2","s3"]}]"##,
        )
        .unwrap();
        let store = GroupStore::open(url(&d));
        assert_eq!(store.group("g1").unwrap().session_ids, ["s1", "s2"]);
        assert_eq!(store.group("g2").unwrap().session_ids, ["s3"]);
        assert_eq!(store.group_for_session("s2").unwrap().id, "g1");
        assert_eq!(reload(&store).group("g2").unwrap().session_ids, ["s3"], "bereinigter Stand ist gespeichert");
    }

    #[test]
    fn newer_schema_version_is_never_overwritten() {
        let d = TempDir::new();
        let text = r##"{"version":9,"items":[{"id":"g1","name":"n","color":"#fff","cwd":"/p","sessionIds":[]}]}"##;
        std::fs::write(url(&d), text).unwrap();
        let mut store = GroupStore::open(url(&d));
        assert_eq!(store.groups.len(), 1);
        store.add(Group::new("g2", "x", "#000", "/q", &[]));
        assert_eq!(std::fs::read_to_string(url(&d)).unwrap(), text);
    }

    #[test]
    fn swift_shaped_file_roundtrips_with_host() {
        let d = TempDir::new();
        let swift = r##"[
  {
    "color" : "#fab387",
    "cwd" : "/Users/dev/p/kadrell",
    "favorite" : true,
    "id" : "2bcebb8b-95ac-46d2-8e2e-464dd37f2b86",
    "name" : "Kadrell",
    "sessionIds" : [
      "f45edac5-957e-4453-9dea-ff5728623427"
    ]
  },
  {
    "color" : "#cba6f7",
    "cwd" : "/Users/dev",
    "favorite" : false,
    "host" : "examplehost",
    "id" : "r1",
    "name" : "examplehost",
    "sessionIds" : [
      "ssh-1"
    ]
  }
]"##;
        std::fs::write(url(&d), swift).unwrap();
        let mut store = GroupStore::open(url(&d));
        assert_eq!(store.group_for_host("examplehost").unwrap().id, "r1");
        assert!(store.group_for_cwd("/Users/dev").is_none(), "Remote-Gruppen zählen nicht als lokal");
        assert!(store.groups[0].favorite);
        store.save();
        let written: serde_json::Value = serde_json::from_slice(&std::fs::read(url(&d)).unwrap()).unwrap();
        assert_eq!(written, serde_json::from_str::<serde_json::Value>(swift).unwrap(), "Schreiben ergibt dasselbe JSON");
    }

    #[test]
    fn remote_sessions_group_by_host_not_cwd() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        let mut r1 = session("ssh-1", HOME);
        r1.host = Some("examplehost".into());
        let mut r2 = session("ssh-2", HOME);
        r2.host = Some("examplehost".into());
        r2.tmux_session = Some("test".into());
        let local = session("shell-1", HOME);
        store.assign(&[r1.clone(), local, r2.clone()], HOME);
        assert_eq!(store.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["examplehost", "test"]);
        assert_eq!(store.groups[0].session_ids, ["ssh-1", "ssh-2"]);
        assert_eq!(store.groups[0].host.as_deref(), Some("examplehost"));
        assert_eq!(store.groups[1].host, None);
        assert_eq!(r1.title(), "examplehost");
        assert_eq!(r2.title(), "test");
        assert_eq!(reload(&store).groups, store.groups);
    }

    #[test]
    fn move_appends_keeps_cwd_and_survives_assign() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        let all = [session("a1", "/p/a"), session("a2", "/p/a"), session("b1", "/p/b")];
        store.assign(&all, HOME);
        let (a, b) = (store.groups[0].id.clone(), store.groups[1].id.clone());
        assert!(store.move_sessions(&ids(&["a1"]), &b));
        assert_eq!(store.group(&a).unwrap().session_ids, ["a2"]);
        assert_eq!(store.group(&b).unwrap().session_ids, ["b1", "a1"]);
        assert!(!store.assign(&all, HOME), "der Abgleich ordnet nach cwd nur Sessions ohne Gruppe zu");
        assert_eq!(reload(&store).groups, store.groups);
        assert!(!store.move_sessions(&ids(&["a1"]), &b), "schon dort: nichts zu tun");
        assert!(!store.move_sessions(&ids(&["a1"]), "gibt-es-nicht"));
    }

    #[test]
    fn moving_last_session_removes_empty_group_unless_favorite() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("a1", "/p/a"), session("b1", "/p/b"), session("c1", "/p/c")], HOME);
        let (a, b, c) = (store.groups[0].id.clone(), store.groups[1].id.clone(), store.groups[2].id.clone());
        store.toggle_favorite(&c);
        store.move_sessions(&ids(&["a1"]), &b);
        assert!(store.group(&a).is_none());
        store.move_sessions(&ids(&["c1", "a1", "c1"]), &b);
        assert_eq!(store.group(&b).unwrap().session_ids, ["b1", "a1", "c1"], "doppelte Ids zählen einmal");
        assert!(store.group(&c).unwrap().session_ids.is_empty(), "Favorit bleibt leer stehen");
    }

    #[test]
    fn colors_come_from_palette_in_order_then_wrap() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        assert_eq!(store.next_color(), PALETTE[0]);
        for (i, expected) in PALETTE.iter().enumerate() {
            let mut g = store.make_group(&format!("/p/{i}"), None);
            g.session_ids = vec![format!("s{i}")];
            assert_eq!(g.color, *expected);
            store.groups.push(g);
        }
        assert_eq!(store.next_color(), PALETTE[PALETTE.len() % PALETTE.len()], "alle vergeben: reihum");
        store.groups.remove(0);
        assert_eq!(store.next_color(), PALETTE[0], "freigewordene Farbe zuerst");
        assert_eq!(PALETTES.len(), 7);
        assert_eq!(PALETTES[0].0, "Catppuccin");
    }

    #[test]
    fn rename_attach_and_default_name() {
        let d = TempDir::new();
        let mut store = GroupStore::open(url(&d));
        store.assign(&[session("a", "/p/a"), session("b", "/p/b")], HOME);
        let (ga, gb) = (store.groups[0].id.clone(), store.groups[1].id.clone());
        store.rename(&ga, "Neu");
        store.set_color(&ga, "#000000");
        assert_eq!((store.group(&ga).unwrap().name.as_str(), store.group(&ga).unwrap().color.as_str()), ("Neu", "#000000"));
        store.attach("a", &gb);
        assert_eq!(store.group(&gb).unwrap().session_ids, ["b", "a"]);
        assert_eq!(store.ordered_keys(), ["b", "a"]);
        assert_eq!(GroupStore::default_name("/p/kadrell/", None), "kadrell");
        assert_eq!(GroupStore::default_name("/p/x", Some("srv")), "srv");
        store.remove(&ga);
        assert!(store.group(&ga).is_none());
    }
}
