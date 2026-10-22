import XCTest
import SwiftUI
@testable import Kadrell

/// Gemeinsame Hülle aller SwiftUI-Dialoge: klares Fenster ohne Systemschatten, erhabene Fläche aus `OverlayScroll`,
/// Fußleiste mit Fuge und Primärtaste, Neuzeichnen beim Schemawechsel.
@MainActor
final class DialogChromeTests: XCTestCase {
    func testOverlayPanelIsClearWithoutSystemShadow() {
        let p = OverlayPanel(rootView: Text("x"))
        XCTAssertFalse(p.isOpaque); XCTAssertEqual(p.backgroundColor, .clear); XCTAssertFalse(p.hasShadow)
        XCTAssertFalse(PaletteWindow().hasShadow)
    }

    func testThemeChangeRedrawsOverlaySurface() {
        let saved = Theme.current; defer { Theme.current = saved }
        let p = OverlayPanel(rootView: Text("x"))
        let before = p.limit.themeGeneration
        Theme.current = .named("github-light")
        p.applyTheme()
        XCTAssertEqual(p.limit.themeGeneration, before + 1)
    }

    /// Wie Review Focus 1: Schema wechselt bei offenem Dialog, jede Instrument-Fläche der Hülle (Fläche, Fuge und
    /// Taste der Fußleiste) bekommt `updateNSView` und zeichnet neu. Ein Bild sähe das nicht (`cacheDisplay` zeichnet
    /// ohnehin alles im aktuellen Schema), `needsDisplay` auch nicht (AppKit setzt es zurück), deshalb der Wächter.
    /// Dunkel nach dunkel, damit nicht schon der Wechsel der Appearance alles neu aufbaut.
    func testThemeChangeReachesEverySurface() {
        let saved = Theme.current; defer { Theme.current = saved }
        Theme.current = .named("instrument-dark")
        let p = OverlayPanel(rootView: VStack { Text("x"); DialogFoot(hint: "h", button: "OK") {} }.frame(width: 400))
        assertChangeReachesEverySurface(p) { Theme.current = .named("mocha") }
    }

    /// UI-Größe ändert sich bei offenem Dialog (Einstellungen): jede Fläche zeichnet neu, Schattenpolster und
    /// Scrollhöhe rechnen mit dem neuen Maßstab, der Dialog samt Schatten bleibt in `maxHeight`.
    func testScaleChangeReachesOpenDialog() throws {
        let savedTheme = Theme.current, savedScale = Theme.scale
        defer { Theme.current = savedTheme; Theme.scale = savedScale }
        Theme.current = .named("instrument-dark")
        Theme.scale = 1
        let p = OverlayPanel(rootView: VStack { Color.clear.frame(height: 1000); DialogFoot(hint: "h", button: "OK") {} }.frame(width: 400))
        p.limit.maxHeight = 300
        assertChangeReachesEverySurface(p) { Theme.scale = 1.3 }
        let view = p.contentViewController!.view
        settle(view)
        XCTAssertLessThanOrEqual(view.fittingSize.height, 300.5, "Dialog samt Schatten passt in maxHeight")
        // Unter der Taste: 10 pt Polster der Fußleiste plus Schattentiefe im neuen Maßstab (3 × 1,3).
        let surfaces = instrumentSurfaces(in: view)
        let raised = try XCTUnwrap(surfaces.first { $0.part == .raised })
        let key = try XCTUnwrap(surfaces.first { if case .key = $0.part { true } else { false } })
        let bottom = { (v: NSView) in let r = v.convert(v.bounds, to: view); return view.isFlipped ? r.maxY : -r.minY }
        XCTAssertEqual(bottom(raised) - bottom(key), 10 + Instrument.shadowDepth * 1.3, accuracy: 0.3)
    }

    func testScrollLimitLeavesRoomForShadow() {
        let limit = OverlayLimit(); limit.maxHeight = 300
        let host = NSHostingView(rootView: OverlayScroll(limit: limit, content: Color.clear.frame(width: 200, height: 1000)))
        settle(host)
        XCTAssertLessThanOrEqual(host.fittingSize.height, 300.5)
    }

    /// Wie Review Focus 2: 130 % mit Fußleiste, der ganze Dialog samt Schatten bleibt in `maxHeight`.
    func testScrollLimitLeavesRoomForShadowAtScaleWithFoot() {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1.3
        let limit = OverlayLimit(); limit.maxHeight = 300
        let host = NSHostingView(rootView: OverlayScroll(limit: limit, content: VStack {
            Color.clear.frame(width: 200, height: 1000)
            DialogFoot(hint: "h", button: "OK") {}
        }))
        settle(host)
        settle(host)   // Höhe der Fußleiste kommt erst nach der Präferenz an
        XCTAssertGreaterThan(host.fittingSize.height, 250, "Inhalt füllt die Höhe, die Grenze greift")
        XCTAssertLessThanOrEqual(host.fittingSize.height, 300.5)
    }

    func testFootBarKeyIsPrimary() {
        // Dialog mit Fußleiste in Instrument Light: im unteren Streifen gibt es Pixel in accent (Primärtaste).
        let t = ColorTheme.named("instrument-light")
        let s = snapshot(OverlayScroll(limit: OverlayLimit(), content: VStack { Text("x"); DialogFoot(hint: "h", button: "OK") {} }.frame(width: 400)), theme: t)
        let hits = stride(from: 200.0, to: 395, by: 2).contains { x in distance(pixel(s, at: CGPoint(x: x, y: s.size.height - 22)), t.accent) < 0.05 }
        XCTAssertTrue(hits)
        // Die Fußleiste malt keinen eigenen Grund: die untere linke Ecke bleibt durch Radius 14 der Hülle frei.
        XCTAssertLessThan(s.rep.colorAt(x: 0, y: s.rep.pixelsHigh - 1)!.alphaComponent, 0.05)
    }

    func testPaletteChromeUsesInstrumentParts() throws {
        let p = PaletteWindow()
        let parts = try XCTUnwrap(p.contentView).subviews.compactMap { ($0 as? InstrumentSurfaceView)?.part }
        XCTAssertEqual(parts.first, .raised)
        XCTAssertEqual(parts.filter { $0 == .hSeam }.count, 2)
        XCTAssertTrue(parts.contains(.well(focused: false)))
    }

    /// Fenster ist um die Schattentiefe höher, Fläche und Feld liegen darin, die Fugen behalten ihre Lage zur Fläche.
    func testPaletteWindowLeavesRoomForShadow() throws {
        let saved = Theme.scale; defer { Theme.scale = saved }
        for s in [1.0, 1.3] as [CGFloat] {
            Theme.scale = s
            let p = PaletteWindow()
            let root = try XCTUnwrap(p.contentView)
            let d = (Instrument.shadowDepth * s).rounded()
            XCTAssertEqual(p.frame.height, (420 * s).rounded() + d)
            XCTAssertEqual(root.subviews.first?.frame, root.bounds)
            let seams = root.subviews.filter { ($0 as? InstrumentSurfaceView)?.part == .hSeam }
            XCTAssertEqual(Set(seams.map(\.frame.minY)), [p.frame.height - 56 * s, 34 * s + d])
            XCTAssertTrue(seams.allSatisfy { $0.frame.height == 2 })
        }
    }

    /// Die Palette baut sich bei Schema-, Sprach- und Größenwechsel neu (`applyAppearance`), Flächen und Zeilen lesen
    /// das Schema beim Zeichnen: ein neues Fenster trägt gleiche Teile, und eine Zeile zeichnet im neuen Schema.
    func testPaletteRowFollowsThemeChange() {
        let row = paletteRow(selected: true)
        let dark = ColorTheme.named("instrument-dark"), light = ColorTheme.named("github-light")
        let a = render(theme: dark, width: 400, height: 44, scale: 1) { dark.raised.setFill(); NSBezierPath(rect: row.bounds).fill(); row.draw(row.bounds) }
        let b = render(theme: light, width: 400, height: 44, scale: 1) { light.raised.setFill(); NSBezierPath(rect: row.bounds).fill(); row.draw(row.bounds) }
        XCTAssertLessThan(distance(pixel(a, x: 380, y: 40), dark.bg), 0.02)
        XCTAssertLessThan(distance(pixel(b, x: 380, y: 40), light.bg), 0.02)
        XCTAssertLessThan(distance(pixel(b, x: 6, y: 22), light.fg), 0.15)
    }

    /// Ausgewählte Zeile: kein Akzentbalken links mehr, Fläche in `bg` mit 1,5-pt-Umriss in `fg` (Zeilenfläche rückt
    /// 6 pt vom Rand ein, der Umriss belegt Pixel 6 voll).
    func testPaletteSelectionIsOutlineNotBar() {
        let t = ColorTheme.named("instrument-dark")
        let row = paletteRow(selected: true)
        let rep = render(theme: t, width: 400, height: 44, scale: 1) {
            t.raised.setFill(); NSBezierPath(rect: row.bounds).fill()
            row.draw(row.bounds)
        }
        XCTAssertLessThan(distance(pixel(rep, x: 1, y: 22), t.raised), 0.02, "kein Akzentbalken")
        XCTAssertGreaterThan(distance(pixel(rep, x: 1, y: 22), t.accent), 0.1)
        XCTAssertLessThan(distance(pixel(rep, x: 6, y: 22), t.fg), 0.15, "Umriss")
        XCTAssertLessThan(distance(pixel(rep, x: 380, y: 40), t.bg), 0.02, "Fläche")
        XCTAssertLessThan(distance(pixel(rep, x: 200, y: 0), t.raised), 0.02, "Luft über der Fläche")
        // Nicht ausgewählt bleibt die Zeile blank.
        let plain = paletteRow(selected: false)
        let rep2 = render(theme: t, width: 400, height: 44, scale: 1) {
            t.raised.setFill(); NSBezierPath(rect: plain.bounds).fill()
            plain.draw(plain.bounds)
        }
        XCTAssertLessThan(distance(pixel(rep2, x: 6, y: 22), t.raised), 0.02)
        XCTAssertLessThan(distance(pixel(rep2, x: 380, y: 40), t.raised), 0.02)
    }

    private func paletteRow(selected: Bool) -> PaletteRow {
        let row = PaletteRow(frame: NSRect(x: 0, y: 0, width: 400, height: 44))
        row.item = PaletteWindow.Item(label: "Alpha", sub: "", group: nil, status: nil, sessionKey: nil, run: {})
        row.selected = selected
        return row
    }

    /// Wächter-Prüfung: jede Fläche bekommt ein fremdes `part`, nur `updateNSView` setzt das echte zurück. Eine
    /// Kontrollrunde ohne Änderung zeigt, dass nichts anderes den Wächter entfernt.
    private func assertChangeReachesEverySurface(_ p: OverlayPanel, change: () -> Void, line: UInt = #line) {
        let view = p.contentViewController!.view
        settle(view)
        let surfaces = instrumentSurfaces(in: view)
        XCTAssertEqual(Set(surfaces.map { "\($0.part)" }).count, 3, "Fläche, Fuge und Taste", line: line)
        let sentinel = InstrumentSurfaceView.Part.well(focused: true)
        surfaces.forEach { $0.part = sentinel }
        settle(view)
        XCTAssertTrue(surfaces.allSatisfy { $0.part == sentinel }, "ohne Änderung bleibt der Wächter stehen", line: line)
        change()
        p.applyTheme()
        settle(view)
        XCTAssertTrue(surfaces.allSatisfy { $0.part != sentinel }, "jede Fläche zeichnet neu", line: line)
    }

    /// Layout plus ein Durchlauf der RunLoop: Präferenzen (Fußleiste) und Geometrie kommen erst dann an.
    private func settle(_ view: NSView) {
        view.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date() + 0.05)
        view.layoutSubtreeIfNeeded()
    }

    private func instrumentSurfaces(in view: NSView) -> [InstrumentSurfaceView] {
        view.subviews.flatMap { sub in (sub as? InstrumentSurfaceView).map { [$0] } ?? instrumentSurfaces(in: sub) }
    }
}
