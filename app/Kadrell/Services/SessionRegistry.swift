import Foundation

/// Die Sessions, die Kadrell verwaltet, persistiert als JSON unter Application Support. Liest alle 2 s die
/// Session-Dateien der eigenen Claude-Prozesse (Status, Name, aktuelle sessionId) und meldet nur echte Änderungen.
@MainActor
final class SessionRegistry {
    let cli: ClaudeCLI
    let url: URL
    private(set) var sessions: [Session] = []
    /// Letzte Textantwort von Claude je Session (`Session.id`), nur wenn in den Einstellungen eingeschaltet.
    private(set) var lastMessages: [String: String] = [:]
    private var transcripts: [String: Transcript.Entry] = [:]
    /// Ersatztitel je sessionId. Die erste Nachricht ändert sich nicht, einmal gefunden wird nie wieder gelesen.
    private var firstPrompts: [String: String] = [:]
    /// Schlüssel der Session je pid des eigenen Claude-Prozesses, liefert der AttachManager.
    var pids: () -> [Int: String] = { [:] }
    var onChange: (([Session]) -> Void)?
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
        Hooks.fire(.sessionNew, session, environment: cli.environment)
    }

    /// Leer = wieder der Titel von Claude Code.
    func rename(_ id: String, to name: String) {
        guard let i = sessions.firstIndex(where: { $0.id == id }) else { return }
        let n = name.trimmingCharacters(in: .whitespacesAndNewlines)
        sessions[i].customName = n.isEmpty ? nil : n
        save()
        onChange?(sessions)
    }

    func remove(_ ids: Set<String>) {
        for s in sessions where ids.contains(s.id) { Hooks.fire(.sessionRemove, s, environment: cli.environment) }
        sessions.removeAll { ids.contains($0.id) }
        save()
        onChange?(sessions)
    }

    func pollNow() async {
        let pids = pids()
        let agents = Agent.local(pids: Array(pids.keys), configDir: cli.configDir)
        var merged = SessionRegistry.merge(sessions, agents: agents, pids: pids)
        let missing = merged.filter { $0.name.isEmpty && firstPrompts[$0.sessionId] == nil }.map(\.sessionId)
        if !missing.isEmpty {
            let found = await Task.detached { missing.reduce(into: [String: String]()) { r, id in
                if let p = Transcript.path(sessionId: id), let t = Transcript.firstPrompt(path: p) { r[id] = t }
            } }.value
            firstPrompts.merge(found) { a, _ in a }
        }
        for i in merged.indices { merged[i].firstPrompt = firstPrompts[merged[i].sessionId] }
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
    }

    /// Live-Werte nur über die pid des eigenen Prozesses: eine fremde Session mit derselben sessionId
    /// (z. B. der gestoppte Hintergrund-Eintrag) ist nicht diese Kachel, und `/clear` wechselt die sessionId.
    static func merge(_ sessions: [Session], agents: [Agent], pids: [Int: String]) -> [Session] {
        var live: [String: Agent] = [:]
        for a in agents { if let pid = a.pid, let key = pids[pid] { live[key] = a } }
        return sessions.map { s in
            var s = s
            s.branch = Git.branch(at: s.cwd)
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
    var stored: [String] { [id, cwd, String(startedAt), sessionId, name, customName ?? ""] }
}
