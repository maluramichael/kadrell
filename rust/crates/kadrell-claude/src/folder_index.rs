//! Ordner für ⌘N: benutzte Ordner mit Häufigkeit und letzter Nutzung, dazu Git-Repos unter dem Startordner und neben
//! bekannten Projekten (Swift: `Services/FolderIndex.swift`). Der Scan läuft im Hintergrund und liegt zwischengespeichert
//! (`folders.json`, `folders-uses.json`, gleiche Formate wie die Mac-App), die Suche geht nur gegen den Index.
//! Nicht hier: Zwischenablage und Finder-Ordner (`clipboardFolder`, `finderFolder`), die brauchen das Fenstersystem.

use crate::{paths, text};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Use {
    pub count: u32,
    /// Sekunden seit 1970.
    pub last: f64,
}

/// Ungenutzte Einträge (Ordner gelöscht, Repo umbenannt) verfallen nach 90 Tagen, siehe `prune_uses`.
pub const USES_MAX_AGE_SECS: f64 = 90.0 * 86_400.0;
pub const USES_LIMIT: usize = 500;

#[derive(Clone)]
pub struct FolderIndex {
    pub repos: Vec<String>,
    pub uses: HashMap<String, Use>,
    repos_file: String,
    uses_file: String,
    home: String,
}

impl FolderIndex {
    /// `directory`: Profilordner, `home`: persönlicher Ordner (für `~`).
    pub fn load(directory: &str, home: &str) -> FolderIndex {
        let repos_file = format!("{directory}/folders.json");
        let uses_file = format!("{directory}/folders-uses.json");
        FolderIndex {
            repos: std::fs::read(&repos_file).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default(),
            uses: std::fs::read(&uses_file).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default(),
            repos_file,
            uses_file,
            home: home.to_string(),
        }
    }

    pub fn record_use(&mut self, path: &str, now: f64) {
        let p = normalize(path, false, &self.home);
        let u = self.uses.entry(p).or_insert(Use { count: 0, last: 0.0 });
        u.count += 1;
        u.last = now;
        self.prune_uses(now);
        self.save_uses();
    }

    /// Verworfene und umbenannte Ordner sammeln sich sonst für immer: älter als 90 Tage oder der Pfad existiert nicht mehr
    /// fliegt raus, danach bleiben höchstens `USES_LIMIT` Einträge, die mit der besten Frecency zuerst.
    fn prune_uses(&mut self, now: f64) {
        self.uses.retain(|path, u| now - u.last < USES_MAX_AGE_SECS && Path::new(path).exists());
        if self.uses.len() <= USES_LIMIT {
            return;
        }
        let mut ranked: Vec<(String, Use)> = self.uses.drain().collect();
        ranked.sort_by(|a, b| frecency_of(&b.1, now).total_cmp(&frecency_of(&a.1, now)));
        self.uses = ranked.into_iter().take(USES_LIMIT).collect();
    }

    /// Gelöschte oder umbenannte Ordner sofort aus dem gespeicherten Stand nehmen, ohne den Scan abzuwarten: ⌘N ruft das
    /// beim Öffnen auf, damit nie ein Ordner erscheint, den es nicht mehr gibt.
    pub fn drop_missing(&mut self, now: f64) {
        let alive: Vec<String> = self.repos.iter().filter(|r| is_directory(r)).cloned().collect();
        if alive.len() != self.repos.len() {
            self.repos = alive;
            self.save_repos();
        }
        let before = self.uses.len();
        self.prune_uses(now);
        if self.uses.len() != before {
            self.save_uses();
        }
    }

    /// Scan der Wurzeln (auf einem Hintergrund-Thread aufrufen), Ergebnis gespeichert und übernommen.
    pub fn refresh(&mut self, roots: &[String]) {
        let roots: HashSet<String> = roots.iter().map(|r| normalize(r, false, &self.home)).collect();
        let mut roots: Vec<String> = roots.into_iter().collect();
        roots.sort();
        self.repos = scan_repos(&roots, 4, 5000);
        self.save_repos();
    }

    /// „Oft und kürzlich“ wie bei zoxide: Nutzungen zählen, je älter die letzte, desto weniger.
    pub fn frecency(&self, path: &str, now: f64) -> f64 {
        self.uses.get(path).map(|u| frecency_of(u, now)).unwrap_or(0.0)
    }

    fn save_uses(&self) {
        write_json(&self.uses_file, &self.uses);
    }

    fn save_repos(&self) {
        write_json(&self.repos_file, &self.repos);
    }
}

fn write_json<T: Serialize>(file: &str, value: &T) {
    let Ok(data) = serde_json::to_vec(value) else { return };
    let _ = std::fs::create_dir_all(paths::parent(file));
    let tmp = format!("{file}.tmp");
    if std::fs::write(&tmp, data).is_ok() {
        let _ = std::fs::rename(&tmp, file);
    }
}

fn frecency_of(u: &Use, now: f64) -> f64 {
    let age = now - u.last;
    let weight = if age < 3600.0 {
        4.0
    } else if age < 86_400.0 {
        2.0
    } else if age < 604_800.0 {
        1.0
    } else {
        0.5
    };
    f64::from(u.count) * weight
}

// Scan

/// Ordner, in die der Scan nicht hineinläuft. Desktop/Documents/Downloads mit rein: sonst löst der erste ⌘N beim Scan
/// von ~ eine Kaskade an Rückfragen des Systems aus. Als ausdrücklicher Startordner bleiben sie trotzdem durchsuchbar, der
/// Skip greift nur beim Hineinlaufen aus einer anderen Wurzel, nicht auf die Wurzel selbst.
const SKIP: [&str; 12] = ["node_modules", "vendor", "build", "dist", "Library", "Applications", "Pictures", "Music", "Movies", "Desktop", "Documents", "Downloads"];

/// Git-Repos bis Tiefe `max_depth` unter den Wurzeln. In ein Repo wird nicht weiter hineingeschaut. Fester Tiefen- und
/// Mengen-Deckel statt echter Ausschlussliste, reicht für Projektordner.
pub fn scan_repos(roots: &[String], max_depth: usize, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut queue: VecDeque<(String, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    let mut seen = HashSet::new();
    while let Some((dir, depth)) = queue.pop_front() {
        if out.len() >= limit {
            break;
        }
        if !seen.insert(dir.clone()) {
            continue;
        }
        if Path::new(&paths::join(&dir, ".git")).exists() {
            out.push(dir);
            continue;
        }
        if depth >= max_depth {
            continue;
        }
        for n in subdirectories(&dir) {
            if !SKIP.contains(&n.as_str()) {
                queue.push_back((paths::join(&dir, &n), depth + 1));
            }
        }
    }
    out.sort();
    out
}

// Suche

/// Wortfetzen wie bei zoxide: jeder muss im Pfad vorkommen, der letzte im letzten Ordnernamen. Kleiner ist besser:
/// 0 Präfix, 1 Teilstring, 2 Buchstabenfolge im Namen, 3 nur im Pfad. nil = kein Treffer.
pub fn rank(query: &str, path: &str) -> Option<u8> {
    let lowered = query.to_lowercase();
    let words: Vec<&str> = lowered.split(' ').filter(|w| !w.is_empty()).collect();
    let Some((last, init)) = words.split_last() else { return Some(0) };
    let lower = path.to_lowercase();
    let name = paths::last_component(&lower);
    if init.iter().any(|w| !lower.contains(w)) {
        return None;
    }
    if name.starts_with(last) {
        Some(0)
    } else if name.contains(last) {
        Some(1)
    } else if text::fuzzy(last, name) {
        Some(2)
    } else if words.len() == 1 && text::fuzzy(last, &lower) {
        Some(3)
    } else {
        None
    }
}

/// `~/d/p/kad` gibt `~/development/projects/kadrell`: jedes Stück passt als Präfix (sonst Buchstabenfolge) auf einen
/// Ordner. Ein exakter Name gewinnt, damit ausgeschriebene Pfade nicht aufblähen.
pub fn expand_abbreviated(typed: &str, base: &str, home: &str, limit: usize) -> Vec<String> {
    let path = normalize(typed, true, home);
    let (root, rest) = paths::split_root(&path);
    let mut parts: Vec<&str> = rest.split('/').collect();
    let mut starts: Vec<String> = vec![if root.is_empty() { base.to_string() } else { root.to_string() }];
    let trailing = parts.last() == Some(&"");
    if trailing {
        parts.pop();
    }
    for (i, part) in parts.iter().enumerate() {
        let is_last = i == parts.len() - 1 && !trailing;
        starts = next_level(&starts, part, is_last, limit);
        if starts.is_empty() {
            return Vec::new();
        }
    }
    if !trailing {
        return starts;
    }
    starts.iter().flat_map(|d| sorted_subdirectories(d).into_iter().map(|n| paths::join(d, &n))).take(limit).collect()
}

fn sorted_subdirectories(dir: &str) -> Vec<String> {
    let mut names = subdirectories(dir);
    names.sort_by_key(|n| n.to_lowercase());
    names
}

/// Über alle Kandidaten gemeinsam: exakt vor Präfix, Buchstabenfolge nur, wenn nirgends ein Präfix passt.
fn next_level(starts: &[String], part: &str, is_last: bool, limit: usize) -> Vec<String> {
    let q = part.to_lowercase();
    let (mut exact, mut prefix, mut fuzzy) = (Vec::new(), Vec::new(), Vec::new());
    for dir in starts {
        for n in sorted_subdirectories(dir) {
            let l = n.to_lowercase();
            if n == part {
                exact.push(paths::join(dir, &n));
            } else if l.starts_with(&q) {
                prefix.push(paths::join(dir, &n));
            } else if is_last && text::fuzzy(&q, &l) {
                fuzzy.push(paths::join(dir, &n));
            }
        }
    }
    let chosen = if !exact.is_empty() {
        exact
    } else if !prefix.is_empty() {
        prefix
    } else {
        fuzzy
    };
    chosen.into_iter().take(limit).collect()
}

// Neuer Ordner

/// Ordner, den ⌘N für die Eingabe neu anlegen würde, oder nil. Ein bloßer Name landet im Startordner, ein Pfad mit „/“ oder
/// „~“ dort, wo er hinzeigt; der übergeordnete Teil darf wie bei der Suche abgekürzt sein (`~/d/p/neu` gibt
/// `~/development/projects/neu`). Nil, wenn der Ordner schon existiert, außerhalb von `home` läge, „.“ oder „..“ enthält oder
/// ein bloßer Name Leerzeichen hat (dort trennen sie Suchwörter).
pub fn creation_target(typed: &str, start_folder: &str, home: &str) -> Option<String> {
    let q = normalize(typed, false, home);
    let name = q.split(['/', '\\']).rfind(|p| !p.is_empty())?.to_string();
    if q.split(['/', '\\']).any(|p| p == "." || p == "..") {
        return None;
    }
    let is_path = paths::is_abs(&q) || q.contains('/');
    if !is_path && name.contains(' ') {
        return None;
    }
    let parent = if is_path { resolved_parent(&q, start_folder, home) } else { normalize(start_folder, false, home) };
    let path = paths::join(&parent, &name);
    let root = normalize(home, false, home);
    (paths::fwd(&path).starts_with(&format!("{}/", paths::fwd(&root))) && !Path::new(&path).exists()).then_some(path)
}

/// Übergeordneter Ordner eines getippten Pfads: wörtlich, wenn es ihn gibt (relativ zum Startordner), sonst die erste
/// Auflösung der Abkürzung, sonst wörtlich (die fehlenden Zwischenordner entstehen beim Anlegen mit).
fn resolved_parent(q: &str, start_folder: &str, home: &str) -> String {
    let typed = paths::parent(q);
    let literal = if paths::is_abs(q) {
        if typed.is_empty() { "/".to_string() } else { typed.clone() }
    } else {
        paths::join(&normalize(start_folder, false, home), &typed)
    };
    if is_directory(&literal) {
        return literal;
    }
    let abbreviated = if typed.is_empty() { "/" } else { typed.as_str() };
    expand_abbreviated(abbreviated, &normalize(home, false, home), home, 20).into_iter().next().unwrap_or(literal)
}

pub fn subdirectories(dir: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect()
}

/// `~` auflösen, Trenner am Ende weg (außer bei der Wurzel selbst).
pub fn normalize(path: &str, keep_trailing_slash: bool, home: &str) -> String {
    let mut p = paths::expand_tilde(path.trim(), home);
    if !keep_trailing_slash {
        while p.len() > 1 && (p.ends_with('/') || p.ends_with('\\')) && !(p.len() == 3 && p.as_bytes()[1] == b':') {
            p.pop();
        }
    }
    p
}

pub fn is_directory(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// Ordner selbst, bei einer Datei ihr Ordner, sonst nil.
pub fn folder_for(path: &str, home: &str) -> Option<String> {
    if !paths::is_abs(path) || !Path::new(path).exists() {
        return None;
    }
    Some(if is_directory(path) { normalize(path, false, home) } else { paths::parent(path) })
}

// Kontext

/// Ordner aus dem Text der Zwischenablage: ein Pfad ohne Zeilenumbruch und unter 1024 Zeichen, der einen Ordner (oder eine
/// Datei darin) bezeichnet (Swift: `clipboardFolder`, Textzweig). Dateien aus dem Dateimanager liefert das Fenstersystem
/// als Pfadliste, dafür `folder_for`.
pub fn clipboard_folder(text: &str, home: &str) -> Option<String> {
    if text.contains('\n') || text.chars().count() >= 1024 {
        return None;
    }
    folder_for(&normalize(&text.replace('\\', "/"), false, home), home)
}

/// Ordner des vordersten Dateimanager-Fensters (Swift: `finderFolder`), mit `/`. macOS: Finder per `osascript` (fragt beim
/// ersten Mal nach der Erlaubnis, deshalb ohne Zeitlimit), Windows: das oberste Explorer-Fenster der Fensterfolge per
/// PowerShell. Blockiert, auf einem Hintergrund-Thread aufrufen.
pub fn finder_folder(home: &str) -> Option<String> {
    let out = file_manager_output()?;
    folder_for(&normalize(&out.trim().replace('\\', "/"), false, home), home)
}

#[cfg(target_os = "macos")]
fn file_manager_output() -> Option<String> {
    let script = "tell application \"Finder\" to if (count of Finder windows) > 0 then POSIX path of (target of front Finder window as alias)";
    let out = std::process::Command::new("/usr/bin/osascript").args(["-e", script]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Explorer-Fenster (Shell.Application) in der Reihenfolge der Fenster von oben nach unten (user32 `GetTopWindow`,
/// `GetWindow(GW_HWNDNEXT)`): das erste mit einem Dateisystempfad gewinnt.
#[cfg(windows)]
const EXPLORER_SCRIPT: &str = r#"
$sig = '[DllImport("user32.dll")] public static extern IntPtr GetTopWindow(IntPtr h); [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint c);'
Add-Type -MemberDefinition $sig -Name Z -Namespace Kadrell
$map = @{}
foreach ($w in (New-Object -ComObject Shell.Application).Windows()) { try { $map[[int64]$w.HWND] = $w.Document.Folder.Self.Path } catch {} }
$h = [Kadrell.Z]::GetTopWindow([IntPtr]::Zero)
while ($h -ne [IntPtr]::Zero) { if ($map.ContainsKey([int64]$h)) { $map[[int64]$h]; break }; $h = [Kadrell.Z]::GetWindow($h, 2) }
"#;

#[cfg(windows)]
fn file_manager_output() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", EXPLORER_SCRIPT])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn file_manager_output() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_folder_takes_one_line_paths_to_existing_folders() {
        let t = TempDir::new("clip");
        let file = t.write("a/b.txt", "x");
        assert_eq!(clipboard_folder(&format!("  {}  ", t.sub("a")), &t.path()), Some(t.sub("a")));
        assert_eq!(clipboard_folder(&file, &t.path()), Some(t.sub("a")), "bei einer Datei ihr Ordner");
        assert_eq!(clipboard_folder(&format!("{}\n{}", t.sub("a"), t.sub("a")), &t.path()), None, "mehrere Zeilen");
        assert_eq!(clipboard_folder(&t.sub("fehlt"), &t.path()), None);
        assert_eq!(clipboard_folder("kein pfad", &t.path()), None);
        assert_eq!(clipboard_folder(&"a".repeat(1024), &t.path()), None);
    }

    use crate::paths::testdir::TempDir;

    const NOW: f64 = 1_800_000_000.0;

    #[test]
    fn rank_prefers_name_prefix_then_contains_then_fuzzy() {
        assert_eq!(rank("kad", "/x/kadrell"), Some(0));
        assert_eq!(rank("agent", "/x/claude-agent-overview"), Some(1));
        assert_eq!(rank("cao", "/x/claude-agent-overview"), Some(2));
        assert_eq!(rank("prj", "/x/projects/kadrell"), Some(3));
        assert_eq!(rank("zzz", "/x/kadrell"), None);
        assert_eq!(rank("", "/x/kadrell"), Some(0));
    }

    #[test]
    fn rank_words_must_all_match_last_in_name() {
        assert_eq!(rank("projects kad", "/dev/projects/kadrell"), Some(0));
        assert_eq!(rank("acme kad", "/dev/projects/kadrell"), None);
    }

    #[test]
    fn expand_abbreviated_and_scan() {
        let t = TempDir::new("folders");
        for d in ["development/projects/kadrell/.git", "development/projects/kalender/.git", "development/other", "downloads"] {
            t.mkdir(d);
        }
        let root = t.path();
        let at = |p: &str| format!("{root}/{p}");
        let abbr = |typed: &str, base: &str| expand_abbreviated(typed, base, &root, 20);
        assert_eq!(abbr("dev/pro/kad", &root), [at("development/projects/kadrell")]);
        assert_eq!(abbr("development/projects/k", &root), [at("development/projects/kadrell"), at("development/projects/kalender")]);
        assert_eq!(abbr("dev/projects/", &root), [at("development/projects/kadrell"), at("development/projects/kalender")]);
        assert_eq!(abbr(&format!("{root}/d/pr/kdrl"), "/"), [at("development/projects/kadrell")]);
        assert_eq!(scan_repos(std::slice::from_ref(&root), 4, 5000), [at("development/projects/kadrell"), at("development/projects/kalender")]);
    }

    /// `folders-uses.json` statt eines Einstellungs-Blobs, und wächst nicht unbegrenzt weiter.
    #[test]
    fn record_use_persists_and_prunes_missing_and_old_paths() {
        let t = TempDir::new("uses");
        let existing = t.mkdir("existing");
        let gone = Use { count: 9, last: NOW };
        let old = Use { count: 5, last: NOW - 100.0 * 86_400.0 };
        let seed: HashMap<String, Use> = [(t.sub("gone"), gone), (existing.clone(), old)].into();
        std::fs::write(t.sub("folders-uses.json"), serde_json::to_vec(&seed).unwrap()).unwrap();

        let mut index = FolderIndex::load(&t.path(), &t.path());
        assert_eq!(index.uses.len(), 2);
        index.record_use(&existing, NOW);
        // Gelöschter Ordner fliegt raus, der existierende bleibt (record_use aktualisiert `last`).
        assert!(!index.uses.contains_key(&t.sub("gone")));
        assert_eq!(index.uses[&existing].count, 6);

        let reloaded = FolderIndex::load(&t.path(), &t.path());
        assert_eq!(reloaded.uses.keys().cloned().collect::<Vec<_>>(), [existing]);
    }

    /// ⌘N räumt den gespeicherten Stand auf: gelöschte Repos und Nutzungen verschwinden sofort, auch in der Datei.
    #[test]
    fn drop_missing_removes_deleted_repos_and_uses() {
        let t = TempDir::new("dropmissing");
        let alive = t.mkdir("alive");
        let gone = t.sub("gone");
        std::fs::write(t.sub("folders.json"), serde_json::to_vec(&[&alive, &gone]).unwrap()).unwrap();
        let use_ = Use { count: 1, last: NOW };
        let seed: HashMap<String, Use> = [(alive.clone(), use_), (gone.clone(), use_)].into();
        std::fs::write(t.sub("folders-uses.json"), serde_json::to_vec(&seed).unwrap()).unwrap();

        let mut index = FolderIndex::load(&t.path(), &t.path());
        assert_eq!(index.repos, [alive.clone(), gone]);
        index.drop_missing(NOW);
        let only = std::slice::from_ref(&alive);
        assert_eq!(index.repos, only);
        assert_eq!(index.uses.keys().cloned().collect::<Vec<_>>(), only);

        let reloaded = FolderIndex::load(&t.path(), &t.path());
        assert_eq!(reloaded.repos, only);
        assert_eq!(reloaded.uses.keys().cloned().collect::<Vec<_>>(), only);
    }

    #[test]
    fn frecency_weights_by_age_and_count() {
        let mut index = FolderIndex { repos: vec![], uses: HashMap::new(), repos_file: String::new(), uses_file: String::new(), home: String::new() };
        index.uses.insert("/a".into(), Use { count: 3, last: NOW - 60.0 });
        index.uses.insert("/b".into(), Use { count: 3, last: NOW - 2.0 * 86_400.0 });
        index.uses.insert("/c".into(), Use { count: 3, last: NOW - 30.0 * 86_400.0 });
        assert_eq!((index.frecency("/a", NOW), index.frecency("/b", NOW), index.frecency("/c", NOW), index.frecency("/x", NOW)), (12.0, 3.0, 1.5, 0.0));
    }

    #[test]
    fn uses_are_capped_by_best_frecency() {
        let t = TempDir::new("cap");
        let mut index = FolderIndex::load(&t.path(), &t.path());
        for i in 0..(USES_LIMIT + 5) {
            let dir = t.mkdir(&format!("d{i:03}"));
            index.uses.insert(dir, Use { count: if i < 5 { 1 } else { 10 }, last: NOW });
        }
        index.prune_uses(NOW);
        assert_eq!(index.uses.len(), USES_LIMIT);
        assert!(!index.uses.contains_key(&t.sub("d000")), "die am wenigsten genutzten fliegen raus");
    }

    #[test]
    fn creation_target_rules() {
        let t = TempDir::new("create");
        let home = t.path();
        t.mkdir("development/projects");
        t.mkdir("vorhanden");
        let start = t.sub("development/projects");
        assert_eq!(creation_target("neu", &start, &home), Some(format!("{start}/neu")));
        assert_eq!(creation_target("zwei worte", &start, &home), None, "Leerzeichen trennen Suchwörter");
        assert_eq!(creation_target("vorhanden", &home, &home), None, "gibt es schon");
        assert_eq!(creation_target("../x", &start, &home), None);
        assert_eq!(creation_target("~/d/p/neu", &start, &home), Some(format!("{home}/development/projects/neu")));
        assert_eq!(creation_target(&format!("{}/nicht-im-home", paths::parent(&home)), &start, &home), None, "außerhalb von home");
    }

    #[test]
    fn normalize_and_folder_for() {
        assert_eq!(normalize("  ~/x/// ", false, "/h"), "/h/x");
        assert_eq!(normalize("/", false, "/h"), "/");
        assert_eq!(normalize("~/x/", true, "/h"), "/h/x/");
        let t = TempDir::new("folderfor");
        let file = t.write("a/b.txt", "x");
        assert_eq!(folder_for(&file, &t.path()), Some(t.sub("a")));
        assert_eq!(folder_for(&t.sub("a"), &t.path()), Some(t.sub("a")));
        assert_eq!(folder_for(&t.sub("fehlt"), &t.path()), None);
        assert_eq!(folder_for("relativ", &t.path()), None);
    }
}
