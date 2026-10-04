import XCTest
import AppKit
@testable import Kadrell

/// Cmd+B blendet den Baum aus, indem er ganz aus dem Split genommen wird. Dann bleibt kein Trenner übrig –
/// weder als vertikale Linie noch als Griffzone zum Herausziehen. Einblenden stellt die alte Breite wieder her.
@MainActor
final class SidebarToggleTests: XCTestCase {
    func testHidingSidebarRemovesDividerAndRestoresWidth() {
        let wc = MainWindowController(index: 7, cascade: nil)
        wc.window.setContentSize(NSSize(width: 1000, height: 700))
        wc.split.layoutSubtreeIfNeeded()

        XCTAssertFalse(wc.isSidebarHidden)
        XCTAssertEqual(wc.split.arrangedSubviews.count, 2, "sichtbar: Baum und Arbeitsfläche mit Trenner dazwischen")
        let widthBefore = wc.sidebarScroll.frame.width
        XCTAssertGreaterThan(widthBefore, 100, "Baum hat eine sinnvolle Startbreite")

        wc.toggleSidebar()
        XCTAssertTrue(wc.isSidebarHidden)
        XCTAssertEqual(wc.split.arrangedSubviews.count, 1, "ausgeblendet: nur die Arbeitsfläche, kein Trenner")
        XCTAssertNil(wc.sidebarScroll.superview, "Baum ist nicht mehr im Split, also nichts zu zeichnen oder zu ziehen")

        wc.toggleSidebar()
        XCTAssertFalse(wc.isSidebarHidden)
        XCTAssertEqual(wc.split.arrangedSubviews.count, 2)
        wc.split.layoutSubtreeIfNeeded()
        XCTAssertEqual(wc.sidebarScroll.frame.width, widthBefore, accuracy: 1, "kommt auf der zuletzt genutzten Breite zurück")
    }
}
