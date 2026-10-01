import XCTest
@testable import Kadrell

/// Baum per Tastatur: ⇧↑↓ erweitern die Auswahl, `x` schaltet um, VoiceOver folgt dem Cursor.
@MainActor
final class SidebarKeyboardTests: XCTestCase {
    private func make() -> (SidebarView, NSWindow) {
        let ss = ["a", "b", "c"].map { Session(id: $0, cwd: "/p", startedAt: 0, sessionId: $0, name: $0.uppercased()) }
        let g = Group(id: "g", name: "P", color: "#89b4fa", cwd: "/p", sessionIds: ["a", "b", "c"], favorite: false)
        let sidebar = SidebarView(frame: NSRect(x: 0, y: 0, width: 240, height: 400))
        let w = NSWindow(contentRect: sidebar.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.contentView = sidebar
        w.makeFirstResponder(sidebar)
        sidebar.reload(groups: [g], sessions: ss)
        return (sidebar, w)
    }

    private func key(_ code: UInt16, _ mods: NSEvent.ModifierFlags = []) -> NSEvent {
        NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: 0, windowNumber: 0, context: nil,
                         characters: "", charactersIgnoringModifiers: "", isARepeat: false, keyCode: code)!
    }

    func testShiftDownAddsNextSession() {
        let (s, w) = make(); defer { w.close() }
        s.focused = "a"
        var picked: ([String], SidebarView.SelectMode)?
        s.onSelect = { picked = ($0, $1) }
        s.keyDown(with: key(125, .shift))
        XCTAssertEqual(picked?.0, ["b"]); XCTAssertEqual(picked?.1, .add)
        XCTAssertEqual(s.focused, "b")
    }

    func testShiftUpAtTopStaysOnFirst() {
        let (s, w) = make(); defer { w.close() }
        s.focused = "a"
        var picked: ([String], SidebarView.SelectMode)?
        s.onSelect = { picked = ($0, $1) }
        s.keyDown(with: key(126, .shift))
        XCTAssertEqual(picked?.0, ["a"]); XCTAssertEqual(picked?.1, .add)
    }

    func testXTogglesCursorSession() {
        let (s, w) = make(); defer { w.close() }
        s.focused = "b"
        var picked: ([String], SidebarView.SelectMode)?
        s.onSelect = { picked = ($0, $1) }
        s.keyDown(with: key(7))
        XCTAssertEqual(picked?.0, ["b"]); XCTAssertEqual(picked?.1, .toggle)
    }

    func testSpaceStillOpens() {
        let (s, w) = make(); defer { w.close() }
        s.focused = "b"
        var picked: ([String], SidebarView.SelectMode)?
        s.onSelect = { picked = ($0, $1) }
        s.keyDown(with: key(49))
        XCTAssertEqual(picked?.1, .replace)
    }

    func testAccessibilityFocusFollowsCursor() throws {
        let (s, w) = make(); defer { w.close() }
        s.focused = "a"
        s.onSelect = { _, _ in }
        s.keyDown(with: key(125))
        let e = try XCTUnwrap(s.accessibilityFocusedUIElement as? A11yElement)
        XCTAssertEqual(e.key, "s:b")
    }
}
