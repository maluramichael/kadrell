import XCTest
@testable import Kadrell

/// Die dritte Region (rechts) hängt als eigener Split-Pane neben Baum und Arbeitsfläche, startet versteckt und
/// bleibt höchstens halb so breit. Mit `KADRELL_RENDER_OUT=<pfad>.png` legt der Test ein Bild für die Sichtprüfung ab.
@MainActor
final class MainWindowRightPanelTests: XCTestCase {
    private func cleanup(_ c: MainWindowController) {
        c.window.close()
        for key in ["KadrellSplit.99", "KadrellMain.99", "workspace.selected.99", "workspace.mode.99", "workspace.auto.99"] {
            Profile.defaults.removeObject(forKey: key)
        }
    }

    func testRightPanelStructureConstraintAndRender() throws {
        let hadVisible = Settings.rightPanelVisible
        Settings.rightPanelVisible = false
        let c = MainWindowController(index: 98, cascade: nil)
        defer { cleanup(c); Settings.rightPanelVisible = hadVisible }

        // Struktur: drei Regionen, rechts ist die dritte und startet versteckt.
        XCTAssertEqual(c.split.arrangedSubviews.count, 3)
        XCTAssertTrue(c.split.arrangedSubviews[2] === c.rightContainer)
        XCTAssertTrue(c.rightContainer.isHidden)

        let size = NSSize(width: 1000, height: 640)
        c.window.setContentSize(size)
        c.rightContainer.isHidden = false
        c.split.layoutSubtreeIfNeeded()

        // Rechts höchstens halb so breit, auch wenn man den Divider weit nach links zieht.
        c.split.setPosition(40, ofDividerAt: MainWindowController.rightDivider)
        c.split.layoutSubtreeIfNeeded()
        XCTAssertLessThanOrEqual(c.rightContainer.frame.width, c.split.bounds.width / 2 + 1)

        // Filter zeigt nur die passenden Tickets.
        c.ticketPanel.setTickets([
            Ticket(id: "8", title: "Rechte Ticket-Leiste bauen", description: "", url: "", project: "Kadrell"),
            Ticket(id: "9", title: "Kanboard-Provider anbinden", description: "", url: "", project: "Vivatis"),
        ])
        c.ticketPanel.filter = "kanboard"
        XCTAssertEqual(c.ticketPanel.tickets.count, 1)
        c.ticketPanel.filter = ""

        guard let out = ProcessInfo.processInfo.environment["KADRELL_RENDER_OUT"] else { return }
        c.ensureRightWidth()
        c.ticketPanel.setTickets([
            Ticket(id: "8", title: "Rechte Ticket-Leiste bauen", description: "", url: "", project: "Kadrell"),
            Ticket(id: "9", title: "Kanboard-Provider anbinden", description: "", url: "", project: "Kadrell"),
            Ticket(id: "12", title: "Usage-Tooltip verbessern", description: "", url: "", project: "Kadrell"),
        ])
        c.window.contentView?.layoutSubtreeIfNeeded()
        c.ticketPanel.reload()
        guard let root = c.window.contentView else { return XCTFail("kein contentView") }
        root.frame = NSRect(origin: .zero, size: size)
        root.layoutSubtreeIfNeeded()
        guard let rep = root.bitmapImageRepForCachingDisplay(in: root.bounds) else { return XCTFail("kein Bitmap") }
        root.cacheDisplay(in: root.bounds, to: rep)
        guard let png = rep.representation(using: .png, properties: [:]) else { return XCTFail("kein PNG") }
        try png.write(to: URL(fileURLWithPath: out))
    }
}
