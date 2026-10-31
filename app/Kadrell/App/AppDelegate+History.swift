import AppKit

/// Warum eine Konversation gerade nicht übernommen werden kann (`AppDelegate.resumeBlocker`).
enum ResumeBlocker: Equatable {
    case inKadrell, noTranscript, running(pid: Int)
}

/// Verlaufssuche (`#` in ⌘P, ⌘⇧H): alle bisherigen Claude-Sessions finden und dort weitermachen.
extension AppDelegate {
    func wireHistory(_ src: inout PaletteWindow.Source) {
        let configDir = registry.cli.configDir
        src.history = { SessionHistory.shared.index }
        src.refreshHistory = { done in SessionHistory.shared.refresh(configDir: configDir, then: done) }
        src.liveBySessionId = Dictionary(registry.sessions.filter { !$0.sessionId.isEmpty }.map { ($0.sessionId, $0) }, uniquingKeysWith: { a, _ in a })
        src.onResumeHistory = { [weak self] s in self?.resumeFromHistory(s) }
    }

    /// Läuft sie schon in Kadrell, dorthin springen. Sonst `claude --resume` in ihrem Ordner, in dessen Gruppe.
    func resumeFromHistory(_ s: HistorySession) {
        if let live = registry.sessions.first(where: { $0.sessionId == s.sessionId || $0.id == s.sessionId }) {
            focusSession(live.id)
            return
        }
        if let problem = resumeProblem(s) {
            sheets.report(problem, title: String(localized: "Session lässt sich nicht fortsetzen", bundle: Bundle.app))
            return
        }
        startSession(group: nil, cwd: s.cwd, sessionId: s.sessionId)
    }

    private func resumeProblem(_ s: HistorySession) -> String? {
        var isDir: ObjCBool = false
        guard !s.cwd.isEmpty, FileManager.default.fileExists(atPath: s.cwd, isDirectory: &isDir), isDir.boolValue else {
            return String(localized: "Den Ordner \(Theme.shortPath(s.cwd)) gibt es nicht mehr.", bundle: Bundle.app)
        }
        switch resumeBlocker(s.sessionId) {
        case .running(let pid): return String(localized: "„\(s.title)“ läuft noch außerhalb dieses Kadrell (pid \(pid)). Erst dort beenden.", bundle: Bundle.app)
        case .noTranscript: return String(localized: "Das Transcript von „\(s.title)“ ist nicht mehr da.", bundle: Bundle.app)
        case .inKadrell, nil: return nil
        }
    }
}
