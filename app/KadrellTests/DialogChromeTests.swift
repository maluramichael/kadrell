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
    /// ohnehin alles im aktuellen Schema), `needsDisplay` auch nicht (AppKit setzt es zurück). Deshalb ein Wächter:
    /// jede Fläche bekommt ein fremdes `part`, und nur `updateNSView` setzt das echte zurück. Dunkel nach dunkel, damit
    /// nicht schon der Wechsel der Appearance alles neu aufbaut.
    func testThemeChangeReachesEverySurface() {
        let saved = Theme.current; defer { Theme.current = saved }
        Theme.current = .named("instrument-dark")
        let p = OverlayPanel(rootView: VStack { Text("x"); DialogFoot(hint: "h", button: "OK") {} }.frame(width: 400))
        let view = p.contentViewController!.view
        settle(view)
        let surfaces = instrumentSurfaces(in: view)
        XCTAssertEqual(Set(surfaces.map { "\($0.part)" }).count, 3, "Fläche, Fuge und Taste")
        let sentinel = InstrumentSurfaceView.Part.well(focused: true)
        surfaces.forEach { $0.part = sentinel }
        settle(view)
        XCTAssertTrue(surfaces.allSatisfy { $0.part == sentinel }, "ohne Schemawechsel bleibt der Wächter stehen")
        Theme.current = .named("mocha")
        p.applyTheme()
        settle(view)
        XCTAssertTrue(surfaces.allSatisfy { $0.part != sentinel }, "jede Fläche zeichnet im neuen Schema")
    }

    func testScrollLimitLeavesRoomForShadow() {
        let limit = OverlayLimit(); limit.maxHeight = 300
        let host = NSHostingView(rootView: OverlayScroll(limit: limit, content: Color.clear.frame(width: 200, height: 1000)))
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date() + 0.05)   // onGeometryChange setzt die Höhe erst im zweiten Durchlauf
        host.layoutSubtreeIfNeeded()
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
