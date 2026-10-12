import XCTest
@testable import Kadrell

@MainActor
final class AttachManagerTests: XCTestCase {
    /// Nach Stop und gemeldetem Prozessende hält nichts mehr das Terminal (kein Zyklus über `onExit`).
    func testTerminalIsReleasedAfterStop() async throws {
        let id = Session.shellPrefix + "release-test"
        let attach = AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ["SHELL": "/bin/sh"]))
        weak var weakTerminal: KadrellTerminalView?
        do {
            attach.attachNow(Session(id: id, cwd: NSTemporaryDirectory(), startedAt: 0, sessionId: id, name: "Shell"))
            weakTerminal = attach.terminal(for: id)
            XCTAssertNotNil(weakTerminal)
            attach.stop(id)
        }
        let deadline = Date().addingTimeInterval(5)
        while weakTerminal != nil, Date() < deadline { try await Task.sleep(for: .milliseconds(50)) }
        XCTAssertNil(weakTerminal)
    }

    /// Ein Satzpunkt (oder Komma) hinter einem erkannten Dateipfad gehört nicht zum Link, sonst öffnet der Klick nichts.
    func testTrimmedLinkDropsTrailingPunctuation() {
        XCTAssertEqual(KadrellTerminalView.trimmedLink("~/.claude/texte/fragen-an-rapp.txt."), "~/.claude/texte/fragen-an-rapp.txt")
        XCTAssertEqual(KadrellTerminalView.trimmedLink("~/a/b.txt,"), "~/a/b.txt")
        XCTAssertEqual(KadrellTerminalView.trimmedLink("https://example.com/path"), "https://example.com/path")
        XCTAssertEqual(KadrellTerminalView.trimmedLink("file.swift:12"), "file.swift:12")
    }

    /// Hintergrundjob, der die Pipe erbt, blockiert `ProcessRunner.run` nicht; ein hängender Prozess endet per Timeout.
    func testProcessRunnerReturnsAtProcessExitAndTimesOut() async throws {
        let t0 = Date()
        let r = try await ProcessRunner.run("/bin/sh", ["-c", "sleep 30 & echo fertig"])
        XCTAssertEqual(r.status, 0)
        XCTAssertEqual(r.output, "fertig\n")
        XCTAssertLessThan(Date().timeIntervalSince(t0), 10)
        let hung = try await ProcessRunner.run("/bin/sleep", ["30"], timeout: 0.5)
        XCTAssertEqual(hung.status, SIGKILL)
        XCTAssertLessThan(Date().timeIntervalSince(t0), 15)
    }

    /// Remote-Kachel: ohne tmux (Windows, PowerShell) geht kein Shell-Code an den Host, ssh startet ohne Befehl.
    /// Ein Fake-`ssh` beantwortet die Probe und schreibt die Argumente jedes Aufrufs als eine Zeile.
    func testSSHLaunchFallsBackToPlainShellWithoutTmux() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        // Probe-Ausgabe zählt, nicht der Exit-Code (PowerShell meldet 0 ohne tmux).
        for (probe, expected) in [("", "-t -o ConnectTimeout=5 -- win"), ("tmux 3.3a", "-t -o ConnectTimeout=5 -- win tmux new -As 'a b'")] {
            let log = dir.appendingPathComponent("log\(probe.count)").path
            let fake = dir.appendingPathComponent("ssh\(probe.count)").path
            try "#!/bin/sh\ncase \"$*\" in *BatchMode*) echo '\(probe)'; exit 0;; esac\necho \"$*\" >> '\(log)'\n".write(toFile: fake, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: fake)
            let (exe, args, _) = AttachManager.sshLaunch(host: "win", tmuxSession: "a b", ssh: fake)
            _ = try await ProcessRunner.run(exe, args)
            XCTAssertEqual(try String(contentsOfFile: log, encoding: .utf8), expected + "\n")
        }
    }
}
