import XCTest
@testable import Kadrell

/// Ein Terminal ist eine NSView und hängt nur in einem Fenster; die Kachel im anderen zeigt den Hinweis und holt es per Fokus.
@MainActor
final class MultiWindowTests: XCTestCase {
    private func window(_ view: NSView) -> NSWindow {
        let w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 600, height: 400), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        view.frame = NSRect(x: 0, y: 0, width: 600, height: 400)
        w.contentView = view
        return w
    }

    private func cell(_ ws: WorkspaceView) -> CellView { ws.subviews.compactMap { $0 as? CellView }.first! }

    func testTerminalHangsInOneWindow() {
        let id = Session.shellPrefix + "multiwindow-test"
        let s = Session(id: id, cwd: NSTemporaryDirectory(), startedAt: 0, sessionId: id, name: "Shell")
        let g = Group(id: "g", name: "Projekt", color: "#89b4fa", cwd: NSTemporaryDirectory(), sessionIds: [id], favorite: false)
        let attach = AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ProcessInfo.processInfo.environment))
        let a = WorkspaceView(frame: .zero, defaultsSuffix: ".test-a"), b = WorkspaceView(frame: .zero, defaultsSuffix: ".test-b")
        let wa = window(a), wb = window(b)
        defer {
            attach.detachAll(); a.close(); b.close(); wa.close(); wb.close()
            for k in ["workspace.selected", "workspace.mode", "workspace.auto"] { for sfx in [".test-a", ".test-b"] { Profile.defaults.removeObject(forKey: k + sfx) } }
        }
        for ws in [a, b] {
            ws.attach = attach
            ws.reload(groups: [g], sessions: [s])
            ws.select([id], add: false, takeKeyboard: false)
        }
        guard let t = attach.terminal(for: id) else { return XCTFail("kein Terminal") }
        XCTAssertTrue(t.superview?.superview === a)
        XCTAssertTrue(cell(b).elsewhere)

        b.setFocus(id)
        XCTAssertTrue(t.superview?.superview === b)
        XCTAssertTrue(cell(a).elsewhere)
        XCTAssertFalse(cell(b).elsewhere)

        var released = false
        b.onReleaseTerminal = { released = true }
        b.select([id], add: true, takeKeyboard: false)
        XCTAssertTrue(released)
        a.relayout()
        XCTAssertTrue(t.superview?.superview === a)
        XCTAssertFalse(cell(a).elsewhere)
    }
}
