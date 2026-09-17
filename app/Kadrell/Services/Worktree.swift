import Foundation

/// Ordnet einer Session ihren tatsächlich aktiven Worktree zu: Prozess-`cwd` und Transcript bleiben oft im
/// Repo-Root, während per absoluten Pfaden in einem Geschwister- oder Unterordner-Worktree gearbeitet wird.
enum Worktree {
    struct Entry: Equatable, Sendable {
        let path: String
        let branch: String?
    }

    /// `git worktree list --porcelain` im Repo von `cwd`, geparst. Erster Eintrag ist laut Git immer die
    /// Hauptarbeitskopie. Leer ohne Repo oder wenn `git` fehlschlägt.
    static func list(at cwd: String) async -> [Entry] {
        guard let result = try? await ClaudeCLI.runRaw("/usr/bin/git", ["-C", cwd, "worktree", "list", "--porcelain"], environment: nil, cwd: nil),
              result.status == 0 else { return [] }
        return parsePorcelain(result.output)
    }

    /// Blöcke durch Leerzeile getrennt: `worktree <pfad>`, `HEAD <sha>`, `branch refs/heads/<name>` (fehlt bei
    /// detached HEAD), dazu ggf. `bare`/`locked`/`prunable`, die hier nicht gebraucht werden.
    static func parsePorcelain(_ output: String) -> [Entry] {
        var result: [Entry] = []
        var path: String?
        var branch: String?
        func flush() {
            if let path { result.append(Entry(path: path, branch: branch)) }
            path = nil; branch = nil
        }
        for line in output.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.isEmpty { flush() }
            else if line.hasPrefix("worktree ") { path = String(line.dropFirst("worktree ".count)) }
            else if line.hasPrefix("branch refs/heads/") { branch = String(line.dropFirst("branch refs/heads/".count)) }
        }
        flush()
        return result
    }

    /// Längster passender Präfix gewinnt: ein Worktree kann unter dem Repo liegen (z. B. `.claude/worktrees/x`),
    /// dann matchen sowohl der Repo-Root als auch der Worktree selbst.
    static func longestPrefixMatch(_ path: String, in worktrees: [Entry]) -> Entry? {
        worktrees.filter { path == $0.path || path.hasPrefix($0.path + "/") }.max { $0.path.count < $1.path.count }
    }

    /// Neuester Kandidat (Transcript-Tool-Pfade/-Kommandos, neueste zuerst) zuerst, der einen der `worktrees`
    /// referenziert, gewinnt. Kandidaten ohne Treffer (z. B. ein Plan unter `~/.claude/plans`) werden übersprungen,
    /// bis einer trifft oder die Liste endet. Trifft der jüngste Treffer die Hauptarbeitskopie, gilt wieder das
    /// Hauptverzeichnis (nil), sonst der gefundene Worktree.
    static func active(candidates: [String], in worktrees: [Entry]) -> Entry? {
        guard let main = worktrees.first, worktrees.count > 1 else { return nil }
        for candidate in candidates {
            guard let hit = match(candidate, in: worktrees) else { continue }
            return hit.path == main.path ? nil : hit
        }
        return nil
    }

    /// Datei-/Notebook-Pfade als Präfix, Bash-Kommandos (auch `cd <pfad>`) als Teilstring gegen den Worktree-Pfad.
    /// Intern statt `private`: `SessionRegistry` prüft damit, ob ein neuer Kandidat noch von der gecachten
    /// Liste abgedeckt ist, ohne für jede Kandidatenänderung `git worktree list` neu aufzurufen.
    static func match(_ candidate: String, in worktrees: [Entry]) -> Entry? {
        if candidate.hasPrefix("/") { return longestPrefixMatch(candidate, in: worktrees) }
        return worktrees.filter { candidate.contains($0.path) }.max { $0.path.count < $1.path.count }
    }
}
