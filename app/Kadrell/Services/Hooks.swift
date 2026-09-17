import Foundation

/// Ausführbare Skripte unter `~/.config/kadrell/hooks/<event>`, wie tmux `set-hook`. Aufruf ohne Warten:
/// `$1` Ordner, `$2` sessionId von Claude Code, `$3` Titel; dazu `KADRELL_*` in der Umgebung.
enum Hooks {
    enum Event: String { case sessionNew = "session-new", sessionFocus = "session-focus", sessionRemove = "session-remove" }

    static let dir = NSHomeDirectory() + "/.config/kadrell/hooks"

    static func fire(_ event: Event, _ s: Session, environment: [String: String]) {
        let path = dir + "/" + event.rawValue
        guard FileManager.default.isExecutableFile(atPath: path) else { return }
        // Bei aktivem Worktree (siehe `Session.activeWorktree`) zeigt der Hook dorthin statt auf den Repo-Root.
        let cwd = s.activeWorktree ?? s.cwd
        var env = environment
        env["KADRELL_EVENT"] = event.rawValue
        env["KADRELL_CWD"] = cwd
        env["KADRELL_SESSION_ID"] = s.sessionId
        env["KADRELL_SESSION_KEY"] = s.id
        env["KADRELL_TITLE"] = s.title
        env["KADRELL_BRANCH"] = (s.activeWorktree != nil ? Git.branch(at: cwd) : s.branch) ?? Git.branch(at: cwd) ?? ""
        let p = Process()
        p.executableURL = URL(fileURLWithPath: path)
        p.arguments = [cwd, s.sessionId, s.title]
        p.environment = env
        if FileManager.default.fileExists(atPath: cwd) { p.currentDirectoryURL = URL(fileURLWithPath: cwd) }
        p.standardInput = FileHandle.nullDevice
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        do { try p.run() } catch { ClaudeCLI.log.error("Hook \(path, privacy: .public): \(String(describing: error), privacy: .public)") }
    }
}
