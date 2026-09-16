import Foundation
import os

enum SessionStatus: String, Codable, CaseIterable, Sendable {
    case running, waiting, idle, error
}

/// Eine Session, die Kadrell selbst als Claude-Prozess im Terminal startet (`claude --resume`).
/// Gespeichert werden nur Schlüssel, Ordner, Start, sessionId und Name; Status und pid kommen live
/// aus `claude agents --json`, solange der eigene Prozess läuft.
struct Session: Codable, Equatable, Sendable, Identifiable {
    /// Stabiler Schlüssel für Gruppen und Auswahl: bei neuen Sessions die erste sessionId,
    /// bei übernommenen Hintergrund-Sessions deren alte kurze Id.
    let id: String
    let cwd: String
    let startedAt: Double
    /// Aktuelle Konversation. `/clear` oder `/resume` im Terminal wechselt sie, `--resume` braucht die neueste.
    var sessionId: String
    /// Leer, bis Claude Code einen echten Titel vergibt.
    var name: String
    /// Von Hand vergeben (F2, Stift). Geht immer vor `name`, Titel von Claude Code ändern daran nichts.
    var customName: String? = nil
    var rawStatus: String? = nil
    var pid: Int? = nil
    var waitingFor: String? = nil
    /// Git-Branch des Projektordners, wird bei jedem Refresh aus `.git/HEAD` gelesen.
    var branch: String? = nil
    /// Erste Nachricht aus dem Transcript, Ersatztitel solange Claude Code keinen vergeben hat (Haiku scheitert still).
    var firstPrompt: String? = nil

    enum CodingKeys: String, CodingKey { case id, cwd, startedAt, sessionId, name, customName }

    var title: String { customName ?? autoTitle }
    var autoTitle: String {
        if isShell { return URL(fileURLWithPath: cwd).lastPathComponent }
        return name.isEmpty ? firstPrompt ?? URL(fileURLWithPath: cwd).lastPathComponent + " · " + String(id.prefix(4)) : name
    }
    /// Terminal ohne Claude (⌘T): Login-Shell statt `claude`, erkennbar am Schlüssel.
    static let shellPrefix = "shell-"
    var isShell: Bool { id.hasPrefix(Session.shellPrefix) }
    var status: SessionStatus { Session.mapStatus(state: nil, status: rawStatus) }
    var startDate: Date { Date(timeIntervalSince1970: startedAt / 1000) }

    /// Ohne Titel benennt Claude Code interaktive Sessions `<ordner>-<2 hex>`, bei jedem Start neu.
    static func isAutoName(_ name: String, cwd: String) -> Bool {
        let base = URL(fileURLWithPath: cwd).lastPathComponent
        return name.hasPrefix(base + "-") && name.dropFirst(base.count + 1).range(of: "^[0-9a-f]{2}$", options: .regularExpression) != nil
    }

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

    func elapsed(now: Date = Date()) -> String {
        let m = Int((now.timeIntervalSince(startDate) / 60).rounded())
        if m < 60 { return "\(max(m, 0))m" }
        if m < 1440 { return "\(Int((Double(m) / 60).rounded()))h" }
        return "\(Int((Double(m) / 1440).rounded()))d"
    }
}

/// Ein Eintrag aus `claude agents --json --all`.
struct Agent: Decodable, Equatable, Sendable {
    /// Kurze Id (8 Hex), nur bei Hintergrund-Sessions aus `claude --bg`.
    let shortId: String?
    let cwd: String
    let kind: String
    let startedAt: Double
    let sessionId: String
    let name: String
    let state: String?
    let status: String?
    let pid: Int?
    let waitingFor: String?

    enum CodingKeys: String, CodingKey {
        case shortId = "id", cwd, kind, startedAt, sessionId, name, state, status, pid, waitingFor
    }

    /// Tolerant: Einträge ohne `name` (z. B. liegengebliebene Kopien) dürfen nicht die ganze Liste kippen.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        shortId = try c.decodeIfPresent(String.self, forKey: .shortId)
        cwd = try c.decodeIfPresent(String.self, forKey: .cwd) ?? ""
        kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? "background"
        startedAt = try c.decodeIfPresent(Double.self, forKey: .startedAt) ?? 0
        sessionId = try c.decodeIfPresent(String.self, forKey: .sessionId) ?? shortId ?? ""
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? shortId ?? sessionId
        state = try c.decodeIfPresent(String.self, forKey: .state)
        status = try c.decodeIfPresent(String.self, forKey: .status)
        pid = try c.decodeIfPresent(Int.self, forKey: .pid)
        waitingFor = try c.decodeIfPresent(String.self, forKey: .waitingFor)
    }

    /// Laufende Hintergrund-Session aus `claude --bg` (gestoppte haben keine pid).
    var isRunningBackground: Bool { kind == "background" && shortId != nil && pid != nil }

    static func decodeList(_ data: Data) throws -> [Agent] { try JSONDecoder().decode([Agent].self, from: data) }

    /// Claude Code schreibt je laufendem Prozess `<configDir>/sessions/<pid>.json` mit denselben Feldern, die
    /// `claude agents --json` liefert. Die Datei zu lesen kostet nichts, der CLI-Aufruf rund 0,3 s CPU und 180 MB.
    static func local(pids: [Int], configDir: String) -> [Agent] {
        pids.compactMap { pid in
            guard let data = FileManager.default.contents(atPath: configDir + "/sessions/\(pid).json") else { return nil }
            return try? JSONDecoder().decode(Agent.self, from: data)
        }
    }
}
