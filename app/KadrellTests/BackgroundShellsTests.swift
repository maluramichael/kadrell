import XCTest
@testable import Kadrell

/// Erkennung noch laufender Kommandos aus dem Bash-Tool: gegen echte Prozesse, nicht gegen eine Attrappe.
/// Der Test baut die Kette nach, die Claude Code aufmacht: ein Prozess startet eine Shell, die zuerst einen
/// Schnappschuss einliest (`BackgroundShells.marker`) und dann wartet.
final class BackgroundShellsTests: XCTestCase {
    private var processes: [Process] = []

    override func tearDown() {
        for p in processes where p.isRunning { p.terminate(); p.waitUntilExit() }
        processes = []
        super.tearDown()
    }

    private func spawn(_ command: String) throws -> Process {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/zsh")
        p.arguments = ["-c", command]
        try p.run()
        processes.append(p)
        return p
    }

    func testChildrenFindsOwnChild() throws {
        let p = try spawn("sleep 30")
        XCTAssertTrue(BackgroundShells.children(of: getpid()).contains(p.processIdentifier))
    }

    func testArgumentsContainCommand() throws {
        let p = try spawn("sleep 31")
        XCTAssertEqual(BackgroundShells.arguments(of: p.processIdentifier)?.contains("sleep 31"), true)
    }

    func testRunningSeesShellWithSnapshot() throws {
        let snapshot = NSTemporaryDirectory() + "shell-snapshots/snapshot-zsh-test.sh"
        try FileManager.default.createDirectory(atPath: (snapshot as NSString).deletingLastPathComponent,
                                                withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: snapshot, contents: Data())
        _ = try spawn("source \(snapshot) 2>/dev/null || true && sleep 30")
        XCTAssertEqual(BackgroundShells.running(pids: [Int(getpid())]), [Int(getpid())])
    }

    /// MCP-Server und Helfer wie `caffeinate` sind ebenfalls Kinder des Claude-Prozesses, zählen aber nicht.
    func testRunningIgnoresOtherChildren() throws {
        _ = try spawn("exec sleep 30")
        XCTAssertTrue(BackgroundShells.running(pids: [Int(getpid())]).isEmpty)
    }

    func testRunningIgnoresUnknownPid() {
        XCTAssertTrue(BackgroundShells.running(pids: [999_999]).isEmpty)
    }
}

/// Punktfarbe: blau, solange in einer wartenden Session noch ein Kommando läuft.
final class StatusDotColorTests: XCTestCase {
    private func session(status: String?, shell: Bool) -> Session {
        var s = Session(id: "a", cwd: "/tmp", startedAt: 0, sessionId: "a", name: "a")
        s.rawStatus = status
        s.hasRunningShell = shell
        return s
    }

    func testRunningShellPaintsShellColor() {
        XCTAssertEqual(Theme.dotColor(session(status: "idle", shell: true), attached: true, pulse: 1), Theme.shell)
    }

    func testRunningShellPulses() {
        XCTAssertEqual(Theme.dotColor(session(status: "idle", shell: true), attached: true, pulse: 0.5).alphaComponent, 0.5, accuracy: 0.001)
    }

    func testWorkingSessionKeepsRunningColor() {
        XCTAssertEqual(Theme.dotColor(session(status: "busy", shell: true), attached: true, pulse: 1), Theme.running)
    }

    func testIdleWithoutShellDoesNotPulse() {
        XCTAssertEqual(Theme.dotColor(session(status: "idle", shell: false), attached: true, pulse: 0.5).alphaComponent, 1, accuracy: 0.001)
    }

    func testDetachedStaysGrey() {
        XCTAssertEqual(Theme.dotColor(session(status: "idle", shell: true), attached: false, pulse: 1), Theme.detached)
    }
}

/// Verdrahtung: was `BackgroundShells` findet, steht nach dem Poll an der Session.
@MainActor
final class RunningShellWiringTests: XCTestCase {
    func testPollMarksSessionWithRunningShell() async throws {
        let fm = FileManager.default
        let dir = fm.temporaryDirectory.appendingPathComponent("kadrell-shells-\(UUID().uuidString)")
        try fm.createDirectory(at: dir.appendingPathComponent("sessions"), withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: dir) }
        let snapshot = dir.appendingPathComponent("shell-snapshots/snapshot-zsh-wiring.sh")
        try fm.createDirectory(at: snapshot.deletingLastPathComponent(), withIntermediateDirectories: true)
        fm.createFile(atPath: snapshot.path, contents: Data())
        let shell = Process()
        shell.executableURL = URL(fileURLWithPath: "/bin/zsh")
        shell.arguments = ["-c", "source \(snapshot.path) 2>/dev/null || true && sleep 30"]
        try shell.run()
        defer { if shell.isRunning { shell.terminate(); shell.waitUntilExit() } }

        let pid = Int(ProcessInfo.processInfo.processIdentifier)
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: ["CLAUDE_CONFIG_DIR": dir.path])
        let r = SessionRegistry(cli: cli, url: dir.appendingPathComponent("sessions.json"))
        r.add(Session(id: "s1", cwd: dir.path, startedAt: 1, sessionId: "s1", name: ""))
        r.pids = { [pid: "s1"] }

        await r.pollNow()
        XCTAssertTrue(r.sessions[0].hasRunningShell)

        shell.terminate()
        shell.waitUntilExit()
        await r.pollNow()
        XCTAssertFalse(r.sessions[0].hasRunningShell)
    }
}
