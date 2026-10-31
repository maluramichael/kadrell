import AppKit
import SwiftUI
import XCTest
@testable import Kadrell

/// Grundregel aller Overlays: Größe und Lage folgen dem Fenster, beim Ändern der Größe und beim Verschieben, mittig
/// (Dialoge), fensterfüllend (Einstellungen) und für die Palette (⌘P).
@MainActor
final class OverlayFollowsWindowTests: XCTestCase {
    /// Rahmenlos und weit außerhalb des Bildschirms: Inhalt und Rahmen sind gleich, macOS schiebt nichts zurück.
    private func window(_ size: NSSize) -> NSWindow {
        let w = NSWindow(contentRect: NSRect(origin: CGPoint(x: -20000, y: -20000), size: size), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.orderFront(nil)
        addTeardownBlock { w.close() }
        return w
    }

    private func settle() { RunLoop.main.run(until: Date() + 0.1) }

    private func assertCentered(_ p: NSWindow, in w: NSWindow, line: UInt = #line) {
        XCTAssertEqual(p.frame.midX, w.frame.midX, accuracy: 1, "waagrecht mittig", line: line)
        XCTAssertEqual(p.frame.midY, w.frame.midY, accuracy: 1, "senkrecht mittig", line: line)
    }

    private func resize(_ w: NSWindow, _ size: NSSize) {
        w.setFrame(NSRect(origin: w.frame.origin, size: size), display: false)
    }

    func testCenteredDialogFollowsResizeAndMove() {
        let w = window(NSSize(width: 1400, height: 900))
        let p = OverlayPanel(rootView: Color.clear.frame(width: 400, height: 300))
        p.open(over: w)
        settle()
        assertCentered(p, in: w)
        resize(w, NSSize(width: 900, height: 600))
        settle()
        assertCentered(p, in: w)
        w.setFrameOrigin(CGPoint(x: -21000, y: -20500))
        settle()
        assertCentered(p, in: w)
        p.dismiss()
    }

    /// Ein Dialog, der breiter und höher wäre als das Fenster, bleibt samt Rand darin und wächst mit dem Fenster wieder
    /// auf seine eigene Breite.
    func testCenteredDialogStaysInsideWindow() {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let w = window(NSSize(width: 1400, height: 900))
        let p = OverlayPanel(rootView: Color.clear.frame(height: 2000).overlayWidth(1000))
        p.open(over: w)
        settle()
        XCTAssertEqual(p.frame.width, 1000, accuracy: 1)
        XCTAssertLessThanOrEqual(p.frame.height, 900 - 2 * OverlayGeometry.margin + 0.5)
        resize(w, NSSize(width: 800, height: 500))
        settle()
        XCTAssertEqual(p.frame.width, 800 - 2 * OverlayGeometry.margin, accuracy: 1)
        XCTAssertLessThanOrEqual(p.frame.height, 500 - 2 * OverlayGeometry.margin + 0.5)
        assertCentered(p, in: w)
        resize(w, NSSize(width: 1600, height: 900))
        settle()
        XCTAssertEqual(p.frame.width, 1000, accuracy: 1)
        assertCentered(p, in: w)
        p.dismiss()
    }

    /// Fensterfüllend: rund 99 % der Breite, oben bündig, schmaler Rand seitlich und unten. Der Rahmen folgt sofort,
    /// ohne Umweg über die RunLoop: auch während des Ziehens steht nichts über.
    func testFullWindowFillsWindowAndFollowsImmediately() {
        let w = window(NSSize(width: 1400, height: 900))
        let p = OverlayPanel(rootView: Text("x"), placement: .fullWindow)
        p.open(over: w)
        XCTAssertEqual(p.frame, OverlayGeometry.fullWindow(in: w.frame))
        XCTAssertEqual(p.frame.width / w.frame.width, 0.99, accuracy: 0.002)
        XCTAssertEqual(p.frame.maxY, w.frame.maxY)
        XCTAssertEqual(p.frame.minY - w.frame.minY, OverlayGeometry.inset(of: w.frame))
        resize(w, NSSize(width: 900, height: 600))
        XCTAssertEqual(p.frame, OverlayGeometry.fullWindow(in: w.frame))
        w.setFrameOrigin(CGPoint(x: -21000, y: -20500))
        XCTAssertEqual(p.frame, OverlayGeometry.fullWindow(in: w.frame))
        XCTAssertEqual(p.limit.maxWidth, p.frame.width)
        p.dismiss()
    }

    /// Einstellungen nach Vergrößern und wieder Verkleinern: die Fläche schrumpft mit und steht nicht über das Panel
    /// hinaus (die Fußleiste hielt sie früher auf der alten Breite fest), die Spalten fallen auf eine zurück.
    func testSettingsShrinkWithWindow() throws {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let w = window(NSSize(width: 1400, height: 900))
        let p = OverlayPanel(rootView: SettingsView(model: SettingsModel()), placement: .fullWindow)
        p.open(over: w)
        defer { p.limit.slideOut {}; p.dismiss() }
        let view = try XCTUnwrap(p.contentView)
        for (size, columns) in [(NSSize(width: 2000, height: 1100), 2), (NSSize(width: 900, height: 700), 1)] {
            resize(w, size)
            for _ in 0..<4 { view.layoutSubtreeIfNeeded(); settle() }
            let surface = try XCTUnwrap(surfaces(in: view).first { $0.part == .raised })
            let frame = surface.convert(surface.bounds, to: view)
            XCTAssertEqual(frame.minX, 0, accuracy: 0.5, "\(size)")
            XCTAssertEqual(frame.width, p.frame.width, accuracy: 0.5, "\(size)")
            let edges = Set(surfaces(in: view).filter { $0.part == .group }.map { $0.convert($0.bounds, to: view).minX.rounded() })
            XCTAssertEqual(edges.count, columns, "Spalten bei \(size)")
        }
    }

    /// Fensterfüllend fährt der Dialog erst hinaus: sofort keine Klicks mehr, nach der Ausfahrt weg samt Blur.
    func testFullWindowSlidesOutBeforeClosing() {
        let saved = Feedback.reduceMotion; defer { Feedback.reduceMotion = saved }
        for reduce in [false, true] {
            Feedback.reduceMotion = reduce
            let w = window(NSSize(width: 1000, height: 700))
            let p = OverlayPanel(rootView: Text("x"), placement: .fullWindow)
            p.open(over: w)
            settle()
            XCTAssertTrue(p.limit.shown, "eingefahren")
            p.dismiss()
            XCTAssertTrue(p.isVisible, "fährt noch hinaus")
            XCTAssertTrue(p.ignoresMouseEvents)
            XCTAssertFalse(p.limit.shown)
            RunLoop.main.run(until: Date() + OverlayLimit.slideOutDuration + 0.15)
            XCTAssertFalse(p.isVisible, "nach der Ausfahrt weg")
            XCTAssertFalse(w.contentView!.subviews.contains { $0.identifier?.rawValue == "KadrellBackdrop" }, "Blur weg")
        }
    }

    /// Mit „Bewegung reduzieren“ fährt nichts, der Dialog blendet nur kurz ein und aus.
    func testReduceMotionShortensSlide() {
        let saved = Feedback.reduceMotion; defer { Feedback.reduceMotion = saved }
        Feedback.reduceMotion = false
        let full = OverlayLimit.slideOutDuration
        Feedback.reduceMotion = true
        XCTAssertLessThan(OverlayLimit.slideOutDuration, full)
        XCTAssertNotEqual(OverlayLimit.slideInAnimation, OverlayLimit.slideOutAnimation)
    }

    /// Ein geschlossener Dialog hängt nicht mehr am Fenster: Größenänderungen danach bewegen ihn nicht.
    func testDismissedOverlayNoLongerFollows() {
        let w = window(NSSize(width: 1400, height: 900))
        let p = OverlayPanel(rootView: Text("x"), placement: .fullWindow)
        p.open(over: w)
        p.limit.slideOut {}   // ohne Ausfahrt schließen
        p.dismiss()
        let before = p.frame
        resize(w, NSSize(width: 900, height: 600))
        XCTAssertEqual(p.frame, before)
    }

    /// Palette: oben bei 12 % der Höhe, mittig, schrumpft in kleinen Fenstern auf den Inhalt abzüglich Rand.
    func testPaletteFollowsWindow() {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let w = window(NSSize(width: 1400, height: 900))
        let p = PaletteWindow()
        let full = p.frame.size
        p.open(over: w)
        XCTAssertEqual(p.frame.size, full)
        XCTAssertEqual(p.frame.midX, w.frame.midX, accuracy: 1)
        XCTAssertEqual(w.frame.maxY - p.frame.maxY, (0.12 * 900).rounded(), accuracy: 1)
        resize(w, NSSize(width: 600, height: 400))
        XCTAssertEqual(p.frame.width, 600 - 2 * OverlayGeometry.margin, accuracy: 1)
        XCTAssertEqual(p.frame.height, 400 - 2 * OverlayGeometry.margin, accuracy: 1)
        XCTAssertEqual(p.frame.midX, w.frame.midX, accuracy: 1)
        XCTAssertGreaterThanOrEqual(p.frame.minY, w.frame.minY + OverlayGeometry.margin - 0.5)
        resize(w, NSSize(width: 1400, height: 900))
        XCTAssertEqual(p.frame.size, full)
        p.dismiss(runHighlightReset: false)
    }
}
