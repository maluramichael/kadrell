import XCTest
@testable import Kadrell

@MainActor
final class PiSessionTests: XCTestCase {
    private let ref = LocalModelRef(provider: .ollama, name: "qwen3.6:35b-a3b-q4_K_M")

    private func piSession() -> Session {
        let id = Session.newPiId()
        return Session(id: id, cwd: "/p/kadrell", startedAt: 1, sessionId: id, name: "", model: ref.string)
    }

    func testPiIdCarriesPiSessionIdAndModel() {
        let s = piSession()
        XCTAssertTrue(s.isPi)
        XCTAssertFalse(s.isShell)
        XCTAssertNotNil(UUID(uuidString: s.piSessionId), "hinter dem Präfix steht die --session-id von Pi")
        XCTAssertEqual(s.localModel, ref)
        XCTAssertFalse(s.hasTranscript, "Pi-Sessions haben kein Claude-Transcript")
    }

    func testModelSurvivesSaving() throws {
        let s = piSession()
        let back = try JSONDecoder().decode(Session.self, from: JSONEncoder().encode(s))
        XCTAssertEqual(back.model, ref.string)
    }

    func testOldSessionsWithoutModelStillLoad() throws {
        let json = #"{"id":"abc","cwd":"/p","startedAt":1,"sessionId":"abc","name":""}"#
        let s = try JSONDecoder().decode(Session.self, from: Data(json.utf8))
        XCTAssertNil(s.model)
        XCTAssertTrue(s.hasTranscript)
    }

    func testLaunchPassesModelAddressAndPrompt() {
        let s = piSession()
        let l = PiCLI.launch(binary: "/bin/pi", session: s, model: ref, baseURL: URL(string: "http://127.0.0.1:11434"), prompt: "Idee")
        XCTAssertEqual(l.args, ["--provider", "kadrell", "--model", ref.name, "--session-id", s.piSessionId, "Idee"])
        XCTAssertEqual(l.env["KADRELL_PI"], "1")
        XCTAssertEqual(l.env["KADRELL_PI_MODEL"], ref.name)
        XCTAssertEqual(l.env["KADRELL_PI_BASE_URL"], "http://127.0.0.1:11434/v1")
    }

    func testLaunchWithoutAddressOrPrompt() {
        let l = PiCLI.launch(binary: "/bin/pi", session: piSession(), model: ref, baseURL: nil, prompt: nil)
        XCTAssertEqual(l.args.count, 6, "ohne Prompt kein leeres Argument")
        XCTAssertNil(l.env["KADRELL_PI_BASE_URL"])
    }

    /// Aus Dock oder Finder gestartet fehlt `~/.local/bin` oft im PATH der Login-Shell: Kadrell findet `pi` dort trotzdem,
    /// `#!/usr/bin/env node` aber nicht den `node` daneben. Der Ordner von `pi` gehört deshalb vorne in den PATH.
    func testPathGetsBinaryFolderOnce() {
        XCTAssertEqual(PiCLI.path(for: "/Users/x/.local/bin/pi", in: "/usr/bin:/bin"), "/Users/x/.local/bin:/usr/bin:/bin")
        XCTAssertEqual(PiCLI.path(for: "/Users/x/.local/bin/pi", in: "/usr/bin:/Users/x/.local/bin"), "/usr/bin:/Users/x/.local/bin",
                       "schon im PATH: Reihenfolge bleibt")
        XCTAssertEqual(PiCLI.path(for: "/opt/pi/bin/pi", in: nil), "/opt/pi/bin")
    }

    func testPromptIsNeverReadAsOptionOrFile() {
        XCTAssertEqual(PiCLI.safePrompt("--help mir"), " --help mir")
        XCTAssertEqual(PiCLI.safePrompt("@datei.md lesen"), " @datei.md lesen")
        XCTAssertEqual(PiCLI.safePrompt("normale Idee"), "normale Idee")
    }

    func testBinaryFromPathOrLocalBin() throws {
        let home = FileManager.default.temporaryDirectory.appending(path: "pi-\(UUID().uuidString)").path
        defer { try? FileManager.default.removeItem(atPath: home) }
        XCTAssertNil(PiCLI.binary(environment: ["PATH": "/nirgends"], home: home))
        try FileManager.default.createDirectory(atPath: home + "/.local/bin", withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: home + "/.local/bin/pi", contents: Data("#!/bin/sh\n".utf8), attributes: [.posixPermissions: 0o755])
        XCTAssertEqual(PiCLI.binary(environment: ["PATH": "/nirgends"], home: home), home + "/.local/bin/pi")
    }

    func testExtensionShipsInBundleAndInstallsOnce() throws {
        let source = try XCTUnwrap(PiCLI.extensionSource, "kadrell.ts gehört ins App-Bundle")
        XCTAssertTrue(source.contains("KADRELL_PI"))
        let home = FileManager.default.temporaryDirectory.appending(path: "pi-\(UUID().uuidString)").path
        defer { try? FileManager.default.removeItem(atPath: home) }
        let file = home + "/.pi/agent/extensions/kadrell.ts"
        XCTAssertTrue(PiCLI.installExtension(source: "alt", home: home))
        XCTAssertTrue(PiCLI.installExtension(source: source, home: home))
        XCTAssertEqual(try String(contentsOfFile: file, encoding: .utf8), source, "veraltete Fassung wird ersetzt")
        let written = try FileManager.default.attributesOfItem(atPath: file)[.modificationDate] as? Date
        XCTAssertTrue(PiCLI.installExtension(source: source, home: home))
        XCTAssertEqual(try FileManager.default.attributesOfItem(atPath: file)[.modificationDate] as? Date, written, "gleiche Fassung bleibt liegen")
    }
}
