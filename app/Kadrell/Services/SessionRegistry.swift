import Foundation
import os

/// Die Sessions, die Kadrell verwaltet, persistiert als JSON unter Application Support. Liest alle 2 s (versteckt/
/// inaktiv seltener, siehe `setBackground`) die Session-Dateien der eigenen Claude-Prozesse (Status, Name, aktuelle
/// sessionId) und meldet nur echte Änderungen.
@MainActor
final class SessionRegistry {
    private static let log = Logger(subsystem: "de.malura.kadrell", category: "registry")
    private static let schemaVersion = 1
    let cli: ClaudeCLI
    let url: URL
    private(set) var sessions: [Session] = []
    /// Letzte Textantwort von Claude je Session (`Session.id`), nur wenn in den Einstellungen eingeschaltet.
    private(set) var lastMessages: [String: String] = [:]
    /// Binary nicht gefunden, `claude agents` schlägt fehl, sessions.json ist kaputt oder Speichern schlägt fehl.
    /// Von außen (oder aus `save`/`init`) über `fail(_:)` gesetzt, kein Poll räumt es wieder ab.
    private(set) var lastError: String?
    /// Ob schon ein Poll durchgelaufen ist, für den Lade-Zustand davor.
    private(set) var polled = false
    /// Umschlag nennt eine höhere Version als diese Kadrell-Version kennt: nur lesen, nie überschreiben.
    private var readOnly = false
    /// Zuletzt geschlossene Sessions (neueste zuerst, höchstens `maxClosed`): der Schließen-Dialog verspricht
    /// "Konversation bleibt erhalten", `remove` löschte die sessionId bisher trotzdem restlos aus Kadrells Daten.
    private(set) var closed: [ClosedSession] = []
    private let closedURL: URL
    private static let maxClosed = 50
    private var transcripts: [String: Transcript.Entry] = [:]
    /// `git worktree list` je Root-Ordner (`cwd`), neu geholt nur wenn der oberste Transcript-Kandidat einer
    /// Session in diesem Ordner von der gecachten Liste nicht mehr abgedeckt ist (siehe `applyActiveWorktree`),
    /// nicht bei jeder Kandidatenänderung: Tool-Aufrufe innerhalb desselben Worktrees ändern den Kandidaten
    /// praktisch bei jedem Poll, ohne dass ein neuer Worktree entstanden sein kann.
    private var worktreeCache: [String: [Worktree.Entry]] = [:]
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
        self.closedURL = url.deletingLastPathComponent().appendingPathComponent("closed.json")
        let result = JSONFile.loadArray(Session.self, from: url, currentVersion: Self.schemaVersion) { [weak self] msg in self?.lastError = msg }
        readOnly = result.newerThanKnown
        sessions = SessionRegistry.dedupedById(result.items)
        closed = JSONFile.loadArray(ClosedSession.self, from: closedURL, currentVersion: Self.schemaVersion).items
    }

    /// Erste gewinnt: eine doppelte Id (Handbearbeitung, `offerAdopt` mit einer schon vorhandenen Kurz-Id) würde
    /// `WorkspaceView.reload` sonst bei jedem Start abstürzen lassen.
    private static func dedupedById(_ sessions: [Session]) -> [Session] {
        var seen = Set<String>()
        let result = sessions.filter { seen.insert($0.id).inserted }
        if result.count != sessions.count { log.warning("sessions.json: \(sessions.count - result.count) doppelte Id(s) bereinigt") }
        return result
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
        guard !sessions.contains(where: { $0.id == session.id }) else {
            Self.log.warning("add: Id '\(session.id, privacy: .public)' existiert schon, ignoriert")
            return
        }
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
        let removed = sessions.filter { ids.contains($0.id) }
        for s in removed { Hooks.fire(.sessionRemove, s, environment: cli.environment) }
        sessions.removeAll { ids.contains($0.id) }
        save()
        archive(removed)
        onChange?(sessions)
    }

    /// Merkt sich die sessionId geschlossener Sessions, neueste zuerst, gedeckelt auf `maxClosed`.
    private func archive(_ removed: [Session]) {
        guard !removed.isEmpty else { return }
        let now = Date().timeIntervalSince1970
        closed.insert(contentsOf: removed.map { ClosedSession(session: $0, closedAt: now) }, at: 0)
        closed = Array(closed.prefix(Self.maxClosed))
        guard !readOnly else { return }
        JSONFile.saveArray(closed, to: closedURL, version: Self.schemaVersion) { [weak self] msg in self?.lastError = msg }
    }

    /// Binary fehlt oder `claude agents` schlägt fehl: vom Aufrufer gesetzt, kein Poll räumt es automatisch wieder ab.
    func fail(_ message: String) {
        lastError = message
        onChange?(sessions)
    }

    func pollNow() async {
        let pids = pids(), priorSessions = sessions, configDir = cli.configDir
        // Agent.local liest je pid eine Datei, merge() je Session `.git/HEAD` (Git.branch), applyShellCwd() je
        // Shell-pid /proc: reines I/O ohne UI-Zugriff, deshalb abseits des Main-Threads statt bei jedem Poll
        // Tastatureingaben in allen Terminals zu blockieren.
        var merged = await Task.detached {
            let agents = Agent.local(pids: Array(pids.keys), configDir: configDir)
            var merged = SessionRegistry.merge(priorSessions, agents: agents, pids: pids)
            SessionRegistry.applyShellCwd(&merged, pids: pids)
            return merged
        }.value
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
    private nonisolated static func applyShellCwd(_ merged: inout [Session], pids: [Int: String]) {
        for (pid, key) in pids {
            guard let i = merged.firstIndex(where: { $0.id == key && $0.isShell }), let dir = SessionRegistry.cwd(pid: pid_t(pid)) else { continue }
            merged[i].cwd = dir
        }
    }

    private func fillFirstPrompts(_ merged: inout [Session]) async {
        // Shell- und Remote-Sessions haben nie ein Transcript, sonst würde jeder Poll erneut vergeblich scannen.
        let missing = merged.filter { $0.name.isEmpty && !$0.isShell && !$0.isRemote && firstPrompts[$0.sessionId] == nil }.map(\.sessionId)
        if !missing.isEmpty {
            let configDir = cli.configDir
            let found = await Task.detached { missing.reduce(into: [String: String]()) { r, id in
                guard let p = Transcript.path(sessionId: id, configDir: configDir) else { return }
                // Kein Treffer trotz vorhandenem Transcript (nur Slash-Commands, oder die Eingabe liegt hinter dem
                // gelesenen Kopf): als Sentinel merken, sonst läse jeder Poll dieselben 256 KB erneut ein.
                r[id] = Transcript.firstPrompt(path: p) ?? ""
            } }.value
            firstPrompts.merge(found) { a, _ in a }
        }
        for i in merged.indices { merged[i].firstPrompt = firstPrompts[merged[i].sessionId].flatMap { $0.isEmpty ? nil : $0 } }
    }

    /// Liest nur gewachsene Transcripts neu und leitet daraus die Nachrichtenzeile ab (falls eingeschaltet).
    private func refreshTranscripts(_ merged: [Session]) async -> [String: String] {
        // Shell- und Remote-Sessions haben nie ein Transcript, sonst würde jeder Poll erneut vergeblich den ganzen
        // Projektordner scannen (siehe `Transcript.path`).
        let ids = merged.filter { !$0.isShell && !$0.isRemote }.map(\.sessionId), cache = transcripts, wantText = Settings.showLastMessage, configDir = cli.configDir
        transcripts = await Task.detached { Transcript.refresh(ids, configDir: configDir, cache: cache, wantText: wantText) }.value
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
            // Nur neu laden, wenn der Kandidat von der gecachten Liste nicht mehr abgedeckt ist: nur dann kann
            // ein Worktree entstanden oder verschwunden sein. Mehrere Sessions im selben `cwd` teilen sich den Cache.
            let covered = worktreeCache[cwd].map { Worktree.match(top, in: $0) != nil } ?? false
            if !covered { worktreeCache[cwd] = await Worktree.list(at: cwd) }
            guard let list = worktreeCache[cwd] else { continue }
            guard let active = Worktree.active(candidates: candidates, in: list) else { merged[i].activeWorktree = nil; continue }
            merged[i].activeWorktree = active.path
            merged[i].branch = active.branch ?? merged[i].branch
        }
    }

    /// Live-Werte nur über die pid des eigenen Prozesses: eine fremde Session mit derselben sessionId
    /// (z. B. der gestoppte Hintergrund-Eintrag) ist nicht diese Kachel, und `/clear` wechselt die sessionId.
    /// `nonisolated`: liest nur `.git/HEAD` (`Git.branch`), läuft in `pollNow` abseits des Main-Threads.
    nonisolated static func merge(_ sessions: [Session], agents: [Agent], pids: [Int: String]) -> [Session] {
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
    nonisolated static func cwd(pid: pid_t) -> String? {
        var info = proc_vnodepathinfo()
        let size = Int32(MemoryLayout<proc_vnodepathinfo>.size)
        guard proc_pidinfo(pid, PROC_PIDVNODEPATHINFO, 0, &info, size) == size else { return nil }
        let path = withUnsafeBytes(of: &info.pvi_cdir.vip_path) { String(cString: $0.bindMemory(to: CChar.self).baseAddress!) }
        return path.isEmpty ? nil : path
    }

    private func save() {
        guard !readOnly else {
            Self.log.error("sessions.json hat eine neuere Schema-Version, wird nicht überschrieben")
            return
        }
        JSONFile.saveArray(sessions, to: url, version: Self.schemaVersion) { [weak self] msg in self?.lastError = msg }
    }
}

private extension Session {
    var stored: [String] { [id, cwd, String(startedAt), sessionId, name, customName ?? ""] }
}

/// Eintrag in `closed.json`: die Session, wie sie beim Schließen zuletzt aussah, plus Zeitpunkt.
struct ClosedSession: Codable, Sendable {
    var session: Session
    var closedAt: Double
}
