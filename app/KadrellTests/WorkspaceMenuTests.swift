import XCTest
@testable import Kadrell

/// Rechtsklick auf einer Kachel: mit markiertem Terminaltext Kopieren und Einsetzen, sonst das Kontextmenü der Session.
@MainActor
final class WorkspaceMenuTests: XCTestCase {
    func testRightClickOffersCopyPasteWhileTextIsSelected() {
        let id = Session.shellPrefix + "menu-test"
        let s = Session(id: id, cwd: NSTemporaryDirectory(), startedAt: 0, sessionId: id, name: "Shell")
        let g = Group(id: "g", name: "Projekt", color: "#89b4fa", cwd: NSTemporaryDirectory(), sessionIds: [id], favorite: false)
        let attach = AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ProcessInfo.processInfo.environment))
        let ws = WorkspaceView(frame: .zero, defaultsSuffix: ".test-menu")
        let w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 600, height: 400), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        ws.frame = NSRect(x: 0, y: 0, width: 600, height: 400)
        w.contentView = ws
        defer {
            attach.detachAll(); ws.close(); w.close()
            for k in ["workspace.selected", "workspace.mode", "workspace.auto"] { Profile.defaults.removeObject(forKey: k + ".test-menu") }
        }
        ws.attach = attach
        ws.reload(groups: [g], sessions: [s])
        ws.select([id], add: false, takeKeyboard: false)
        guard let t = attach.terminal(for: id), let cell = ws.subviews.compactMap({ $0 as? CellView }).first else { return XCTFail("keine Kachel") }
        ws.onContextMenu = { _ in
            let m = NSMenu()
            m.addItem(NSMenuItem(title: "Stoppen", action: nil, keyEquivalent: ""))
            return m
        }
        let center = ws.convert(CGPoint(x: cell.frame.midX, y: cell.frame.midY), to: nil)
        let click = NSEvent.mouseEvent(with: .rightMouseDown, location: center, modifierFlags: [], timestamp: 0, windowNumber: w.windowNumber,
                                       context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!

        XCTAssertEqual(ws.menu(for: click)?.items.map(\.title), ["Stoppen"], "ohne Auswahl bleibt das Kontextmenü der Session")

        t.feed(text: "hallo\r\n")
        t.selectAll()
        XCTAssertEqual(t.getSelection()?.contains("hallo"), true, "Text ist markiert")
        XCTAssertEqual(ws.menu(for: click)?.items.map(\.action), [#selector(NSText.copy(_:)), #selector(NSText.paste(_:))])
        XCTAssertTrue(ws.menu(for: click)?.items.allSatisfy { $0.target === t } == true, "Kopieren und Einsetzen gehen an das Terminal")
    }
}
