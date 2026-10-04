import XCTest
@testable import Kadrell

/// `ExtensionProcess`: Start, Protokoll und Überwachung eines Extension-Helpers.
@MainActor
final class ExtensionProcessTests: XCTestCase {
    private enum Event: Equatable {
        case message(ExtensionMessage), stderr(String), exit(expected: Bool, reason: String)
    }

    private var events: [Event] = []
    private var procs: [ExtensionProcess] = []
    // Eigene Umgebung: die des Test-Hosts trägt die XCTest-Injektion (DYLD_*), die soll der Helper nicht erben.
    private let environment = ["HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]

    override func tearDown() async throws {
        procs.forEach { $0.stop() }
        await until(5) { self.exits.count >= self.procs.count }
        XCTAssertEqual(exits.count, procs.count, "onExit genau einmal pro Prozess")
        procs = []
        events = []
        try await super.tearDown()
    }

    private var messages: [ExtensionMessage] { events.compactMap { if case .message(let m) = $0 { m } else { nil } } }
    private var stderr: [String] { events.compactMap { if case .stderr(let s) = $0 { s } else { nil } } }
    private var exits: [(expected: Bool, reason: String)] {
        events.compactMap { if case .exit(let e, let r) = $0 { (e, r) } else { nil } }
    }

    @discardableResult
    private func until(_ seconds: TimeInterval, _ condition: () -> Bool) async -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while !condition(), Date() < deadline { try? await Task.sleep(for: .milliseconds(20)) }
        return condition()
    }

    private func launch(_ dir: URL, executable: String? = nil) throws -> ExtensionProcess {
        let p = executable.map { ExtensionProcess(dir: dir, executable: $0, environment: environment) }
            ?? ExtensionProcess(dir: dir, environment: environment)
        p.onMessage = { [unowned self] in events.append(.message($0)) }
        p.onStderr = { [unowned self] in events.append(.stderr($0)) }
        p.onExit = { [unowned self] in events.append(.exit(expected: $0, reason: $1)) }
        procs.append(p)
        try p.start(hello: .hello(api: 1, name: dir.lastPathComponent, dir: dir.path, storageDir: NSTemporaryDirectory(),
                                  config: [:], locale: "de", sessions: []))
        return p
    }

    private func launchLua(_ initLua: String) throws -> ExtensionProcess {
        try launch(try ExtFixture.make(name: "p", initLua: initLua))
    }

    /// Fremdes Programm statt des Helpers: ein Shell-Skript, das `ext-host <dir>` ignoriert.
    private func launchScript(_ body: String) throws -> ExtensionProcess {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-fake-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let script = dir.appendingPathComponent("fake.sh")
        try Data("#!/bin/sh\n\(body)\n".utf8).write(to: script)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: script.path)
        return try launch(dir, executable: script.path)
    }

    /// Sammelzeile „N Zeilen übersprungen“. Die genaue Zahl hängt vom Takt ab, geprüft wird das Muster
    /// (Zahl mit Tausendertrennern).
    private func isSkippedSummary(_ s: String) -> Bool {
        let parts = String(localized: "\(-1) Zeilen übersprungen", bundle: Bundle.app).components(separatedBy: "-1")
        let number = s.dropFirst(parts[0].count).dropLast(parts[1].count)
        return s.hasPrefix(parts[0]) && s.hasSuffix(parts[1]) && number.first?.isNumber == true
            && number.allSatisfy { $0.isNumber || $0.isPunctuation || $0.isWhitespace }
    }

    /// Startet die Lua-Extension, wartet auf `ready` und schickt `app.ready`.
    private func launchReady(_ initLua: String) async throws -> ExtensionProcess {
        let p = try launchLua(initLua)
        let ready = await until(3) { self.messages.contains(.ready) }
        XCTAssertTrue(ready)
        p.send(.event(name: "app.ready", data: .object([:])))
        return p
    }

    func testHelloDeliversReadyAndPanel() async throws {
        _ = try await launchReady(#"kadrell.on("app.ready", function() kadrell.panel.set{title = "A", children = {}} end)"#)
        let panel = await until(3) { self.messages.contains { if case .panel(let t) = $0 { t?["title"]?.string == "A" } else { false } } }
        XCTAssertTrue(panel, "\(events)")
        XCTAssertEqual(messages.first, .ready)
    }

    func testCrashReportsUnexpectedExit() async throws {
        _ = try launchLua(#"error("x")"#)
        await until(3) { !self.exits.isEmpty }
        XCTAssertEqual(exits.first?.expected, false)
        XCTAssertTrue(exits.first?.reason.contains("init.lua:1: x") == true, "\(events)")
    }

    func testInfiniteLoopIsKilledByPing() async throws {
        _ = try await launchReady(#"kadrell.on("app.ready", function() while true do end end)"#)
        await until(12) { !self.exits.isEmpty }
        // onExit kommt erst, wenn der Prozess beendet und eingesammelt ist.
        XCTAssertEqual(exits.first?.expected, false)
        XCTAssertEqual(exits.first?.reason, ExtensionProcess.Kill.noPong.reason)
        XCTAssertTrue(exits.first?.reason.contains("5 s") == true)
    }

    func testFloodIsKilled() async throws {
        _ = try await launchReady(#"kadrell.on("app.ready", function() for i = 1, 1000 do kadrell.status.set{text = tostring(i)} end end)"#)
        await until(3) { !self.exits.isEmpty }
        XCTAssertEqual(exits.first?.expected, false)
        XCTAssertEqual(exits.first?.reason, ExtensionProcess.Kill.flood.reason)
        XCTAssertLessThanOrEqual(messages.count, 51, "nach der Grenze kommt nichts mehr durch")
    }

    /// Log-Zeilen zählen nicht zur Flutgrenze, sie werden gedrosselt wie stderr: höchstens 50 pro Sekunde plus Sammelzeile.
    func testLogFloodIsThrottledNotKilled() async throws {
        let p = try await launchReady(#"kadrell.on("app.ready", function() for i = 1, 1000 do kadrell.log(i) end end)"#)
        try await Task.sleep(for: .milliseconds(1500))
        let logs = messages.filter { if case .log = $0 { true } else { false } }
        XCTAssertLessThanOrEqual(logs.count, 50)
        XCTAssertTrue(stderr.contains(where: isSkippedSummary), "\(stderr)")
        XCTAssertEqual(exits.count, 0)
        p.send(.ping(99))
        let pong = await until(3) { self.messages.contains(.pong(99)) }
        XCTAssertTrue(pong)
    }

    func testLogTextIsClipped() async throws {
        _ = try await launchReady(#"kadrell.on("app.ready", function() kadrell.log(string.rep("a", 2000)) end)"#)
        await until(3) { self.messages.count >= 2 }
        XCTAssertEqual(messages.last, .log(level: "info", text: String(repeating: "a", count: 500)))
    }

    func testGarbageLineIsIgnored() async throws {
        _ = try launchScript(#"echo kaputt; echo '{"t":"ready"}'; exec sleep 30"#)
        await until(3) { self.messages.contains(.ready) }
        try await Task.sleep(for: .milliseconds(300))
        XCTAssertEqual(events, [.stderr(String(localized: "ungültige Nachricht verworfen", bundle: Bundle.app)), .message(.ready)])
    }

    func testStderrLinesAreForwardedAndClipped() async throws {
        _ = try launchScript(#"echo eins >&2; head -c 2000 /dev/zero | tr '\0' b >&2; echo >&2; exec sleep 30"#)
        await until(3) { self.stderr.count >= 2 }
        XCTAssertEqual(stderr, ["eins", String(repeating: "b", count: 500)])
    }

    /// stderr ist gedrosselt: höchstens 50 Zeilen pro Sekunde, der Rest als Sammelzeile, kein Kill.
    func testStderrFloodIsThrottledNotKilled() async throws {
        _ = try await launchReady(#"kadrell.on("app.ready", function() for i = 1, 100000 do io.write(i, "\n") end end)"#)
        try await Task.sleep(for: .seconds(2))
        XCTAssertLessThan(stderr.count, 200)
        XCTAssertTrue(stderr.contains(where: isSkippedSummary), "\(stderr.suffix(3))")
        XCTAssertEqual(exits.count, 0)
    }

    /// Kein Erbe für fremde Kinder: SwiftTerm startet Sessions per forkpty und schließt nichts. Hielte eine Session
    /// das stdin-Schreibende des Helpers, sähe der Helper nach Kadrells Ende nie EOF. posix_spawn ohne
    /// close-Aktionen erbt jeden fd ohne FD_CLOEXEC; die Kontroll-Pipe zeigt, dass die Prüfung Geerbtes erkennt.
    func testPipesAreNotInheritedBySpawnedChildren() async throws {
        let p = try await launchReady("")
        let control = Pipe()
        let fds = p.parentDescriptors + [control.fileHandleForWriting.fileDescriptor]
        let out = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-fds-\(UUID().uuidString)")
        // `[ -e /dev/fd/N ]` öffnet nichts selbst, anders als `ls /dev/fd`, kann also keine Nummer neu belegen.
        let script = "for fd in \(fds.map(String.init).joined(separator: " ")); do [ -e /dev/fd/$fd ] && echo $fd; done; true"
        var actions: posix_spawn_file_actions_t?
        posix_spawn_file_actions_init(&actions)
        defer { posix_spawn_file_actions_destroy(&actions) }
        posix_spawn_file_actions_addopen(&actions, 1, out.path, O_WRONLY | O_CREAT | O_TRUNC, 0o644)
        let argv = ["/bin/sh", "-c", script].map { strdup($0) } + [nil]
        defer { argv.forEach { free($0) } }
        var pid: pid_t = 0, status: Int32 = 0
        XCTAssertEqual(posix_spawn(&pid, "/bin/sh", &actions, nil, argv, environ), 0)
        waitpid(pid, &status, 0)
        let seen = try String(contentsOf: out, encoding: .utf8).split(separator: "\n").map { Int32($0) }
        XCTAssertEqual(seen, [control.fileHandleForWriting.fileDescriptor], "nur die Kontroll-Pipe darf geerbt sein")
    }

    func testMissingReadyIsKilled() async throws {
        _ = try launchScript("exec sleep 30")
        await until(5) { !self.exits.isEmpty }
        XCTAssertEqual(exits.first?.expected, false)
        XCTAssertEqual(exits.first?.reason, ExtensionProcess.Kill.noReady.reason)
    }

    func testOverlongLineIsKilled() async throws {
        _ = try launchScript(#"echo '{"t":"ready"}'; head -c 1100000 /dev/zero | tr '\0' a; exec sleep 30"#)
        await until(3) { !self.exits.isEmpty }
        XCTAssertEqual(exits.first?.expected, false)
        XCTAssertEqual(exits.first?.reason, ExtensionProcess.Kill.longLine.reason)
    }

    /// Ein Helper, der stdin nicht liest: über 1 MB ungeschriebene Nachrichten gelten als Hänger, die App blockiert nicht.
    func testFullInputBufferIsKilled() async throws {
        let p = try launchScript(#"echo '{"t":"ready"}'; exec sleep 30"#)
        await until(3) { self.messages.contains(.ready) }
        let big = JSONValue.string(String(repeating: "x", count: 100_000))
        for _ in 0..<12 { p.send(.event(name: "big", data: big)) }
        await until(3) { !self.exits.isEmpty }
        XCTAssertEqual(exits.first?.expected, false)
        XCTAssertEqual(exits.first?.reason, ExtensionProcess.Kill.inputFull.reason)
    }

    func testStopIsGracefulAndThenHard() async throws {
        let p = try await launchReady(#"kadrell.on("app.ready", function() while true do end end)"#)
        try await Task.sleep(for: .milliseconds(200))
        let begin = Date()
        p.stop()
        await until(3) { !self.exits.isEmpty }
        XCTAssertLessThan(Date().timeIntervalSince(begin), 3)
        XCTAssertEqual(exits.first?.expected, true)
        // Der SIGKILL-Schritt nach 2 s darf kein zweites onExit auslösen.
        try await Task.sleep(for: .milliseconds(2500))
        XCTAssertEqual(exits.count, 1)
    }

    func testStopOfIdleHelperUsesShutdown() async throws {
        let p = try await launchReady("")
        let begin = Date()
        p.stop()
        await until(3) { !self.exits.isEmpty }
        XCTAssertLessThan(Date().timeIntervalSince(begin), 0.9, "vor dem SIGTERM nach 1 s")
        XCTAssertEqual(exits.first?.expected, true)
    }

    /// SIGTERM wird ignoriert, erst der SIGKILL nach 2 s beendet den Prozess.
    func testStopEscalatesToKill() async throws {
        let p = try launchScript(#"trap '' TERM; echo '{"t":"ready"}'; exec sleep 30"#)
        await until(3) { self.messages.contains(.ready) }
        let begin = Date()
        p.stop()
        await until(4) { !self.exits.isEmpty }
        XCTAssertGreaterThan(Date().timeIntervalSince(begin), 1.5)
        XCTAssertEqual(exits.first?.expected, true)
    }

    /// Review Focus 3: Kadrell endet, die Extension hängt in einer Schleife. Ohne stdin beendet sich der Helper selbst.
    func testParentGoneKillsLoopingHelper() throws {
        let dir = try ExtFixture.make(name: "loop", initLua: #"kadrell.on("app.ready", function() while true do end end)"#)
        let p = try HostPipe(dir: dir)
        p.send(["t": "hello", "api": 1, "name": "loop", "dir": dir.path, "storageDir": NSTemporaryDirectory(),
                "config": [:], "locale": "de", "sessions": []])
        XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "ready")
        p.send(["t": "event", "name": "app.ready", "data": [:]])
        usleep(200_000)
        p.closeStdin()
        XCTAssertTrue(p.waitForExit(timeout: 1))
    }
}
