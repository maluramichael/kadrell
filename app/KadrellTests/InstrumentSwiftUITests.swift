import XCTest
import SwiftUI
@testable import Kadrell

/// SwiftUI-Gegenstücke des Instrument-Kits: Fuge, erhabene Fläche, Feld, Tasten und der Dialograhmen, als Bild
/// gerendert und an einzelnen Pixeln geprüft.
@MainActor
final class InstrumentSwiftUITests: XCTestCase {
    func testSeamLineDarkThenLight() {
        let t = ColorTheme.named("instrument-dark")
        let s = snapshot(ZStack { Color(nsColor: t.raised); SeamLine() }.frame(width: 40, height: 2), theme: t)
        XCTAssertLessThan(luma(pixel(s, at: CGPoint(x: 20, y: 0.25))), luma(t.raised))
        XCTAssertGreaterThan(luma(pixel(s, at: CGPoint(x: 20, y: 1.25))), luma(pixel(s, at: CGPoint(x: 20, y: 0.25))))
    }

    func testRaisedPanelAddsShadowDepth() {
        let s = snapshot(Color.clear.frame(width: 100, height: 50).raisedPanel(), theme: .named("instrument-dark"))
        XCTAssertEqual(s.size.height, 53, accuracy: 0.5)
    }

    func testRaisedPanelDoesNotClipContent() {
        // Ein Ring, der 2 pt über den Rand ragt (wie kbdFocusRing), bleibt sichtbar. Die Linie liegt mittig auf dem
        // um 2 pt erweiterten Rand, also 1 bis 3 pt vom Bildrand: y = 2 trifft ihre Mitte.
        let t = ColorTheme.named("instrument-light")
        let s = snapshot(Color.clear.frame(width: 100, height: 50)
            .overlay(Rectangle().stroke(Color(nsColor: t.accent), lineWidth: 2).padding(-2)).raisedPanel().padding(4), theme: t)
        XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: 50, y: 2)), t.accent), 0.1)
    }

    func testPrimaryKeyShowsAccentCap() {
        let t = ColorTheme.named("instrument-light")
        var edge = NSColor.clear
        withTheme(t) { edge = Instrument.keyEdge }
        let s = snapshot(Button("Starten") {}.buttonStyle(InstrumentKeyStyle(.primary)), theme: t)
        XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: 3, y: s.size.height / 2)), t.accent), 0.05, "Kappe in accent")
        XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: s.size.width / 2, y: s.size.height - 0.5)), edge), 0.05, "Dicke unten")
    }

    func testFocusedWellShowsAccentRing() {
        let t = ColorTheme.named("instrument-dark")
        let s = snapshot(Text("x").frame(width: 80).instrumentWell(focused: true), theme: t)
        XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: 40, y: 0.5)), t.accent), 0.05)
    }

    func testRowShowsSurfaceAndOutlineOnlyWhenSelected() {
        let t = ColorTheme.named("instrument-dark")
        let row = { (selected: Bool) in Color.clear.frame(width: 100, height: 24).instrumentRow(selected: selected) }
        let on = snapshot(row(true), theme: t), off = snapshot(row(false), theme: t)
        XCTAssertLessThan(distance(pixel(on, at: CGPoint(x: 50, y: 12)), t.bg), 0.05, "Fläche")
        XCTAssertLessThan(distance(pixel(on, at: CGPoint(x: 50, y: 1.75)), t.fg), 0.1, "Umriss in fg")
        XCTAssertLessThan(off.rep.colorAt(x: off.rep.pixelsWide / 2, y: off.rep.pixelsHigh / 2)!.alphaComponent, 0.05, "ohne Auswahl leer")
    }

    func testDialogFrameHasNoOwnBackground() {
        let s = snapshot(Text("x").dialogFrame(), theme: .named("instrument-dark"))
        XCTAssertLessThan(s.rep.colorAt(x: s.rep.pixelsWide - 2, y: 2)!.alphaComponent, 0.05)
    }
}
