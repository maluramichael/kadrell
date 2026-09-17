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
        let uuid = "0b7e1c2a-3f4d-4e5f-8a9b-0c1d2e3f4a5b"
        XCTAssertEqual(try ControlCommand.parse(["new", "-c", "/p", "--resume", uuid]),
                       .newSession(target: nil, dir: "/p", name: nil, detached: false, prompt: nil, resume: uuid))
        XCTAssertThrowsError(try ControlCommand.parse(["new", "--resume", uuid, "hallo"]))
        XCTAssertThrowsError(try ControlCommand.parse(["new", "--resume", "--dangerously-skip-permissions"]))
        XCTAssertThrowsError(try ControlCommand.parse(["new", "--resume", "../../x"]))
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

    /// Die Kachel ergibt sich aus der pid, nicht aus dem mitgeschickten Key: Terminal-Session oder Vorfahr.
    func testCallerFromProcessTree() {
        let terminals = [100: "k1", 200: "k2"]
        let parents: [pid_t: pid_t] = [300: 250, 250: 200, 400: 1, 500: 450]
        let parent: (pid_t) -> pid_t? = { parents[$0] }
        XCTAssertEqual(ControlCaller.session(pid: 300, terminals: terminals, sid: { $0 }, parent: parent), "k2")
        XCTAssertEqual(ControlCaller.session(pid: 400, terminals: terminals, sid: { $0 }, parent: parent), nil)
        XCTAssertEqual(ControlCaller.session(pid: 500, terminals: terminals, sid: { $0 }, parent: parent), nil)
        // Doppelt geforkt und an launchd gehängt: die Terminal-Session bleibt.
        XCTAssertEqual(ControlCaller.session(pid: 400, terminals: terminals, sid: { _ in 100 }, parent: parent), "k1")
        XCTAssertNotNil(ControlCaller.parent(getpid()))
    }

    func testCallerScope() {
        let g1 = Group(id: "g1", name: "A", color: "#fff", cwd: "/a", sessionIds: ["s1", "s2"])
        let g2 = Group(id: "g2", name: "B", color: "#fff", cwd: "/b", sessionIds: ["s3"])
        let groups = [g1, g2]
        XCTAssertTrue(ControlCaller.allowed(session: "s3", groups: groups, caller: nil, othersAllowed: false))
        XCTAssertTrue(ControlCaller.allowed(session: "s3", groups: groups, caller: "s1", othersAllowed: true))
        XCTAssertTrue(ControlCaller.allowed(session: "s2", groups: groups, caller: "s1", othersAllowed: false))
        XCTAssertTrue(ControlCaller.allowed(session: "s9", groups: groups, caller: "s9", othersAllowed: false))
        XCTAssertFalse(ControlCaller.allowed(session: "s3", groups: groups, caller: "s1", othersAllowed: false))
        XCTAssertTrue(ControlCaller.allowed(group: g1, caller: "s1", othersAllowed: false))
        XCTAssertFalse(ControlCaller.allowed(group: g2, caller: "s1", othersAllowed: false))
        XCTAssertTrue(ControlCaller.allowed(group: g2, caller: nil, othersAllowed: false))
    }

    func testPromptAfterDoubleDash() {
        XCTAssertEqual(ClaudeCLI.promptArgs("--dangerously-skip-permissions"), ["--", "--dangerously-skip-permissions"])
    }

    func testHookTrust() throws {
        let path = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-hook-\(UUID().uuidString)").path
        FileManager.default.createFile(atPath: path, contents: Data("#!/bin/sh\n".utf8))
        defer { try? FileManager.default.removeItem(atPath: path) }
        XCTAssertEqual(chmod(path, 0o755), 0)
        XCTAssertTrue(Hooks.trusted(path))
        XCTAssertEqual(chmod(path, 0o775), 0)
        XCTAssertFalse(Hooks.trusted(path))
        XCTAssertEqual(chmod(path, 0o757), 0)
        XCTAssertFalse(Hooks.trusted(path))
        XCTAssertFalse(Hooks.trusted("/nonexistent/hook"))
        XCTAssertFalse(Hooks.trusted("/bin/sh"))
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
        nonisolated(unsafe) var caller: String?, stranger: String?
        let server = ControlServer(path: path) { req, pid in
            seen = req
            // Der Client ist ein Kind dieses Testprozesses und wartet noch auf die Antwort.
            caller = ControlCaller.session(pid: pid, terminals: [Int(getpid()): "test"])
            stranger = ControlCaller.session(pid: pid, terminals: [99_999_999: "fremd"])
            return req.argv == ["ls"] ? .ok("hallo") : .fail("nein")
        }
        try server.start()
        defer { server.stop() }
        var attrs = stat()
        XCTAssertEqual(stat(path, &attrs), 0)
        XCTAssertEqual(attrs.st_mode & 0o777, 0o600)

        let bin = try XCTUnwrap(Bundle.main.executablePath)
        let env = ["KADRELL_SOCKET": path, "KADRELL_SESSION_KEY": "k1", "HOME": NSHomeDirectory()]
        let ok = try await ProcessRunner.run(bin, ["ls"], environment: env, cwd: "/tmp")
        XCTAssertEqual(ok.status, 0)
        XCTAssertEqual(ok.output, "hallo\n")
        XCTAssertEqual(seen?.caller, "k1")
        XCTAssertEqual(caller, "test")
        XCTAssertNil(stranger)
        XCTAssertEqual(seen?.cwd, "/private/tmp")

        let bad = try await ProcessRunner.run(bin, ["kill"], environment: env, cwd: "/tmp")
        XCTAssertEqual(bad.status, 1)
        XCTAssertEqual(bad.output, "kadrell: nein\n")

        let help = try await ProcessRunner.run(bin, ["help"], environment: env, cwd: "/tmp")
        XCTAssertEqual(help.status, 0)
        XCTAssertTrue(help.output.contains("kadrell new-group"))
    }

    /// `ControlClient.run` wartet weiter, solange die Antwort `startingStatus` trägt (`boot()` läuft noch),
    /// statt sie wie einen echten Fehler zu behandeln.
    func testClientWaitsWhileStarting() async throws {
        let path = "/tmp/kadrell-test-boot-\(getpid()).sock"
        nonisolated(unsafe) var calls = 0
        let server = ControlServer(path: path) { _, _ in
            calls += 1
            return calls < 2 ? ControlResponse(status: ControlResponse.startingStatus, stderr: "kadrell: Kadrell startet noch\n") : .ok("hallo")
        }
        try server.start()
        defer { server.stop() }
        let bin = try XCTUnwrap(Bundle.main.executablePath)
        let env = ["KADRELL_SOCKET": path, "HOME": NSHomeDirectory()]
        let ok = try await ProcessRunner.run(bin, ["ls"], environment: env, cwd: "/tmp")
        XCTAssertEqual(ok.status, 0)
        XCTAssertEqual(ok.output, "hallo\n")
    }

    /// Zwei `kadrell send` nacheinander (wie in einem Orchestrierungs-Skript): die Antwort auf den ersten
    /// Aufruf kommt erst, wenn dessen Handler ganz fertig ist, der zweite startet also nie hinein.
    func testHandlerRunsSequentiallyAcrossCalls() async throws {
        let path = "/tmp/kadrell-test-seq-\(getpid()).sock"
        nonisolated(unsafe) var order: [String] = []
        let server = ControlServer(path: path) { req, _ in
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
        _ = try await ProcessRunner.run(bin, ["send", "foo"], environment: env, cwd: "/tmp")
        _ = try await ProcessRunner.run(bin, ["send", "bar"], environment: env, cwd: "/tmp")
        XCTAssertEqual(order, ["foo start", "foo end", "bar start", "bar end"])
    }
}
