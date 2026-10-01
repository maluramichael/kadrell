import XCTest
@testable import Kadrell

@MainActor
final class WorkspaceViewTests: XCTestCase {
    private let sfx = ".test-workspaceview"

    private func make() -> (WorkspaceView, NSWindow) {
        let ws = WorkspaceView(frame: .zero, defaultsSuffix: sfx)
        let w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 600, height: 400), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        ws.frame = NSRect(x: 0, y: 0, width: 600, height: 400)
        w.contentView = ws
        return (ws, w)
    }

    private func cleanup(_ ws: WorkspaceView, _ w: NSWindow) {
        ws.close(); w.close()
        ws.clearPersisted()
        Settings.resetLayoutRatios()
    }

    func testEmptyStateButtonsAreAccessible() {
        let (ws, w) = make()
        defer { cleanup(ws, w) }
        var started = false
        ws.onEmptyClick = { started = true }
        ws.polled = true
        ws.reload(groups: [], sessions: [])
        ws.display()
        let buttons = (ws.accessibilityChildren() ?? []).compactMap { $0 as? A11yElement }
        XCTAssertEqual(buttons.count, ws.emptyHitRects.count)
        XCTAssertTrue(buttons.allSatisfy { $0.accessibilityRole() == .button })
        guard let b = buttons.first(where: { $0.accessibilityLabel() == String(localized: "Neue Session starten  ⌘N", bundle: Bundle.app) }) else { return XCTFail("kein Knopf") }
        XCTAssertTrue(b.accessibilityPerformPress())
        XCTAssertTrue(started)
    }

    func testClearPersistedRemovesAllKeys() {
        let (ws, w) = make()
        defer { cleanup(ws, w) }
        for k in WorkspaceView.persistedKeys { Profile.defaults.set("x", forKey: k + sfx) }
        ws.clearPersisted()
        for k in WorkspaceView.persistedKeys { XCTAssertNil(Profile.defaults.object(forKey: k + sfx), k) }
    }

    func testDividerDragPersistsOnlyOnMouseUp() {
        let (ws, w) = make()
        defer { cleanup(ws, w) }
        Settings.resetLayoutRatios()
        let ids = ["a", "b"].map { Session.shellPrefix + "wvt-" + $0 }
        let sessions = ids.map { Session(id: $0, cwd: NSTemporaryDirectory(), startedAt: 0, sessionId: $0, name: "Shell") }
        let g = Group(id: "g", name: "P", color: "#89b4fa", cwd: NSTemporaryDirectory(), sessionIds: ids, favorite: false)
        ws.reload(groups: [g], sessions: sessions)
        ws.select(ids, add: false, takeKeyboard: false)
        func saved() -> [String] { Profile.defaults.dictionaryRepresentation().keys.filter { $0.hasPrefix("layout.") && !$0.hasSuffix(".columns") } }
        func event(_ type: NSEvent.EventType, x: CGFloat) -> NSEvent {
            let p = ws.convert(CGPoint(x: x, y: 200), to: nil)
            return NSEvent.mouseEvent(with: type, location: p, modifierFlags: [], timestamp: 0, windowNumber: w.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
        }
        ws.mouseDown(with: event(.leftMouseDown, x: 300))
        ws.mouseDragged(with: event(.leftMouseDragged, x: 400))
        XCTAssertEqual(saved(), [], "beim Ziehen nichts speichern")
        ws.mouseUp(with: event(.leftMouseUp, x: 400))
        XCTAssertFalse(saved().isEmpty, "bei mouseUp speichern")
    }
}
