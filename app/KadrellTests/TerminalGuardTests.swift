import XCTest
import SwiftTerm
@testable import Kadrell

@MainActor
final class TerminalGuardTests: XCTestCase {
    func testGuardDeniesClipboardRead() {
        let t = KadrellTerminalView(frame: NSRect(x: 0, y: 0, width: 400, height: 200))
        let g = TerminalDelegateGuard(t)
        XCTAssertNil(g.clipboardRead(source: t))
        XCTAssertNil(g.clipboardRead(source: t))
    }

    /// Nach dem Start hängt der Guard als Delegate am Terminal, und Eingaben über ihn erreichen weiter den Prozess.
    func testAttachedTerminalUsesGuardAndForwardsInput() async throws {
        let id = Session.shellPrefix + "guard-test"
        let attach = AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ["SHELL": "/bin/sh"]))
        attach.attachNow(Session(id: id, cwd: NSTemporaryDirectory(), startedAt: 0, sessionId: id, name: "Shell"))
        let t = try XCTUnwrap(attach.terminal(for: id))
        let delegate = try XCTUnwrap(t.terminalDelegate)
        XCTAssertTrue(delegate is TerminalDelegateGuard)
        XCTAssertNil(delegate.clipboardRead(source: t))
        delegate.send(source: t, data: Array("echo $((6*7))x\r".utf8)[...])
        let deadline = Date().addingTimeInterval(5)
        while !AttachManager.snapshotRows(t, trimTrailing: true).contains("42x"), Date() < deadline {
            try await Task.sleep(for: .milliseconds(50))
        }
        XCTAssertTrue(AttachManager.snapshotRows(t, trimTrailing: true).contains("42x"))
        attach.stop(id)
        await attach.shutdown(timeout: 5)
    }

    func testLinkAction() throws {
        XCTAssertEqual(KadrellTerminalView.linkAction(for: "https://example.com/a.app"), .open)
        XCTAssertEqual(KadrellTerminalView.linkAction(for: "mailto:a@b.de"), .open)
        XCTAssertEqual(KadrellTerminalView.linkAction(for: "/Applications/Calculator.app"),
                       .reveal(URL(fileURLWithPath: "/Applications/Calculator.app")))
        let home = URL(fileURLWithPath: NSHomeDirectory())
        XCTAssertEqual(KadrellTerminalView.linkAction(for: "~/x.command"), .reveal(home.appendingPathComponent("x.command")))

        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-link-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let exe = dir.appendingPathComponent("run")
        let txt = dir.appendingPathComponent("notes.txt")
        try Data("echo hi".utf8).write(to: exe)
        try Data("hi".utf8).write(to: txt)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: exe.path)
        XCTAssertEqual(KadrellTerminalView.linkAction(for: exe.path), .reveal(exe))
        XCTAssertEqual(KadrellTerminalView.linkAction(for: exe.absoluteString), .reveal(exe))
        XCTAssertEqual(KadrellTerminalView.linkAction(for: txt.path), .open)
        XCTAssertEqual(KadrellTerminalView.linkAction(for: txt.path + ":12"), .open)
        XCTAssertEqual(KadrellTerminalView.linkAction(for: dir.path), .open)
    }
}
