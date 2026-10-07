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
}
