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
        try #"{"pid":\#(pid),"sessionId":"alt","cwd":"/p","startedAt":9999999999999,"kind":"interactive","name":"Aus Datei","status":"busy"}"#
            .write(to: dir.appendingPathComponent("sessions/\(pid).json"), atomically: true, encoding: .utf8)
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: ["CLAUDE_CONFIG_DIR": dir.path])
        let r = SessionRegistry(cli: cli, url: dir.appendingPathComponent("sessions.json"))
        r.add(Session(id: "s1", cwd: dir.path, startedAt: 1, sessionId: "s1", name: ""))
        var running = true
        r.pids = { running ? [pid: "s1"] : [:] }

        await r.pollNow()
        XCTAssertEqual(r.sessions[0].status, .running)
        XCTAssertEqual(r.sessions[0].sessionId, "alt")

        r.report("s1", state: "waiting", sessionId: "neu", title: "Vom Hook", waitingFor: "Bash", message: "Antwort", firstPrompt: "Erste Frage")
        let s = r.sessions[0]
        XCTAssertEqual(s.status, .waiting)
        XCTAssertEqual(s.waitingFor, "Bash")
        XCTAssertEqual(s.sessionId, "neu")
        XCTAssertEqual(s.name, "Vom Hook")
        XCTAssertEqual(s.firstPrompt, "Erste Frage")
        XCTAssertEqual(s.pid, pid)

        await r.pollNow()
        XCTAssertEqual(r.sessions[0].status, .waiting, "die Datei (busy) darf die Meldung nicht überschreiben")
        XCTAssertEqual(r.sessions[0].sessionId, "neu")
        XCTAssertEqual(r.sessions[0].name, "Vom Hook")
        XCTAssertEqual(SessionRegistry(cli: cli, url: r.url).sessions[0].sessionId, "neu", "gemeldete sessionId ist gespeichert")

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
}
