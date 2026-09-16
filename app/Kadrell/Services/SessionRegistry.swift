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
    /// Binary nicht gefunden oder `claude agents` schlägt fehl. Von außen über `fail(_:)` gesetzt, kein Poll räumt es wieder ab.
    private(set) var lastError: String?
    /// Ob schon ein Poll durchgelaufen ist, für den Lade-Zustand davor.
    private(set) var polled = false
    /// Sessions, deren Transcript seit dem letzten Fokus gewachsen ist (`Session.id`).
    private(set) var unread: Set<String> = []
    private var transcripts: [String: Transcript.Entry] = [:]
    /// Transcript-Größe je Session beim letzten Fokus, übersteht einen Neustart der App.
    private var lastSeenSize: [String: Int] {
        get { UserDefaults.standard.dictionary(forKey: "session.lastSeenSize") as? [String: Int] ?? [:] }
        set { UserDefaults.standard.set(newValue, forKey: "session.lastSeenSize") }
    }
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

    /// Binary fehlt oder `claude agents` schlägt fehl: vom Aufrufer gesetzt, kein Poll räumt es automatisch wieder ab.
    func fail(_ message: String) {
        lastError = message
        onChange?(sessions)
    }

    func pollNow() async {
        let pids = pids()
        let agents = Agent.local(pids: Array(pids.keys), configDir: cli.configDir)
        var merged = SessionRegistry.merge(sessions, agents: agents, pids: pids)
        applyShellCwd(&merged, pids: pids)
        await fillFirstPrompts(&merged)
        let (messages, unread) = await refreshTranscripts(merged)
        // Erster Poll meldet sich auch ohne Änderung: Registrierte hören darauf, um den Lade-Zustand zu verlassen.
        let firstPoll = !polled
        polled = true
        guard merged != sessions || messages != lastMessages || unread != self.unread || firstPoll else { return }
        let persisted = merged.map(\.stored) != sessions.map(\.stored)
        sessions = merged
        lastMessages = messages
        self.unread = unread
        if persisted { save() }
        onChange?(merged)
    }

    /// Bei Terminals ohne Claude folgt der Ordner dem `cd` der Shell.
    private func applyShellCwd(_ merged: inout [Session], pids: [Int: String]) {
        for (pid, key) in pids {
            guard let i = merged.firstIndex(where: { $0.id == key && $0.isShell }), let dir = SessionRegistry.cwd(pid: pid_t(pid)) else { continue }
            merged[i].cwd = dir
        }
    }

    private func fillFirstPrompts(_ merged: inout [Session]) async {
        let missing = merged.filter { $0.name.isEmpty && !$0.isShell && firstPrompts[$0.sessionId] == nil }.map(\.sessionId)
        if !missing.isEmpty {
            let found = await Task.detached { missing.reduce(into: [String: String]()) { r, id in
                if let p = Transcript.path(sessionId: id), let t = Transcript.firstPrompt(path: p) { r[id] = t }
            } }.value
            firstPrompts.merge(found) { a, _ in a }
        }
        for i in merged.indices { merged[i].firstPrompt = firstPrompts[merged[i].sessionId] }
    }

    /// Liest nur gewachsene Transcripts neu und leitet daraus die Nachrichtenzeile (falls eingeschaltet)
    /// sowie den Ungelesen-Status ab: gewachsen seit `lastSeenSize` vom letzten Fokus.
    private func refreshTranscripts(_ merged: [Session]) async -> (messages: [String: String], unread: Set<String>) {
        let ids = merged.map(\.sessionId), cache = transcripts
        transcripts = await Task.detached { Transcript.refresh(ids, cache: cache) }.value
        var messages: [String: String] = [:]
        if Settings.showLastMessage {
            for s in merged { if let t = transcripts[s.sessionId]?.text { messages[s.id] = t } }
        }
        // Session ohne gespeicherten Stand (neu oder erster Start nach dem Update): was schon da ist, gilt als gelesen.
        var seen = lastSeenSize
        let unknown = merged.filter { seen[$0.id] == nil && transcripts[$0.sessionId] != nil }
        for s in unknown { seen[s.id] = Int(transcripts[s.sessionId]!.size) }
        if !unknown.isEmpty { lastSeenSize = seen }
        let unread = Set(merged.compactMap { s -> String? in
            guard let size = transcripts[s.sessionId]?.size, let last = seen[s.id], size > last else { return nil }
            return s.id
        })
        return (messages, unread)
    }

    /// Fokus auf eine Session: Ungelesen-Zustand für sie zurücksetzen.
    func markSeen(_ id: String) {
        guard let s = sessions.first(where: { $0.id == id }), let size = transcripts[s.sessionId]?.size else { return }
        var seen = lastSeenSize
        seen[id] = Int(size)
        lastSeenSize = seen
        unread.remove(id)
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

    /// Aktueller Ordner eines Prozesses, wie `lsof -d cwd`.
    static func cwd(pid: pid_t) -> String? {
        var info = proc_vnodepathinfo()
        let size = Int32(MemoryLayout<proc_vnodepathinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &info, size) == size else { return nil }
        let path = withUnsafeBytes(of: &info.pvi_cdir.vip_path) { String(cString: $0.bindMemory(to: CChar.self).baseAddress!) }
        return path.isEmpty ? nil : path
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
