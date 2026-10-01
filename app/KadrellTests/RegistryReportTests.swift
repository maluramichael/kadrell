import XCTest
@testable import Kadrell

/// `kadrell status` aus einem Hook: die Meldung gilt, der Poll liest die Session-Datei dieser Kachel nicht mehr,
/// bis ihr Prozess endet. Danach greift wieder der alte Weg über `Agent.local`.
@MainActor
final class RegistryReportTests: XCTestCase {
    func testReportedSessionIsNotPolledUntilProcessEnds() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-report-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("sessions"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let pid = Int(ProcessInfo.processInfo.processIdentifier)
        let alt = UUID().uuidString.lowercased(), neu = UUID().uuidString.lowercased()
        try #"{"pid":\#(pid),"sessionId":"\#(alt)","cwd":"/p","startedAt":9999999999999,"kind":"interactive","name":"Aus Datei","status":"busy"}"#
            .write(to: dir.appendingPathComponent("sessions/\(pid).json"), atomically: true, encoding: .utf8)
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: ["CLAUDE_CONFIG_DIR": dir.path])
        let r = SessionRegistry(cli: cli, url: dir.appendingPathComponent("sessions.json"))
        r.add(Session(id: "s1", cwd: dir.path, startedAt: 1, sessionId: "s1", name: ""))
        var running = true
        r.pids = { running ? [pid: "s1"] : [:] }

        await r.pollNow()
        XCTAssertEqual(r.sessions[0].status, .running)
        XCTAssertEqual(r.sessions[0].sessionId, alt)

        r.report("s1", state: "waiting", sessionId: neu, title: "Vom Hook", waitingFor: "Bash", message: "Antwort", firstPrompt: "Erste Frage")
        let s = r.sessions[0]
        XCTAssertEqual(s.status, .waiting)
        XCTAssertEqual(s.waitingFor, "Bash")
        XCTAssertEqual(s.sessionId, neu)
        XCTAssertEqual(s.name, "Vom Hook")
        XCTAssertEqual(s.firstPrompt, "Erste Frage")
        XCTAssertEqual(s.pid, pid)

        await r.pollNow()
        XCTAssertEqual(r.sessions[0].status, .waiting, "die Datei (busy) darf die Meldung nicht überschreiben")
        XCTAssertEqual(r.sessions[0].sessionId, neu)
        XCTAssertEqual(r.sessions[0].name, "Vom Hook")
        XCTAssertEqual(SessionRegistry(cli: cli, url: r.url).sessions[0].sessionId, neu, "gemeldete sessionId ist gespeichert")

        r.report("s1", state: "idle", sessionId: nil, title: nil, waitingFor: nil, message: nil, firstPrompt: nil)
        XCTAssertEqual(r.sessions[0].status, .idle)
        XCTAssertNil(r.sessions[0].waitingFor)

        running = false
        await r.pollNow()
        XCTAssertEqual(r.sessions[0].status, .idle)
        XCTAssertNil(r.sessions[0].pid)
        running = true
        await r.pollNow()
        XCTAssertEqual(r.sessions[0].status, .running, "nach dem Prozessende gilt wieder die Datei")
    }

    func testReportUnknownKeyIsIgnored() {
        let r = SessionRegistry(cli: ClaudeCLI(binary: "/usr/bin/false", environment: [:]),
                                url: FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-report-\(UUID().uuidString)/sessions.json"))
        r.report("weg", state: "idle", sessionId: nil, title: nil, waitingFor: nil, message: nil, firstPrompt: nil)
        XCTAssertTrue(r.sessions.isEmpty)
    }

    private func makeRegistry(_ dir: URL) -> SessionRegistry {
        SessionRegistry(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ["CLAUDE_CONFIG_DIR": dir.path]),
                        url: dir.appendingPathComponent("sessions.json"))
    }

    private func tempDir() throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-report-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("sessions"), withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        return dir
    }

    /// Leere oder kaputte sessionId aus dem Hook lässt die alte UUID stehen.
    func testReportKeepsValidSessionIdOnInvalidValue() throws {
        let r = makeRegistry(try tempDir())
        let id = UUID().uuidString.lowercased()
        r.add(Session(id: "s1", cwd: "/p", startedAt: 1, sessionId: id, name: ""))
        for bad in ["", "kaputt"] {
            r.report("s1", state: "idle", sessionId: bad, title: nil, waitingFor: nil, message: nil, firstPrompt: nil)
            XCTAssertEqual(r.sessions[0].sessionId, id, bad)
        }
    }

    /// Agent ohne gültige sessionId (Datei halb geschrieben, Feld fehlt): alte UUID bleibt.
    func testMergeKeepsValidSessionIdOnInvalidAgent() throws {
        let id = UUID().uuidString.lowercased()
        let stored = [Session(id: "s1", cwd: "/p", startedAt: 1, sessionId: id, name: "")]
        for bad in ["", "kaputt"] {
            let agents = try JSONDecoder().decode([Agent].self, from: Data(#"[{"pid":11,"cwd":"/p","kind":"interactive","startedAt":5,"sessionId":"\#(bad)","name":"x","status":"busy"}]"#.utf8))
            let merged = SessionRegistry.merge(stored, agents: agents, pids: [11: "s1"])
            XCTAssertEqual(merged[0].sessionId, id, bad)
            XCTAssertEqual(merged[0].pid, 11)
        }
        let neu = UUID().uuidString
        let agents = try JSONDecoder().decode([Agent].self, from: Data(#"[{"pid":11,"cwd":"/p","kind":"interactive","startedAt":5,"sessionId":"\#(neu)","name":"x","status":"busy"}]"#.utf8))
        XCTAssertEqual(SessionRegistry.merge(stored, agents: agents, pids: [11: "s1"])[0].sessionId, neu)
    }

    /// Gleichzeitige `pollNow` überlappen nie, Wartende teilen sich höchstens einen Folgelauf.
    func testConcurrentPollsDoNotOverlap() async throws {
        let r = makeRegistry(try tempDir())
        r.add(Session(id: "s1", cwd: "/p", startedAt: 1, sessionId: UUID().uuidString, name: ""))
        async let a: Void = r.pollNow()
        async let b: Void = r.pollNow()
        async let c: Void = r.pollNow()
        _ = await (a, b, c)
        XCTAssertEqual(r.maxPollsInFlight, 1)
        XCTAssertLessThanOrEqual(r.pollRuns, 2)
        XCTAssertGreaterThanOrEqual(r.pollRuns, 1)
        await r.pollNow()
        XCTAssertEqual(r.maxPollsInFlight, 1)
    }

    /// Gemeldete Session: der zweite Poll liest ihr Transcript nicht neu, der Cache-Eintrag muss trotzdem bleiben.
    func testTranscriptCacheSurvivesPollsForReportedSession() async throws {
        let dir = try tempDir()
        let id = UUID().uuidString.lowercased()
        let project = dir.appendingPathComponent("projects/-p")
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        try #"{"type":"user","message":{"role":"user","content":"Hallo"}}"#.appending("\n")
            .write(to: project.appendingPathComponent("\(id).jsonl"), atomically: true, encoding: .utf8)
        let r = makeRegistry(dir)
        r.add(Session(id: "s1", cwd: dir.path, startedAt: 1, sessionId: id, name: "x"))
        r.pids = { [Int(ProcessInfo.processInfo.processIdentifier): "s1"] }
        r.report("s1", state: "idle", sessionId: nil, title: nil, waitingFor: nil, message: nil, firstPrompt: nil)
        await r.pollNow()
        XCTAssertNotNil(r.transcripts[id])
        await r.pollNow()
        XCTAssertNotNil(r.transcripts[id], "nicht neu gelesen heißt nicht vergessen")
    }

    /// Speicherfehler (hier: Zielordner ist eine Datei) landet in `storageError`, `clearError` lässt ihn stehen.
    func testStorageErrorIsSeparateFromLastError() throws {
        let dir = try tempDir()
        let blocker = dir.appendingPathComponent("blocker")
        try Data().write(to: blocker)
        let r = SessionRegistry(cli: ClaudeCLI(binary: "/usr/bin/false", environment: [:]), url: blocker.appendingPathComponent("sessions.json"))
        var changes = 0
        r.onChange = { _ in changes += 1 }
        r.add(Session(id: "s1", cwd: "/p", startedAt: 1, sessionId: UUID().uuidString, name: ""))
        XCTAssertNotNil(r.storageError)
        XCTAssertNil(r.lastError)
        XCTAssertGreaterThanOrEqual(changes, 1)
        r.fail("CLI weg")
        r.clearError()
        XCTAssertNil(r.lastError)
        XCTAssertNotNil(r.storageError)
        r.clearStorageError()
        XCTAssertNil(r.storageError)
    }

    /// `takeClosed` nimmt den Eintrag aus closed.json und speichert sofort, unbekannte Id liefert nil.
    func testTakeClosedRemovesAndSaves() throws {
        let dir = try tempDir()
        let r = makeRegistry(dir)
        let id = UUID().uuidString.lowercased()
        r.add(Session(id: id, cwd: "/p", startedAt: 1, sessionId: id, name: ""))
        r.remove([id])
        XCTAssertEqual(r.closed.map(\.session.sessionId), [id])
        XCTAssertNil(r.takeClosed(sessionId: "unbekannt"))
        XCTAssertEqual(r.closed.count, 1)
        XCTAssertEqual(r.takeClosed(sessionId: id)?.session.id, id)
        XCTAssertTrue(r.closed.isEmpty)
        XCTAssertTrue(makeRegistry(dir).closed.isEmpty, "gespeichert")
        XCTAssertNil(r.takeClosed(sessionId: id))
    }

    /// Wiederherstellbar: nur Claude-Sessions mit gültiger sessionId, die nicht schon wieder laufen, neueste zuerst, höchstens `limit`.
    func testRestorableFiltersAndLimits() {
        func closed(_ s: Session) -> ClosedSession { ClosedSession(session: s, closedAt: 0) }
        let a = UUID().uuidString, b = UUID().uuidString, running = UUID().uuidString
        var remote = Session(id: "ssh-x", cwd: "/p", startedAt: 1, sessionId: UUID().uuidString, name: "")
        remote.host = "h"
        let list = [
            closed(Session(id: "a", cwd: "/p", startedAt: 1, sessionId: a, name: "")),
            closed(Session(id: Session.newShellId(), cwd: "/p", startedAt: 1, sessionId: UUID().uuidString, name: "")),
            closed(remote),
            closed(Session(id: "kaputt", cwd: "/p", startedAt: 1, sessionId: "kaputt", name: "")),
            closed(Session(id: "r", cwd: "/p", startedAt: 1, sessionId: running, name: "")),
            closed(Session(id: "a2", cwd: "/p", startedAt: 1, sessionId: a, name: "")),
            closed(Session(id: "b", cwd: "/p", startedAt: 1, sessionId: b, name: "")),
        ]
        XCTAssertEqual(SessionRegistry.restorable(list, running: [running]).map(\.session.id), ["a", "b"])
        XCTAssertEqual(SessionRegistry.restorable(list, running: [], limit: 1).map(\.session.id), ["a"])
        let many = (0..<15).map { _ in closed(Session(id: UUID().uuidString, cwd: "/p", startedAt: 1, sessionId: UUID().uuidString, name: "")) }
        XCTAssertEqual(SessionRegistry.restorable(many, running: []).map(\.session.id), many.prefix(10).map(\.session.id))
    }
}
