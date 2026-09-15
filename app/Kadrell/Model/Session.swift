import Foundation
import os

enum SessionStatus: String, Codable, CaseIterable, Sendable {
    case running, waiting, idle, error
}

/// Ein Eintrag aus `claude agents --json --all`.
struct Session: Codable, Equatable, Sendable, Identifiable {
    /// Kurze Hintergrund-Id (8 Hex), die `attach`, `stop`, `logs`, `rm` nehmen. Interaktive Sessions haben keine.
    let shortId: String?
    let cwd: String
    let kind: String
    let startedAt: Double
    let sessionId: String
    let name: String
    let state: String?
    let rawStatus: String?
    let pid: Int?
    let waitingFor: String?

    enum CodingKeys: String, CodingKey {
        case shortId = "id", cwd, kind, startedAt, sessionId, name, state, rawStatus = "status", pid, waitingFor
    }

    init(shortId: String?, cwd: String, kind: String, startedAt: Double, sessionId: String, name: String,
         state: String? = nil, rawStatus: String? = nil, pid: Int? = nil, waitingFor: String? = nil) {
        self.shortId = shortId; self.cwd = cwd; self.kind = kind; self.startedAt = startedAt; self.sessionId = sessionId
        self.name = name; self.state = state; self.rawStatus = rawStatus; self.pid = pid; self.waitingFor = waitingFor
    }

    /// Stabiler Schlüssel: die kurze Hintergrund-Id. `/resume` in Claude Code wechselt die `sessionId`,
    /// die `id` des Hintergrundjobs bleibt, damit die Kachel ihren Platz behält.
    var id: String { shortId ?? sessionId }
    /// Anzeigename: bis Claude Code nach der ersten Nachricht einen Titel vergibt, steht im JSON die Id.
    /// Dann Ordnername plus Id-Anfang, damit zwei neue Sessions unterscheidbar bleiben.
    var title: String { name == id ? URL(fileURLWithPath: cwd).lastPathComponent + " · " + String(id.prefix(4)) : name }
    var isBackground: Bool { kind == "background" }
    /// Platzhalter, den Kadrell sofort anlegt, während `claude --bg` noch startet.
    var isPending: Bool { shortId?.hasPrefix("pending-") == true }
    static func pending(cwd: String) -> Session {
        Session(shortId: "pending-" + UUID().uuidString.lowercased().prefix(8), cwd: cwd, kind: "background",
                startedAt: Date().timeIntervalSince1970 * 1000, sessionId: "", name: "startet …", rawStatus: "starting")
    }
    var isInteractive: Bool { kind == "interactive" }
    /// Beendet oder gestoppt: sichtbar, aber erst nach `--bg --resume` wieder anhängbar.
    var isDone: Bool { state == "done" || state == "stopped" }
    /// Hintergrund-Eintrag ohne `pid`: der Prozess ist weg, `attach` scheitert („no saved transcript“); nur `respawn` hilft.
    var isStale: Bool { isBackground && !isDone && pid == nil && !isPending }
    var canAttach: Bool { isBackground && !isDone && shortId != nil && pid != nil }
    var status: SessionStatus { Session.mapStatus(state: state, status: rawStatus) }
    var startDate: Date { Date(timeIntervalSince1970: startedAt / 1000) }

    static let log = Logger(subsystem: "de.malura.kadrell", category: "session")
    nonisolated(unsafe) private static var loggedUnknown = Set<String>()

    static func mapStatus(state: String?, status: String?) -> SessionStatus {
        for raw in [status, state].compactMap({ $0 }) {
            let v = raw.lowercased()
            switch v {
            case "busy", "working", "running", "active", "starting": return .running
            case "waiting", "blocked": return .waiting
            case "idle", "done", "stopped": return .idle
            default:
                if v.contains("error") || v.contains("fail") { return .error }
                if v.contains("run") || v.contains("work") || v.contains("active") { return .running }
                if !loggedUnknown.contains(v) {
                    loggedUnknown.insert(v)
                    log.warning("Unbekannter Session-Wert '\(v, privacy: .public)', behandle als idle")
                }
                return .idle
            }
        }
        return .idle
    }

    /// Dieselbe `sessionId` kann mehrfach vorkommen (z. B. interaktiv und als Hintergrund-Eintrag);
    /// der Eintrag mit kurzer Id gewinnt, sonst der erste.
    static func decodeList(_ data: Data) throws -> [Session] {
        var seen: [String: Int] = [:]
        var out: [Session] = []
        for s in try JSONDecoder().decode([Session].self, from: data) {
            if let i = seen[s.sessionId] {
                if out[i].shortId == nil, s.shortId != nil { out[i] = s }
            } else {
                seen[s.sessionId] = out.count
                out.append(s)
            }
        }
        return out
    }

    func elapsed(now: Date = Date()) -> String {
        let m = Int((now.timeIntervalSince(startDate) / 60).rounded())
        if m < 60 { return "\(max(m, 0))m" }
        if m < 1440 { return "\(Int((Double(m) / 60).rounded()))h" }
        return "\(Int((Double(m) / 1440).rounded()))d"
    }
}
