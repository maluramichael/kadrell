import Foundation

enum Git {
    /// Branch des Repos, das `path` enthält (auch Unterordner und Worktrees), bei detached HEAD die
    /// kurze Commit-Id. nil ohne Repo. Liest nur `.git/HEAD`, startet keinen Prozess.
    static func branch(at path: String) -> String? {
        var dir = URL(fileURLWithPath: path).standardizedFileURL
        while true {
            if let gitDir = resolveGitDir(dir.appendingPathComponent(".git")) {
                guard let head = try? String(contentsOf: gitDir.appendingPathComponent("HEAD"), encoding: .utf8) else { return nil }
                let h = head.trimmingCharacters(in: .whitespacesAndNewlines)
                let prefix = "ref: refs/heads/"
                return h.hasPrefix(prefix) ? String(h.dropFirst(prefix.count)) : String(h.prefix(7))
            }
            guard dir.path != "/" else { return nil }
            dir = dir.deletingLastPathComponent()
        }
    }

    /// `.git` ist ein Ordner, oder bei Worktrees eine Datei `gitdir: <pfad>`.
    private static func resolveGitDir(_ dotGit: URL) -> URL? {
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: dotGit.path, isDirectory: &isDir) else { return nil }
        if isDir.boolValue { return dotGit }
        guard let s = try? String(contentsOf: dotGit, encoding: .utf8), s.hasPrefix("gitdir: ") else { return nil }
        let p = s.dropFirst("gitdir: ".count).trimmingCharacters(in: .whitespacesAndNewlines)
        return URL(fileURLWithPath: p, relativeTo: dotGit.deletingLastPathComponent()).standardizedFileURL
    }
}
