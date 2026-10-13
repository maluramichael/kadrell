import XCTest
@testable import Kadrell

/// Harness je Kachel (`AgentKind`) und Pi-Start: alte `sessions.json` bleibt Claude, Pi bekommt seine eigenen Argumente.
final class AgentKindTests: XCTestCase {
    func testSessionWithoutHarnessDecodesAsClaude() throws {
        let s = try JSONDecoder().decode(Session.self, from: Data(#"{"id":"a","cwd":"/p","startedAt":1,"sessionId":"a","name":"x"}"#.utf8))
        XCTAssertNil(s.harness)
        XCTAssertEqual(s.kind, .claude)
        XCTAssertTrue(s.usesClaudeData)
    }

    func testPiSessionRoundTripsAndSkipsClaudeData() throws {
        var s = Session(id: "a", cwd: "/p", startedAt: 1, sessionId: "a", name: "")
        s.harness = .pi
        let back = try JSONDecoder().decode(Session.self, from: JSONEncoder().encode(s))
        XCTAssertEqual(back.kind, .pi)
        XCTAssertEqual(back.agent, .pi)
        XCTAssertFalse(back.usesClaudeData)
    }

    /// Claude-Sessions schreiben kein `harness`, damit ältere Versionen dieselbe Datei lesen.
    func testClaudeSessionEncodesWithoutHarness() throws {
        let json = String(decoding: try JSONEncoder().encode(Session(id: "a", cwd: "/p", startedAt: 1, sessionId: "a", name: "")), as: UTF8.self)
        XCTAssertFalse(json.contains("harness"))
    }

    func testShellAndRemoteInheritNoAgent() {
        XCTAssertNil(Session(id: Session.newShellId(), cwd: "/p", startedAt: 1, sessionId: "x", name: "").agent)
        var remote = Session(id: "ssh-1", cwd: "/p", startedAt: 1, sessionId: "x", name: "")
        remote.host = "box"
        XCTAssertNil(remote.agent)
    }

    func testPiLaunchArgs() {
        XCTAssertEqual(PiCLI.sessionArgs(sessionId: "u1"), ["--session-id", "u1"])
        XCTAssertEqual(PiCLI.launchArgs(model: "", thinking: "", extensionPath: nil), [])
        XCTAssertEqual(PiCLI.launchArgs(model: "ollama/qwen3.8:27b-mlx", thinking: "high", extensionPath: "/x/kadrell-pi.js"),
                       ["--model", "ollama/qwen3.8:27b-mlx", "--thinking", "high", "-e", "/x/kadrell-pi.js"])
        XCTAssertEqual(PiCLI.promptArgs("--help"), ["--", "--help"])
    }

    /// Ausgabe von pi 1.2.0: Modell-Ids mit Leerzeichen, deshalb nach den Spalten der Kopfzeile geschnitten.
    func testParseModels() {
        let out = """
            provider        model                                   context  max-out  thinking  images
            llamacpp-local  Qwen3.8-27B-Heretic-RVN-Q8 (0bserverx)  262.1K   16.4K    yes       yes
            ollama          qwen3.8:27b-mlx                         262.1K   16.4K    yes       yes

            """
        XCTAssertEqual(PiCLI.parseModels(out), ["llamacpp-local/Qwen3.8-27B-Heretic-RVN-Q8 (0bserverx)", "ollama/qwen3.8:27b-mlx"])
        XCTAssertEqual(PiCLI.parseModels("No models available.\n"), [])
    }

    func testHasSessionFindsFileBySuffix() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("pi-sessions-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let dir = root.appendingPathComponent("--tmp-x--")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data().write(to: dir.appendingPathComponent("2026-10-13T07-33-09-609Z_abc-123.jsonl"))
        let env = ["PI_CODING_AGENT_SESSION_DIR": root.path]
        XCTAssertTrue(PiCLI.hasSession(sessionId: "abc-123", environment: env))
        XCTAssertFalse(PiCLI.hasSession(sessionId: "abc", environment: env))
    }
}

extension ControlTests {
    func testParseNewWithAgent() throws {
        XCTAssertEqual(try ControlCommand.parse(["new", "--agent", "pi", "-c", "/tmp"]),
                       .newSession(target: nil, dir: "/tmp", name: nil, detached: false, prompt: nil, agent: .pi))
        XCTAssertThrowsError(try ControlCommand.parse(["new", "--agent", "gpt"]))
        XCTAssertThrowsError(try ControlCommand.parse(["new", "--agent", "pi", "--host", "box"]))
    }

    func testParseStatusModel() throws {
        XCTAssertEqual(try ControlCommand.parse(["status", "idle", "--model", "ollama/q"]),
                       .status(target: nil, state: "idle", sessionId: nil, title: nil, waitingFor: nil, message: nil, firstPrompt: nil, model: "ollama/q"))
    }
}
