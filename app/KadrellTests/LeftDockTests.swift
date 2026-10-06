import XCTest
import AppKit
@testable import Kadrell

/// Extension-Panels im linken Dock: `dock`-Zuordnung und das Ein-/Ausblenden unter dem Baum.
@MainActor
final class LeftDockTests: XCTestCase {
    func testDockPlacedDefaultsToRight() {
        XCTAssertEqual(Dock.placed("left"), .left)
        XCTAssertEqual(Dock.placed("right"), .right)
        XCTAssertEqual(Dock.placed(nil), .right)
        XCTAssertEqual(Dock.placed("top"), .right, "reserviert, noch nicht platziert: fällt nach rechts")
        XCTAssertEqual(Dock.placed("bogus"), .right)
    }

    func testLeftDockAppearsWithPanelAndHidesWhenEmpty() {
        let wc = MainWindowController(index: 8, cascade: nil)
        wc.window.setContentSize(NSSize(width: 1000, height: 700))
        wc.split.layoutSubtreeIfNeeded()
        XCTAssertNil(wc.leftDockScroll.superview, "ohne Panel kein Dock")

        let panel = [(name: "ssh", tree: PanelTree(title: "SSH", nodes: [.item(text: "vault", detail: nil, color: nil, actions: [])]))]
        wc.updateLeftDock(panel)
        XCTAssertNotNil(wc.leftDockScroll.superview, "mit Panel erscheint das Dock in der linken Spalte")
        XCTAssertEqual(wc.leftDockPanel.panels.count, 1)

        wc.updateLeftDock([])
        XCTAssertNil(wc.leftDockScroll.superview, "leeres Panel nimmt das Dock wieder raus")
    }
}
