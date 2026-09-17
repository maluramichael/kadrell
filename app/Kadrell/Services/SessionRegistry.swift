import Foundation

/// Die Sessions, die Kadrell verwaltet, persistiert als JSON unter Application Support. Liest alle 2 s (versteckt/
/// inaktiv seltener, siehe `setBackground`) die Session-Dateien der eigenen Claude-Prozesse (Status, Name, aktuelle
/// sessionId) und meldet nur echte Änderungen.
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
    private var transcripts: [String: Transcript.Entry] = [:]
    /// `git worktree list` je Root-Ordner (`cwd`), neu geholt nur wenn sich der oberste Transcript-Kandidat
    /// einer Session in diesem Ordner ändert (siehe `applyActiveWorktree`), nicht bei jedem 2-s-Poll.
    private var worktreeCache: [String: [Worktree.Entry]] = [:]
    /// Oberster Transcript-Kandidat je Session beim letzten Abgleich, löst bei Änderung einen `worktreeCache`-Refresh aus.
    private var lastCandidate: [String: String] = [:]
    /// Ersatztitel je sessionId. Die erste Nachricht ändert sich nicht, einmal gefunden wird nie wieder gelesen.
    private var firstPrompts: [String: String] = [:]
    /// Schlüssel der Session je pid des eigenen Claude-Prozesses, liefert der AttachManager.
    var pids: () -> [Int: String] = { [:] }
    var onChange: (([Session]) -> Void)?
    private var task: Task<Void, Never>?
    /// Fenster versteckt oder App nicht aktiv (Menüleisten-Betrieb): seltener pollen, siehe `setBackground`.
    private(set) var isBackground = false

    static var defaultURL: URL { Profile.directory.appendingPathComponent("sessions.json") }

    init(cli: ClaudeCLI, url: URL = SessionRegistry.defaultURL) {
        self.cli = cli
        self.url = url
        if let data = try? Data(contentsOf: url), let s = try? JSONDecoder().decode([Session].self, from: data) { sessions = s }
    }

    /// `backgroundInterval` bleibt bewusst nah an `interval`: Sound und Marke „neu“ bei Statuswechseln
    /// (`Feedback.transitions`) sollen auch im Hintergrund zeitnah reagieren, nicht erst nach zehn Sekunden.
    func start(interval: TimeInterval = 2, backgroundInterval: TimeInterval = 5) {
        task?.cancel()
        task = Task { [weak self] in
            while !Task.isCancelled {
                await self?.pollNow()
                let background = self?.isBackground ?? false
                try? await Task.sleep(for: .seconds(background ? backgroundInterval : interval))
            }
        }
    }

    /// Fenster verdeckt/versteckt oder App im Hintergrund: seltener pollen. Kommt die App zurück, sofort neu laden,
    /// statt bis zu `interval` auf den nächsten Tick zu warten.
    func setBackground(_ background: Bool) {
        guard background != isBackground else { return }
        isBackground = background
        if !background { Task { await self.pollNow() } }
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
        let messages = await refreshTranscripts(merged)
        await applyActiveWorktree(&merged)
        // Während der awaits kann `add`/`remove`/`rename` gelaufen sein: nur die Live-Felder auf den aktuellen Stand legen,
        // sonst verschwindet eine eben angelegte Session (und `attach.sync` beendet ihren Prozess).
        merged = SessionRegistry.applyLive(merged, to: sessions)
        // Erster Poll meldet sich auch ohne Änderung: Registrierte hören darauf, um den Lade-Zustand zu verlassen.
        let firstPoll = !polled
        polled = true
        guard merged != sessions || messages != lastMessages || firstPoll else { return }
        let persisted = merged.map(\.stored) != sessions.map(\.stored)
        sessions = merged
        lastMessages = messages
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
            let configDir = cli.configDir
            let found = await Task.detached { missing.reduce(into: [String: String]()) { r, id in
                if let p = Transcript.path(sessionId: id, configDir: configDir), let t = Transcript.firstPrompt(path: p) { r[id] = t }
            } }.value
            firstPrompts.merge(found) { a, _ in a }
        }
        for i in merged.indices { merged[i].firstPrompt = firstPrompts[merged[i].sessionId] }
    }

    /// Liest nur gewachsene Transcripts neu und leitet daraus die Nachrichtenzeile ab (falls eingeschaltet).
    private func refreshTranscripts(_ merged: [Session]) async -> [String: String] {
        let ids = merged.map(\.sessionId), cache = transcripts, configDir = cli.configDir
        transcripts = await Task.detached { Transcript.refresh(ids, configDir: configDir, cache: cache) }.value
        var messages: [String: String] = [:]
        if Settings.showLastMessage {
            for s in merged { if let t = transcripts[s.sessionId]?.text { messages[s.id] = t } }
        }
        return messages
    }

    /// Viele Sessions laufen im Repo-Root, arbeiten per absoluten Pfaden aber in einem Geschwister- oder
    /// Unterordner-Worktree: `cwd` und Branch aus `merge()` stimmen dann nicht. Ordnet über die zuletzt
    /// angefassten Transcript-Pfade den tatsächlich aktiven Worktree zu (`Worktree.active`).
    private func applyActiveWorktree(_ merged: inout [Session]) async {
        for i in merged.indices {
            let sessionId = merged[i].sessionId
            guard let candidates = transcripts[sessionId]?.toolCandidates, let top = candidates.first else { continue }
            let cwd = merged[i].cwd
            if lastCandidate[sessionId] != top || worktreeCache[cwd] == nil {
                lastCandidate[sessionId] = top
                worktreeCache[cwd] = await Worktree.list(at: cwd)
            }
            guard let list = worktreeCache[cwd] else { continue }
            guard let active = Worktree.active(candidates: candidates, in: list) else { merged[i].activeWorktree = nil; continue }
            merged[i].activeWorktree = active.path
            merged[i].branch = active.branch ?? merged[i].branch
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

    /// Überträgt die vom Poll ermittelten Felder per `id` auf `current`. Neu hinzugekommene Sessions bleiben unverändert,
    /// entfernte kommen nicht zurück, `customName` bleibt aus `current`.
    static func applyLive(_ polled: [Session], to current: [Session]) -> [Session] {
        let byId = Dictionary(polled.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return current.map { s in
            guard let p = byId[s.id] else { return s }
            var s = s
            s.cwd = p.cwd; s.sessionId = p.sessionId; s.name = p.name; s.rawStatus = p.rawStatus; s.pid = p.pid
            s.waitingFor = p.waitingFor; s.branch = p.branch; s.activeWorktree = p.activeWorktree; s.firstPrompt = p.firstPrompt
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
