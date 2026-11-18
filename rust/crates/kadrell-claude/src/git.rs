//! Branch eines Ordners, gelesen aus `.git/HEAD` ohne Prozessstart (Swift: `Services/Git.swift`).

use crate::paths;
use std::path::Path;

/// Branch des Repos, das `path` enthält (auch Unterordner und Worktrees), bei detached HEAD die kurze Commit-Id.
/// nil ohne Repo.
pub fn branch(path: &str) -> Option<String> {
    let mut dir = paths::standardize(path);
    loop {
        if let Some(git_dir) = resolve_git_dir(&dir) {
            let head = std::fs::read_to_string(format!("{git_dir}/HEAD")).ok()?;
            let h = head.trim();
            return Some(match h.strip_prefix("ref: refs/heads/") {
                Some(name) => name.to_string(),
                None => h.chars().take(7).collect(),
            });
        }
        let up = paths::parent(&dir);
        if up.is_empty() || up == dir {
            return None;
        }
        dir = up;
    }
}

/// `.git` ist ein Ordner, oder bei Worktrees eine Datei `gitdir: <pfad>`.
fn resolve_git_dir(dir: &str) -> Option<String> {
    let dot_git = paths::join(dir, ".git");
    let p = Path::new(&dot_git);
    if p.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(p).ok()?;
    let target = text.strip_prefix("gitdir: ")?.trim();
    let full = if paths::is_abs(target) { target.to_string() } else { paths::join(dir, target) };
    Some(paths::standardize(&full))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;

    #[test]
    fn branch_from_repo_worktree_and_detached() {
        let root = TempDir::new("git");
        let repo = root.mkdir("repo");
        root.mkdir("repo/.git/worktrees/wt");
        root.mkdir("repo/src/deep");
        root.write("repo/.git/HEAD", "ref: refs/heads/feature/x\n");
        assert_eq!(branch(&repo).as_deref(), Some("feature/x"));
        assert_eq!(branch(&format!("{repo}/src/deep")).as_deref(), Some("feature/x"));

        let wt = root.mkdir("wt");
        root.write("wt/.git", &format!("gitdir: {repo}/.git/worktrees/wt\n"));
        root.write("repo/.git/worktrees/wt/HEAD", "ref: refs/heads/master\n");
        assert_eq!(branch(&wt).as_deref(), Some("master"));

        root.write("repo/.git/HEAD", "7080dfd1234567890abcdef\n");
        assert_eq!(branch(&repo).as_deref(), Some("7080dfd"));

        assert_eq!(branch(&root.sub("none")), None);
    }

    #[test]
    fn relative_gitdir_is_resolved_from_the_worktree_folder() {
        let root = TempDir::new("git");
        root.mkdir("main/.git/worktrees/w");
        root.write("main/.git/worktrees/w/HEAD", "ref: refs/heads/rel\n");
        root.mkdir("w");
        root.write("w/.git", "gitdir: ../main/.git/worktrees/w\n");
        assert_eq!(branch(&root.sub("w")).as_deref(), Some("rel"));
    }
}
