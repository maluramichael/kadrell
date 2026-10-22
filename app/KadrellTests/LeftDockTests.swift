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

    /// Der waagerechte Trenner der linken Spalte hat einen eigenen Delegate: Min-Höhen (kein Kollaps auf 0) und
    /// eine breite, waagerechte Greifzone statt der 1px-Standardzone.
    func testLeftDockDividerConstrainedAndGrabbable() {
        let wc = MainWindowController(index: 9, cascade: nil)
        wc.window.setContentSize(NSSize(width: 1000, height: 700))
        wc.split.layoutSubtreeIfNeeded()
        XCTAssertTrue(wc.leftColumn.delegate === wc, "linke Spalte hat einen eigenen Delegate")
        XCTAssertEqual(wc.splitView(wc.leftColumn, constrainMinCoordinate: 0, ofSubviewAt: 0), MainWindowController.treeMin,
                       "Baum behält seine Mindesthöhe")
        XCTAssertEqual(wc.splitView(wc.leftColumn, constrainMaxCoordinate: 9999, ofSubviewAt: 0),
                       wc.leftColumn.bounds.height - MainWindowController.dockMin, "Dock behält seine Mindesthöhe")
        let drawn = CGRect(x: 0, y: 100, width: 200, height: 1)
        let eff = wc.splitView(wc.leftColumn, effectiveRect: .zero, forDrawnRect: drawn, ofDividerAt: 0)
        XCTAssertEqual(eff.height, 1 + ThinSplitView.grabWidth, "waagerechte Greifzone ist deutlich höher als 1px")
        XCTAssertEqual(eff.width, 200, "waagerecht wird die Höhe aufgeweitet, nicht die Breite")
    }

    /// Die waagerechte Griffzone sitzt auf dem Trenner (Unterkante des Baums), nicht am oberen Rand der Spalte.
    func testHorizontalGrabZoneSitsOnDivider() {
        let host = FlippedView(frame: CGRect(x: 0, y: 0, width: 100, height: 200))
        let split = ThinSplitView(frame: host.bounds); split.isVertical = false
        host.addSubview(split)
        split.addArrangedSubview(NSView()); split.addArrangedSubview(NSView())
        split.adjustSubviews()
        split.setPosition(100, ofDividerAt: 0)   // Trenner bei y = 100 bis 102
        for y: CGFloat in [97, 101, 105] { XCTAssertTrue(split.hitTest(CGPoint(x: 50, y: y)) === split, "y \(y) greift den Trenner") }
        XCTAssertFalse(split.hitTest(CGPoint(x: 50, y: 2)) === split, "oberer Rand des Baums bleibt klickbar")
    }
}
