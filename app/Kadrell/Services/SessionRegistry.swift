import Foundation

/// Die Sessions, die Kadrell verwaltet, persistiert als JSON unter Application Support. Pollt
/// `claude agents --json --all` alle 2 s für Status, Name und aktuelle sessionId und meldet nur echte Änderungen.
@MainActor
final class SessionRegistry {
    let cli: ClaudeCLI
    let url: URL
    private(set) var sessions: [Session] = []
    /// Alles aus dem letzten Poll, auch fremde Sessions (für die Übernahme alter Hintergrund-Sessions).
    private(set) var agents: [Agent] = []
    /// Letzte Textantwort von Claude je Session (`Session.id`), nur wenn in den Einstellungen eingeschaltet.
    private(set) var lastMessages: [String: String] = [:]
    private var transcripts: [String: Transcript.Entry] = [:]
    /// Schlüssel der Session je pid des eigenen Claude-Prozesses, liefert der AttachManager.
    var pids: () -> [Int: String] = { [:] }
    var onChange: (([Session]) -> Void)?
    var onError: ((Error) -> Void)?
    private var task: Task<Void, Never>?

    static var defaultURL: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return base.appendingPathComponent("de.malura.kadrell/sessions.json")
    }

    init(cli: ClaudeCLI, url: URL = SessionRegistry.defaultURL) {
        self.cli = cli
        self.url = url
        if let data = try? Data(contentsOf: url), let s = try? JSONDecoder().decode([Session].self, from: data) { sessions = s }
    }

    func start(interval: TimeInterval = 2) {
        task?.cancel()
        task = Task { [weak self] in
            while !Task.isCancelled {
                await self?.pollNow()
                try? await Task.sleep(for: .seconds(interval))
            }
        }
    }

    func add(_ session: Session) {
        sessions.append(session)
        save()
        onChange?(sessions)
    }

    func remove(_ ids: Set<String>) {
        sessions.removeAll { ids.contains($0.id) }
        save()
        onChange?(sessions)
    }

    func pollNow() async {
        do {
            agents = try await cli.agents()
            let merged = SessionRegistry.merge(sessions, agents: agents, pids: pids())
            var messages: [String: String] = [:]
            if Settings.showLastMessage {
                let ids = merged.map(\.sessionId), cache = transcripts
                transcripts = await Task.detached { Transcript.refresh(ids, cache: cache) }.value
                for s in merged { if let t = transcripts[s.sessionId]?.text { messages[s.id] = t } }
            }
            if merged != sessions || messages != lastMessages {
                let persisted = merged.map(\.stored) != sessions.map(\.stored)
                sessions = merged
                lastMessages = messages
                if persisted { save() }
                onChange?(merged)
            }
        } catch {
            onError?(error)
        }
    }

    /// Live-Werte nur über die pid des eigenen Prozesses: eine fremde Session mit derselben sessionId
    /// (z. B. der gestoppte Hintergrund-Eintrag) ist nicht diese Kachel, und `/clear` wechselt die sessionId.
    static func merge(_ sessions: [Session], agents: [Agent], pids: [Int: String]) -> [Session] {
        var live: [String: Agent] = [:]
        for a in agents { if let pid = a.pid, let key = pids[pid] { live[key] = a } }
        return sessions.map { s in
            var s = s
            guard let a = live[s.id] else { s.rawStatus = nil; s.pid = nil; s.waitingFor = nil; return s }
            s.sessionId = a.sessionId
            if !Session.isAutoName(a.name, cwd: s.cwd) { s.name = a.name }
            s.rawStatus = a.status
            s.pid = a.pid
            s.waitingFor = a.waitingFor
            return s
        }
    }

    private func save() {
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let enc = JSONEncoder()
        enc.outputFormatting = [.prettyPrinted, .sortedKeys]
        try? enc.encode(sessions).write(to: url, options: .atomic)
    }
}

private extension Session {
    var stored: [String] { [id, cwd, String(startedAt), sessionId, name] }
}
