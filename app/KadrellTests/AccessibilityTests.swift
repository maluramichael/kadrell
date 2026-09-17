import XCTest
@testable import Kadrell

@MainActor
final class AccessibilityTests: XCTestCase {
    private func window(_ view: NSView, _ size: NSSize) -> NSWindow {
        let w = NSWindow(contentRect: NSRect(origin: CGPoint(x: 100, y: 100), size: size), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        view.frame = NSRect(origin: .zero, size: size)
        w.contentView = view
        return w
    }

    private func elements(_ v: NSView) -> [A11yElement] { (v.accessibilityChildren() ?? []).compactMap { $0 as? A11yElement } }

    func testSidebarRows() {
        let a = Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "Alpha")
        let b = Session(id: "b", cwd: "/p", startedAt: 0, sessionId: "b", name: "Beta")
        let g = Group(id: "g", name: "Projekt", color: "#89b4fa", cwd: "/p", sessionIds: ["a", "b"], favorite: nil)
        let sidebar = SidebarView(frame: .zero)
        let w = window(sidebar, NSSize(width: 240, height: 400))
        defer { w.close() }
        sidebar.unread = ["b"]
        var picked: ([String], SidebarView.SelectMode)?
        sidebar.onSelect = { picked = ($0, $1) }
        sidebar.reload(groups: [g], sessions: [a, b])

        let rows = elements(sidebar)
        XCTAssertEqual(rows.map { $0.accessibilityLabel() }, ["Gruppe Projekt, 2 Sessions",
                                                              "Session Alpha, nicht gestartet",
                                                              "Session Beta, nicht gestartet, neu"])
        XCTAssertTrue(rows.allSatisfy { $0.accessibilityParent() as? NSView === sidebar })
        // Gespiegelte View: die zweite Zeile liegt auf dem Bildschirm unter der ersten, beide oben im Fenster.
        let top = rows[0].accessibilityFrame(), second = rows[1].accessibilityFrame()
        XCTAssertEqual(top.maxY, w.frame.maxY, accuracy: 20)
        XCTAssertEqual(second.maxY, top.minY, accuracy: 1)
        XCTAssertEqual(second.width, 240)

        XCTAssertTrue(rows[2].accessibilityPerformPress())
        XCTAssertEqual(picked?.0, ["b"]); XCTAssertEqual(picked?.1, .replace)
        XCTAssertTrue(rows[0].accessibilityPerformPress())
        XCTAssertEqual(picked?.0, ["a", "b"])

        // Einklappen per Aktion: Sessions verschwinden, das Gruppen-Element bleibt dasselbe Objekt.
        _ = rows[0].accessibilityCustomActions()?.first { $0.name == "Einklappen" }?.handler?() ?? false
        let after = elements(sidebar)
        XCTAssertEqual(after.count, 1)
        XCTAssertTrue(after[0] === rows[0])
        XCTAssertEqual(after[0].accessibilityLabel(), "Gruppe Projekt, 2 Sessions, eingeklappt")
    }

    func testStatusBarButtons() {
        let bar = StatusBarView(frame: .zero)
        let w = window(bar, NSSize(width: 900, height: 30))
        defer { w.close() }
        bar.layoutMode = .grid
        bar.sync = true
        bar.waitingCount = 2
        var toggled = 0
        bar.onToggleSync = { toggled += 1 }
        bar.display()

        let buttons = elements(bar)
        XCTAssertEqual(buttons.map { $0.accessibilityLabel() ?? "" },
                       ["Layout", "Weniger Spalten", "Mehr Spalten", "Auto-Modus", "Sync", "Sortierung", "Wartende Sessions"])
        let sync = buttons[4]
        XCTAssertEqual(sync.accessibilityValue() as? String, "an")
        XCTAssertEqual(buttons[6].accessibilityValue() as? String, "2")
        XCTAssertTrue(sync.accessibilityPerformPress())
        XCTAssertEqual(toggled, 1)
        bar.display()
        XCTAssertTrue(elements(bar)[4] === sync)
    }

    func testCellHeader() {
        var a = Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "Alpha"); a.branch = "master"
        let g = Group(id: "g", name: "Projekt", color: "#89b4fa", cwd: "/p", sessionIds: ["a"], favorite: nil)
        let ws = WorkspaceView(frame: .zero)
        let w = window(ws, NSSize(width: 800, height: 600))
        defer { w.close() }
        var renamed: String?
        ws.onRenameSession = { renamed = $0 }
        ws.reload(groups: [g], sessions: [a])
        ws.select(["a"], add: false, takeKeyboard: false)
        guard let cell = ws.subviews.compactMap({ $0 as? CellView }).first else { return XCTFail("keine Kachel") }
        cell.groupName = g.name
        let buttons = elements(cell)
        XCTAssertEqual(buttons.map { $0.accessibilityLabel() ?? "" }, ["Alpha, nicht gestartet, Projekt, master", "Umbenennen", "Schließen"])
        XCTAssertTrue(buttons[1].accessibilityPerformPress())
        XCTAssertEqual(renamed, "a")
    }
}
