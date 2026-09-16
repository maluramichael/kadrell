import Foundation

/// Ausführbare Skripte unter `~/.config/kadrell/hooks/<event>`, wie tmux `set-hook`. Aufruf ohne Warten:
/// `$1` Ordner, `$2` sessionId von Claude Code, `$3` Titel; dazu `KADRELL_*` in der Umgebung.
enum Hooks {
    enum Event: String { case sessionNew = "session-new", sessionFocus = "session-focus", sessionRemove = "session-remove" }

    static let dir = NSHomeDirectory() + "/.config/kadrell/hooks"

    static func fire(_ event: Event, _ s: Session, environment: [String: String]) {
        let path = dir + "/" + event.rawValue
        guard FileManager.default.isExecutableFile(atPath: path) else { return }
        var env = environment
        env["KADRELL_EVENT"] = event.rawValue
        env["KADRELL_CWD"] = s.cwd
        env["KADRELL_SESSION_ID"] = s.sessionId
        env["KADRELL_SESSION_KEY"] = s.id
        env["KADRELL_TITLE"] = s.title
        env["KADRELL_BRANCH"] = s.branch ?? Git.branch(at: s.cwd) ?? ""
        let p = Process()
        p.executableURL = URL(fileURLWithPath: path)
        p.arguments = [s.cwd, s.sessionId, s.title]
        p.environment = env
        if FileManager.default.fileExists(atPath: s.cwd) { p.currentDirectoryURL = URL(fileURLWithPath: s.cwd) }
        p.standardInput = FileHandle.nullDevice
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        do { try p.run() } catch { ClaudeCLI.log.error("Hook \(path, privacy: .public): \(String(describing: error), privacy: .public)") }
    }
}
