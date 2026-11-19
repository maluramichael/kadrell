//! Pfadhilfen für die Oberfläche: Kürzel (`~/d/p/kad`) mit austauschbarem Dateisystem, `~` und Home-Kürzung. Alles
//! Zeichenketten mit `/`, damit es unter Windows und macOS gleich testbar ist. Die Ordnersuche für „Neue Session“ (Rang,
//! Frecency, Repo-Scan) liegt in `kadrell_claude::folder_index`.

/// Buchstaben von `q` kommen in `text` in dieser Reihenfolge vor (Groß/Klein egal).
pub fn fuzzy(q: &str, text: &str) -> bool {
    let mut hay = text.chars().flat_map(char::to_lowercase);
    q.chars().flat_map(char::to_lowercase).all(|c| hay.any(|h| h == c))
}

/// Pfad statt Wortsuche: beginnt mit `/`, `~`, Laufwerk oder enthält einen `/`.
pub fn is_path_query(q: &str) -> bool {
    let q = q.trim();
    q.starts_with('~') || q.starts_with('/') || q.contains('/') || q.contains('\\') || has_drive(q)
}

fn has_drive(p: &str) -> bool {
    let mut c = p.chars();
    matches!((c.next(), c.next()), (Some(a), Some(':')) if a.is_ascii_alphabetic())
}

/// `~` zu Home, Backslashes zu `/`, ohne Schrägstrich am Ende.
pub fn normalize(typed: &str, home: &str) -> String {
    let t = typed.trim().replace('\\', "/");
    let full = match t.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("{home}{rest}"),
        _ => t,
    };
    if full.len() > 1 { full.trim_end_matches('/').to_string() } else { full }
}

/// Home vorn durch `~` ersetzen (ohne Rücksicht auf Groß/Klein unter Windows).
pub fn short_path(path: &str, home: &str) -> String {
    let (p, h) = (path.to_lowercase(), home.to_lowercase());
    match p.strip_prefix(&h) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{}", &path[home.len()..]),
        _ => path.to_string(),
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

/// Startpunkt und restliche Stücke eines getippten Pfads.
fn split_start(typed: &str, home: &str) -> (String, String) {
    let t = typed.trim().replace('\\', "/");
    let (start, rest) = if t == "~" || t.starts_with("~/") {
        (home.to_string(), t[1..].trim_start_matches('/').to_string())
    } else if let Some(r) = t.strip_prefix('/') {
        ("/".to_string(), r.to_string())
    } else if has_drive(&t) {
        (format!("{}/", &t[..2]), t[2..].trim_start_matches('/').to_string())
    } else {
        (home.to_string(), t)
    };
    (start, rest)
}

/// Ein Pfadstück gegen alle Startordner: exakt vor Präfix, Buchstabenfolge nur, wenn nirgends ein Präfix passt.
fn match_part(starts: &[String], part: &str, is_last: bool, dirs: &dyn Fn(&str) -> Vec<String>) -> Vec<String> {
    let q = part.to_lowercase();
    let (mut exact, mut prefix, mut loose) = (vec![], vec![], vec![]);
    for dir in starts {
        let mut names = dirs(dir);
        names.sort_by_key(|n| n.to_lowercase());
        for n in names {
            let l = n.to_lowercase();
            if n == part {
                exact.push(join(dir, &n));
            } else if l.starts_with(&q) {
                prefix.push(join(dir, &n));
            } else if is_last && fuzzy(&q, &l) {
                loose.push(join(dir, &n));
            }
        }
    }
    let pick = if !exact.is_empty() { exact } else if !prefix.is_empty() { prefix } else { loose };
    pick.into_iter().take(20).collect()
}

/// `~/d/p/kad` zu `~/development/projects/kadrell`: jedes Stück passt als Präfix (sonst Buchstabenfolge) auf einen
/// Ordner, ein exakter Name gewinnt. Endet die Eingabe auf `/`, folgen deren Unterordner.
pub fn expand_abbreviated(typed: &str, home: &str, limit: usize, dirs: &dyn Fn(&str) -> Vec<String>) -> Vec<String> {
    let (start, rest) = split_start(typed, home);
    let trailing = rest.ends_with('/') || (rest.is_empty() && typed.trim().ends_with('/'));
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    let mut starts = vec![start];
    let count = parts.len();
    for (i, part) in parts.iter().enumerate() {
        starts = match_part(&starts, part, i == count - 1 && !trailing, dirs);
        if starts.is_empty() {
            return vec![];
        }
    }
    if !trailing {
        return starts.into_iter().take(limit).collect();
    }
    let mut out = vec![];
    for d in &starts {
        let mut names = dirs(d);
        names.sort_by_key(|n| n.to_lowercase());
        out.extend(names.iter().map(|n| join(d, n)));
    }
    out.into_iter().take(limit).collect()
}

/// Unterordner (Namen) eines Ordners ohne versteckte, für `expand_abbreviated`. Blockiert.
pub fn list_dirs(dir: &str) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else { return vec![] };
    read.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| !n.starts_with('.'))
        .collect()
}

pub fn home_dir() -> String {
    let h = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    h.replace('\\', "/").trim_end_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(dir: &str) -> Vec<String> {
        let t: &[(&str, &[&str])] = &[
            ("/h", &["development", "Documents", "downloads"]),
            ("/h/development", &["projects", "start-my-day"]),
            ("/h/development/projects", &["kadrell", "homelab", "kadmon"]),
            ("/h/development/projects/kadrell", &["app", "windows"]),
        ];
        t.iter().find(|(d, _)| *d == dir).map(|(_, n)| n.iter().map(|s| s.to_string()).collect()).unwrap_or_default()
    }

    fn expand(q: &str) -> Vec<String> {
        expand_abbreviated(q, "/h", 20, &tree)
    }

    #[test]
    fn fuzzy_is_subsequence() {
        assert!(fuzzy("kdl", "Kadrell"));
        assert!(!fuzzy("kdx", "Kadrell"));
        assert!(fuzzy("", "x"));
    }

    #[test]
    fn abbreviations_expand_per_segment() {
        assert_eq!(expand("~/d/p/kad"), ["/h/development/projects/kadmon", "/h/development/projects/kadrell"]);
        assert_eq!(expand("~/d/p/kadr"), ["/h/development/projects/kadrell"]);
        assert_eq!(expand("~/dev/proj/hl"), ["/h/development/projects/homelab"]);
        assert!(expand("~/x/p").is_empty());
    }

    #[test]
    fn exact_name_beats_prefix_and_trailing_slash_lists_children() {
        assert_eq!(expand("~/development/projects/kadrell"), ["/h/development/projects/kadrell"]);
        assert_eq!(expand("~/d/p/kadrell/"), ["/h/development/projects/kadrell/app", "/h/development/projects/kadrell/windows"]);
        assert_eq!(expand("~"), ["/h"]);
    }

    #[test]
    fn normalize_and_short_path() {
        assert_eq!(normalize("~\\development\\x\\", "C:/Users/m"), "C:/Users/m/development/x");
        assert_eq!(normalize("~", "C:/Users/m"), "C:/Users/m");
        assert_eq!(short_path("C:/Users/M/development/x", "c:/users/m"), "~/development/x");
        assert_eq!(short_path("C:/Users/mx/x", "C:/Users/m"), "C:/Users/mx/x");
    }

    #[test]
    fn path_queries_are_detected() {
        assert!(is_path_query("~/d"));
        assert!(is_path_query("C:\\x"));
        assert!(is_path_query("a/b"));
        assert!(!is_path_query("kad"));
    }
}
