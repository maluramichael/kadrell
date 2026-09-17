import Foundation

/// Ausführbare Skripte unter `~/.config/kadrell/hooks/<event>`, wie tmux `set-hook`. Aufruf ohne Warten:
/// `$1` Ordner, `$2` sessionId von Claude Code, `$3` Titel; dazu `KADRELL_*` in der Umgebung.
enum Hooks {
    enum Event: String { case sessionNew = "session-new", sessionFocus = "session-focus", sessionRemove = "session-remove" }

    static let dir = NSHomeDirectory() + "/.config/kadrell/hooks"

    static func fire(_ event: Event, _ s: Session, environment: [String: String]) {
        let path = dir + "/" + event.rawValue
        guard FileManager.default.isExecutableFile(atPath: path) else { return }
        guard trusted(path), trusted(dir) else {
            ClaudeCLI.log.error("Hook \(path, privacy: .public) ignoriert: gehört nicht dir oder ist für andere beschreibbar")
            return
        }
        // Bei aktivem Worktree (siehe `Session.activeWorktree`) zeigt der Hook dorthin statt auf den Repo-Root.
        let cwd = s.activeWorktree ?? s.cwd
        var env = environment
        env["KADRELL_EVENT"] = event.rawValue
        env["KADRELL_CWD"] = cwd
        env["KADRELL_SESSION_ID"] = s.sessionId
        env["KADRELL_SESSION_KEY"] = s.id
        env["KADRELL_TITLE"] = s.title
        env["KADRELL_BRANCH"] = (s.activeWorktree != nil ? Git.branch(at: cwd) : s.branch) ?? Git.branch(at: cwd) ?? ""
        do { try ProcessRunner.spawn(path, [cwd, s.sessionId, s.title], environment: env, cwd: FileManager.default.fileExists(atPath: cwd) ? cwd : nil, discardOutput: true) } catch { ClaudeCLI.log.error("Hook \(path, privacy: .public): \(String(describing: error), privacy: .public)") }
    }

    /// Wie sshd bei `authorized_keys`: nur Dateien des eigenen Benutzers, die weder Gruppe noch andere schreiben dürfen.
    static func trusted(_ path: String) -> Bool {
        var st = stat()
        return stat(path, &st) == 0 && st.st_uid == getuid() && st.st_mode & (S_IWGRP | S_IWOTH) == 0
    }
}
