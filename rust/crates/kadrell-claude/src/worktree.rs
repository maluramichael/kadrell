//! Ordnet einer Session ihren tatsächlich aktiven Worktree zu: Prozess-`cwd` und Transcript bleiben oft im Repo-Root,
//! während per absoluten Pfaden in einem Geschwister- oder Unterordner-Worktree gearbeitet wird
//! (Swift: `Services/Worktree.swift`).

use crate::cli::CommandRunner;
use crate::paths;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub branch: Option<String>,
}

/// `git worktree list --porcelain` im Repo von `cwd`, geparst. Erster Eintrag ist laut Git immer die Hauptarbeitskopie.
/// Leer ohne Repo oder wenn `git` fehlschlägt.
pub fn list(runner: &dyn CommandRunner, cwd: &str) -> Vec<Entry> {
    let args: Vec<String> = ["-C", cwd, "worktree", "list", "--porcelain"].map(String::from).to_vec();
    match runner.run("git", &args, None, None, Duration::from_secs(60)) {
        Ok(r) if r.status == 0 => parse_porcelain(&r.stdout),
        _ => Vec::new(),
    }
}

/// Blöcke durch Leerzeile getrennt: `worktree <pfad>`, `HEAD <sha>`, `branch refs/heads/<name>` (fehlt bei detached
/// HEAD), dazu ggf. `bare`/`locked`/`prunable`, die hier nicht gebraucht werden. Pfade mit `/` (Git liefert das auch
/// unter Windows so).
pub fn parse_porcelain(output: &str) -> Vec<Entry> {
    let mut result = Vec::new();
    let mut path: Option<String> = None;
    let mut branch: Option<String> = None;
    let mut flush = |path: &mut Option<String>, branch: &mut Option<String>| {
        if let Some(p) = path.take() {
            result.push(Entry { path: p, branch: branch.take() });
        }
        *branch = None;
    };
    for line in output.lines() {
        if line.is_empty() {
            flush(&mut path, &mut branch);
        } else if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(p.to_string());
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            branch = Some(b.to_string());
        }
    }
    flush(&mut path, &mut branch);
    result
}

fn under(path: &str, root: &str) -> bool {
    path == root || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

/// Längster passender Präfix gewinnt: ein Worktree kann unter dem Repo liegen (z. B. `.claude/worktrees/x`), dann
/// matchen sowohl der Repo-Root als auch der Worktree selbst.
pub fn longest_prefix_match<'a>(path: &str, worktrees: &'a [Entry]) -> Option<&'a Entry> {
    let path = paths::fwd(path);
    worktrees.iter().filter(|w| under(&path, &paths::fwd(&w.path))).max_by_key(|w| w.path.chars().count())
}

/// Neuester Kandidat (Transcript-Tool-Pfade/-Kommandos, neueste zuerst) zuerst, der einen der `worktrees` referenziert,
/// gewinnt. Kandidaten ohne Treffer (z. B. ein Plan unter `~/.claude/plans`) werden übersprungen. Trifft der jüngste
/// Treffer die Hauptarbeitskopie, gilt wieder das Hauptverzeichnis (nil), sonst der gefundene Worktree.
pub fn active<'a>(candidates: &[String], worktrees: &'a [Entry]) -> Option<&'a Entry> {
    let main = worktrees.first().filter(|_| worktrees.len() > 1)?;
    let hit = candidates.iter().find_map(|c| match_candidate(c, worktrees))?;
    if hit.path == main.path { None } else { Some(hit) }
}

/// Datei-/Notebook-Pfade als Präfix, Bash-Kommandos (auch `cd <pfad>`) als Teilstring gegen den Worktree-Pfad.
/// Öffentlich, damit die Registry prüfen kann, ob ein neuer Kandidat noch von der gecachten Liste abgedeckt ist,
/// ohne für jede Kandidatenänderung `git worktree list` neu aufzurufen.
pub fn match_candidate<'a>(candidate: &str, worktrees: &'a [Entry]) -> Option<&'a Entry> {
    if paths::is_abs(candidate) {
        return longest_prefix_match(candidate, worktrees);
    }
    let candidate = paths::fwd(candidate);
    worktrees.iter().filter(|w| candidate.contains(&paths::fwd(&w.path))).max_by_key(|w| w.path.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, branch: Option<&str>) -> Entry {
        Entry { path: path.into(), branch: branch.map(String::from) }
    }

    /// `git worktree list --porcelain`: Hauptarbeitskopie zuerst, `branch` fehlt bei detached HEAD, `locked`/`bare`
    /// werden ignoriert statt die Blockerkennung zu stören.
    #[test]
    fn parse_porcelain_blocks() {
        let output = "worktree /repo\nHEAD 1111111111111111111111111111111111111111\nbranch refs/heads/master\n\n\
                      worktree /repo/.claude/worktrees/x\nHEAD 2222222222222222222222222222222222222222\nbranch refs/heads/feature-x\nlocked claude agent x\n\n\
                      worktree /repo-detached\nHEAD 3333333333333333333333333333333333333333\ndetached\n\n";
        assert_eq!(
            parse_porcelain(output),
            [entry("/repo", Some("master")), entry("/repo/.claude/worktrees/x", Some("feature-x")), entry("/repo-detached", None)]
        );
    }

    /// Längster Präfix gewinnt: ein Worktree unter dem Repo matcht dessen eigenen Pfad, nicht den Repo-Root.
    #[test]
    fn longest_prefix_wins() {
        let wts = [entry("/repo", Some("master")), entry("/repo/.claude/worktrees/x", Some("feature-x"))];
        assert_eq!(longest_prefix_match("/repo/.claude/worktrees/x/src/a.swift", &wts).unwrap().branch.as_deref(), Some("feature-x"));
        assert_eq!(longest_prefix_match("/repo/src/a.swift", &wts).unwrap().branch.as_deref(), Some("master"));
        assert!(longest_prefix_match("/elsewhere/a.swift", &wts).is_none());
        assert!(longest_prefix_match("/repository/a", &wts).is_none(), "Präfix nur an Ordnergrenzen");
    }

    /// Kern der Erkennung: jüngster Treffer gewinnt, Nicht-Treffer werden übersprungen, ein Treffer in der
    /// Hauptarbeitskopie schaltet zurück auf nil, ein Repo ohne Nebenworktree bleibt immer nil.
    #[test]
    fn active_worktree() {
        let main = entry("/repo", Some("master"));
        let sibling = entry("/repo-wt-1", Some("feature-x"));
        let wts = [main.clone(), sibling.clone()];
        let c = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        assert_eq!(active(&c(&["cd /repo-wt-1 && npm test"]), &wts), Some(&sibling));
        assert_eq!(active(&c(&["/repo-wt-1/src/a.swift"]), &wts), Some(&sibling));
        // Fremder Pfad ohne Treffer wird übersprungen, der ältere Treffer im Nebenworktree bleibt aktiv.
        assert_eq!(active(&c(&["/Users/dev/.claude/plans/x.md", "/repo-wt-1/a"]), &wts), Some(&sibling));
        // Treffer in der Hauptarbeitskopie ist jünger: zurück auf das Hauptverzeichnis.
        assert_eq!(active(&c(&["/repo/src/a.swift", "/repo-wt-1/a"]), &wts), None);
        // Ohne jeden Treffer: nichts bekannt, Hauptverzeichnis gilt.
        assert_eq!(active(&c(&["/Users/dev/.claude/plans/x.md"]), &wts), None);
        // Nur ein Worktree im Repo (kein Nebenworktree möglich): immer nil.
        assert_eq!(active(&c(&["/repo-wt-1/a"]), &[main]), None);
    }

    /// `match_candidate` deckt Datei-Präfix wie Bash-Kommando ab (die Registry lädt die Liste nur neu, wenn es nichts trifft).
    #[test]
    fn match_covers_path_and_command_candidates() {
        let wts = [entry("/repo", Some("master")), entry("/repo-wt-1", Some("feature-x"))];
        assert!(match_candidate("/repo-wt-1/src/a.swift", &wts).is_some());
        assert!(match_candidate("cd /repo-wt-1 && npm test", &wts).is_some());
        assert!(match_candidate("/elsewhere/a.swift", &wts).is_none());
    }

    #[test]
    fn windows_style_paths_match_too() {
        let wts = [entry("C:/dev/repo", Some("master")), entry("C:/dev/repo-wt", Some("f"))];
        assert_eq!(match_candidate("C:\\dev\\repo-wt\\a.rs", &wts).unwrap().branch.as_deref(), Some("f"));
    }
}
