import XCTest
@testable import Kadrell

@MainActor
final class ControlTests: XCTestCase {
    func testParse() throws {
        XCTAssertEqual(try ControlCommand.parse([]), .help)
        XCTAssertEqual(try ControlCommand.parse(["ls", "--json"]), .list(json: true))
        XCTAssertEqual(try ControlCommand.parse(["new-group", "~/p", "--name", "API", "--color", "FAB387"]),
                       .newGroup(dir: "~/p", name: "API", color: "#fab387"))
        XCTAssertEqual(try ControlCommand.parse(["new", "-d", "-t", "api", "fix", "the", "tests"]),
                       .newSession(target: "api", dir: nil, name: nil, detached: true, prompt: "fix the tests"))
        XCTAssertEqual(try ControlCommand.parse(["new", "--", "-x"]),
                       .newSession(target: nil, dir: nil, name: nil, detached: false, prompt: "-x"))
        XCTAssertEqual(try ControlCommand.parse(["new", "-c", "/p", "--resume", "abc"]),
                       .newSession(target: nil, dir: "/p", name: nil, detached: false, prompt: nil, resume: "abc"))
        XCTAssertThrowsError(try ControlCommand.parse(["new", "--resume", "abc", "hallo"]))
        XCTAssertEqual(try ControlCommand.parse(["set-group", "--favorite", "off"]), .setGroup(target: nil, name: nil, color: nil, favorite: false))
        XCTAssertEqual(try ControlCommand.parse(["send", "-t", "ab", "hallo", "welt"]), .send(target: "ab", text: "hallo welt", enter: true, keys: false))
        XCTAssertEqual(try ControlCommand.parse(["send-keys", "-k", "C-c", "Enter"]), .send(target: nil, text: "C-c Enter", enter: false, keys: true))
        XCTAssertEqual(try ControlCommand.parse(["kill"]), .killSession(target: nil))
        XCTAssertEqual(try ControlCommand.parse(["move", "-t", "44a3", "acme skills"]), .move(target: "44a3", group: "acme skills"))
        XCTAssertThrowsError(try ControlCommand.parse(["move", "-t", "44a3"]))
        XCTAssertEqual(try ControlCommand.parse(["layout", "stack"]), .layout(.stack))
    }

    func testParseErrors() {
        for argv in [["frobnicate"], ["ls", "--jsn"], ["new-group"], ["new", "-t"], ["layout", "tiles"],
                     ["set-group", "--favorite", "ja"], ["new-group", "/p", "--color", "red"], ["send", "-k", "Hyper"], ["send"]] {
            XCTAssertThrowsError(try ControlCommand.parse(argv), argv.joined(separator: " "))
        }
    }

    func testResolveSession() throws {
        let a = Session(id: "aaaa1111-0000", cwd: "/p", startedAt: 0, sessionId: "aaaa1111-0000", name: "Refactor")
        let b = Session(id: "aaaa2222-0000", cwd: "/p", startedAt: 0, sessionId: "cccc", name: "Tests")
        let all = [a, b]
        XCTAssertEqual(try ControlTarget.session("aaaa2", sessions: all, caller: nil, focused: nil).id, b.id)
        XCTAssertEqual(try ControlTarget.session("cccc", sessions: all, caller: nil, focused: nil).id, b.id)
        XCTAssertEqual(try ControlTarget.session("refactor", sessions: all, caller: nil, focused: nil).id, a.id)
        XCTAssertThrowsError(try ControlTarget.session("aaaa", sessions: all, caller: nil, focused: nil))
        XCTAssertThrowsError(try ControlTarget.session("zzz", sessions: all, caller: nil, focused: nil))
        XCTAssertEqual(try ControlTarget.session(nil, sessions: all, caller: b.id, focused: a.id).id, b.id)
        XCTAssertEqual(try ControlTarget.session(nil, sessions: all, caller: "weg", focused: a.id).id, a.id)
        XCTAssertThrowsError(try ControlTarget.session(nil, sessions: all, caller: nil, focused: nil))
    }

    func testResolveGroupAndEither() throws {
        let s = Session(id: "s1", cwd: "/p", startedAt: 0, sessionId: "s1", name: "api")
        let g1 = Group(id: "g1aa", name: "API", color: "#fff", cwd: "/p", sessionIds: ["s1"])
        let g2 = Group(id: "g2bb", name: "Web", color: "#fff", cwd: "/w", sessionIds: [])
        XCTAssertEqual(try ControlTarget.group(nil, groups: [g1, g2], sessions: [s], caller: "s1", focused: nil).id, "g1aa")
        XCTAssertEqual(try ControlTarget.group("web", groups: [g1, g2], sessions: [s], caller: nil, focused: nil).id, "g2bb")
        guard case .group(let g) = try ControlTarget.sessionOrGroup("g2", groups: [g1, g2], sessions: [s], caller: nil, focused: nil) else { return XCTFail() }
        XCTAssertEqual(g.id, "g2bb")
        // „api“ ist Session-Titel und Gruppenname zugleich.
        XCTAssertThrowsError(try ControlTarget.sessionOrGroup("api", groups: [g1, g2], sessions: [s], caller: nil, focused: nil))
    }

    func testPath() {
        XCTAssertEqual(ControlTarget.path("../b", cwd: "/x/a"), "/x/b")
        XCTAssertEqual(ControlTarget.path("~/p", cwd: "/x"), NSHomeDirectory() + "/p")
        XCTAssertEqual(ControlTarget.path("/abs", cwd: "/x"), "/abs")
    }

    /// Das echte Binary als Kommandozeile gegen einen Server auf einem eigenen Socket, nicht den der laufenden App.
    func testClientBinaryAgainstServer() async throws {
        let path = "/tmp/kadrell-test-\(getpid()).sock"
        nonisolated(unsafe) var seen: ControlRequest?
        let server = ControlServer(path: path) { req in
            seen = req
            return req.argv == ["ls"] ? .ok("hallo") : .fail("nein")
        }
        try server.start()
        defer { server.stop() }
        var attrs = stat()
        XCTAssertEqual(stat(path, &attrs), 0)
        XCTAssertEqual(attrs.st_mode & 0o777, 0o600)

        let bin = try XCTUnwrap(Bundle.main.executablePath)
        let env = ["KADRELL_SOCKET": path, "KADRELL_SESSION_KEY": "k1", "HOME": NSHomeDirectory()]
        let ok = try await ClaudeCLI.runRaw(bin, ["ls"], environment: env, cwd: "/tmp")
        XCTAssertEqual(ok.status, 0)
        XCTAssertEqual(ok.output, "hallo\n")
        XCTAssertEqual(seen?.caller, "k1")
        XCTAssertEqual(seen?.cwd, "/private/tmp")

        let bad = try await ClaudeCLI.runRaw(bin, ["kill"], environment: env, cwd: "/tmp")
        XCTAssertEqual(bad.status, 1)
        XCTAssertEqual(bad.output, "kadrell: nein\n")

        let help = try await ClaudeCLI.runRaw(bin, ["help"], environment: env, cwd: "/tmp")
        XCTAssertEqual(help.status, 0)
        XCTAssertTrue(help.output.contains("kadrell new-group"))
    }

    /// `ControlClient.run` wartet weiter, solange die Antwort `startingStatus` trägt (`boot()` läuft noch),
    /// statt sie wie einen echten Fehler zu behandeln.
    func testClientWaitsWhileStarting() async throws {
        let path = "/tmp/kadrell-test-boot-\(getpid()).sock"
        nonisolated(unsafe) var calls = 0
        let server = ControlServer(path: path) { _ in
            calls += 1
            return calls < 2 ? ControlResponse(status: ControlResponse.startingStatus, stderr: "kadrell: Kadrell startet noch\n") : .ok("hallo")
        }
        try server.start()
        defer { server.stop() }
        let bin = try XCTUnwrap(Bundle.main.executablePath)
        let env = ["KADRELL_SOCKET": path, "HOME": NSHomeDirectory()]
        let ok = try await ClaudeCLI.runRaw(bin, ["ls"], environment: env, cwd: "/tmp")
        XCTAssertEqual(ok.status, 0)
        XCTAssertEqual(ok.output, "hallo\n")
    }

    /// Zwei `kadrell send` nacheinander (wie in einem Orchestrierungs-Skript): die Antwort auf den ersten
    /// Aufruf kommt erst, wenn dessen Handler ganz fertig ist, der zweite startet also nie hinein.
    func testHandlerRunsSequentiallyAcrossCalls() async throws {
        let path = "/tmp/kadrell-test-seq-\(getpid()).sock"
        nonisolated(unsafe) var order: [String] = []
        let server = ControlServer(path: path) { req in
            let tag = req.argv.last ?? "?"
            order.append("\(tag) start")
            try? await Task.sleep(for: .milliseconds(40))
            order.append("\(tag) end")
            return .ok()
        }
        try server.start()
        defer { server.stop() }
        let bin = try XCTUnwrap(Bundle.main.executablePath)
        let env = ["KADRELL_SOCKET": path, "HOME": NSHomeDirectory()]
        _ = try await ClaudeCLI.runRaw(bin, ["send", "foo"], environment: env, cwd: "/tmp")
        _ = try await ClaudeCLI.runRaw(bin, ["send", "bar"], environment: env, cwd: "/tmp")
        XCTAssertEqual(order, ["foo start", "foo end", "bar start", "bar end"])
    }
}
