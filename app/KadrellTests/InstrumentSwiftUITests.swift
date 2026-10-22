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

    // MARK: UI-Größe 130 %

    func testSeamLineStaysOnePlusOneAtScale() {
        let t = ColorTheme.named("instrument-dark")
        let s = snapshot(ZStack(alignment: .top) { Color(nsColor: t.raised); SeamLine() }.frame(width: 40, height: 6), theme: t, scale: 1.3)
        let at = { (y: CGFloat) in self.pixel(s, at: CGPoint(x: 20, y: y)) }
        // Je ein voller Punkt dunkel und hell, beide in sich gleich, darunter nur noch die Fläche.
        XCTAssertLessThan(distance(at(0.25), at(0.75)), 0.01, "dunkle Zeile ganz")
        XCTAssertLessThan(distance(at(1.25), at(1.75)), 0.01, "helle Zeile ganz")
        for y: CGFloat in [2.25, 3, 5.5] { XCTAssertLessThan(distance(at(y), t.raised), 0.02, "Fläche bei \(y)") }
        XCTAssertLessThan(luma(at(0.5)), luma(t.raised) - 0.05, "dunkel unter der Fläche")
        XCTAssertGreaterThan(luma(at(1.5)), luma(t.raised) + 0.02, "hell über der Fläche")
    }

    func testRaisedPanelShadowDepthScales() {
        let s = snapshot(Color.clear.frame(width: 100, height: 50).raisedPanel(), theme: .named("instrument-dark"), scale: 1.3)
        XCTAssertEqual(s.size.height, 50 + Instrument.shadowDepth * 1.3, accuracy: 0.5)
    }

    func testPrimaryKeyDepthScales() {
        // Unter der Kappe liegen Dicke (2) und Kappenrand (1) logische Punkte in keyEdge: bei 100 % 3 pt, bei 130 % 3,9 pt.
        // 3,5 pt über der Unterkante ist also nur bei 130 % noch Kante, 4,5 pt darüber schon Kappe.
        let t = ColorTheme.named("instrument-light")
        var edge = NSColor.clear
        withTheme(t) { edge = Instrument.keyEdge }
        let s = snapshot(Button("Starten") {}.buttonStyle(InstrumentKeyStyle(.primary)), theme: t, scale: 1.3)
        let x = s.size.width / 2
        XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: x, y: s.size.height - 3.5)), edge), 0.05, "Dicke wächst mit")
        XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: x, y: s.size.height - 4.5)), t.accent), 0.05, "darüber die Kappe")
    }

    // MARK: Taste: gesperrt und Trefferfläche

    func testDisabledKeyKeepsCapAndMutesLabel() {
        let t = ColorTheme.named("instrument-light")
        var muted = NSColor.clear
        withTheme(t) { muted = Theme.muted }
        let s = snapshot(Button("Starten") {}.buttonStyle(InstrumentKeyStyle(.primary)).disabled(true), theme: t)
        XCTAssertLessThan(distance(pixel(s, at: CGPoint(x: 3, y: s.size.height / 2)), t.accent), 0.05, "Kappe bleibt sichtbar")
        // Der Pixel der Beschriftung, der am weitesten von der Kappe abweicht, ist voll bedeckte Schrift.
        let ink = labelPixels(s).max { distance($0, t.accent) < distance($1, t.accent) }!
        XCTAssertLessThan(distance(ink, muted), 0.08, "Beschriftung in muted")
    }

    func testKeyCapMarginIsClickable() {
        // Rand der Kappe links neben der Schrift und die Dicke unten lösen die Taste aus, nicht nur die Schrift.
        let oldTheme = Theme.current, oldScale = Theme.scale
        defer { Theme.current = oldTheme; Theme.scale = oldScale }
        Theme.current = .named("instrument-light")
        Theme.scale = 1
        var presses = 0
        let host = NSHostingView(rootView: Button("Starten") { presses += 1 }.buttonStyle(InstrumentKeyStyle(.primary)))
        let size = host.fittingSize
        let window = NSWindow(contentRect: NSRect(origin: CGPoint(x: 200, y: 200), size: size), styleMask: [.borderless],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        window.contentView = host
        window.orderFront(nil)
        host.layout()
        click(host, at: CGPoint(x: 3, y: size.height / 2))
        XCTAssertEqual(presses, 1, "Kappenrand links")
        click(host, at: CGPoint(x: size.width / 2, y: size.height - 1))
        XCTAssertEqual(presses, 2, "Dicke unten")
    }

    func testWellLabelIsClickableBesideText() {
        // Kürzel-Feld der Einstellungen: `.plain`-Knopf, dessen Label ein Feld ist. Die gezeichnete Fläche nimmt keine
        // Klicks an (`hitTest` = nil), rechts neben der Schrift muss der Klick trotzdem treffen.
        var presses = 0
        let host = clickHost(Button { presses += 1 } label: {
            Text("⌥←").frame(width: 200, alignment: .leading).instrumentWell()
        }.buttonStyle(.plain))
        click(host, at: CGPoint(x: host.bounds.width - 20, y: host.bounds.height / 2))
        XCTAssertEqual(presses, 1, "leere Fläche des Felds")
    }

    func testUnselectedRowIsClickableAcrossWidth() {
        // Zeile einer Dialog-Liste ohne Auswahl hat keinen Grund; der Klick rechts neben dem Text trifft trotzdem.
        var taps = 0
        let host = clickHost(Text("Alpha").frame(width: 300, alignment: .leading).padding(.vertical, 4)
            .instrumentRow(selected: false).onTapGesture { taps += 1 })
        click(host, at: CGPoint(x: host.bounds.width - 20, y: host.bounds.height / 2))
        XCTAssertEqual(taps, 1, "leere Fläche der Zeile")
    }

    /// Hängt `view` bei Maßstab 1 in ein sichtbares Fenster, damit echte Mausereignisse ankommen.
    private func clickHost<V: View>(_ view: V) -> NSView {
        let oldScale = Theme.scale
        defer { Theme.scale = oldScale }
        Theme.scale = 1
        let host = NSHostingView(rootView: view)
        let size = host.fittingSize
        let window = NSWindow(contentRect: NSRect(origin: CGPoint(x: 200, y: 200), size: size), styleMask: [.borderless],
                              backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        addTeardownBlock { window.close() }
        window.contentView = host
        window.orderFront(nil)
        host.layout()
        return host
    }

    /// Pixel im Schriftbereich einer Taste (mittleres Drittel).
    private func labelPixels(_ s: (rep: NSBitmapImageRep, size: CGSize)) -> [NSColor] {
        let w = s.rep.pixelsWide, h = s.rep.pixelsHigh
        return (h / 4..<h / 2 + h / 8).flatMap { y in (w / 4..<w * 3 / 4).map { x in pixel(s.rep, x: x, y: y) } }
    }

    /// Klick (Maus runter und hoch) an `p` in Koordinaten des gespiegelten `host`.
    private func click(_ host: NSView, at p: CGPoint) {
        let window = host.window!, location = host.convert(p, to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = NSEvent.mouseEvent(with: type, location: location, modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                                           windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
            window.sendEvent(event)
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
    }
}
