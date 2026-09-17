import AppKit
import SwiftUI
import XCTest
@testable import Kadrell

@MainActor
final class BackdropTests: XCTestCase {
    private func blurred(_ w: NSWindow) -> Bool {
        w.contentView!.subviews.contains { $0.identifier?.rawValue == "KadrellBackdrop" }
    }

    private func window() -> NSWindow {
        // orderFront braucht es für isVisible, auf dem Bildschirm soll es trotzdem nicht aufblitzen. Rahmenlos, weil
        // macOS Fenster mit Titelleiste zurück in den sichtbaren Bereich schiebt.
        let w = NSWindow(contentRect: NSRect(x: -20000, y: -20000, width: 400, height: 300), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.orderFront(nil)
        return w
    }

    func testDismissRemovesBlur() {
        let w = window()
        let p = OverlayPanel(rootView: Text("x"))
        p.open(over: w)
        XCTAssertTrue(blurred(w))
        p.dismiss()
        XCTAssertFalse(blurred(w))
    }

    /// Wird das Panel vorher schon ausgeblendet oder geschlossen, ist `parent` nil. Der Blur muss trotzdem weg.
    func testBlurGoneWhenPanelHiddenOrClosedFirst() {
        let w = window()
        let p = OverlayPanel(rootView: Text("x"))
        p.open(over: w)
        p.orderOut(nil)
        XCTAssertFalse(blurred(w))
        p.dismiss()
        XCTAssertFalse(blurred(w))

        p.open(over: w)
        p.close()
        XCTAssertFalse(blurred(w))
    }

    func testPaletteClosedFirstRemovesBlur() {
        let w = window()
        let p = PaletteWindow()
        p.open(over: w)
        XCTAssertTrue(blurred(w))
        p.close()
        XCTAssertFalse(blurred(w))
    }
}
