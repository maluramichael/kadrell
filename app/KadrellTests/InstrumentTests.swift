import XCTest
@testable import Kadrell

/// Bauteile des Instrument-Looks: Fuge, Taste, Display. Gezeichnet wird in ein gespiegeltes Bitmap (1 pt = 1 px,
/// y wächst nach unten wie in den echten Views), und zwar über `Theme.scaled`, damit der Maßstab wie in der App wirkt.
@MainActor
final class InstrumentTests: XCTestCase {
    private func render(theme: ColorTheme, width: Int, height: Int, scale: CGFloat, _ draw: () -> Void) -> NSBitmapImageRep {
        let oldTheme = Theme.current, oldScale = Theme.scale
        defer { Theme.current = oldTheme; Theme.scale = oldScale }
        Theme.current = theme
        Theme.scale = scale
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height, bitsPerSample: 8, samplesPerPixel: 4,
                                   hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        let cg = NSGraphicsContext(bitmapImageRep: rep)!.cgContext
        cg.translateBy(x: 0, y: CGFloat(height))
        cg.scaleBy(x: 1, y: -1)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: cg, flipped: true)
        Theme.scaled(CGRect(x: 0, y: 0, width: width, height: height)) { _ in draw() }
        NSGraphicsContext.restoreGraphicsState()
        return rep
    }

    private func luma(_ c: NSColor) -> CGFloat {
        let s = c.usingColorSpace(.sRGB) ?? c
        return 0.2126 * s.redComponent + 0.7152 * s.greenComponent + 0.0722 * s.blueComponent
    }

    private func luma(_ rep: NSBitmapImageRep, x: Int, y: Int) -> CGFloat { luma(rep.colorAt(x: x, y: y)!) }

    /// Zeilen, in denen mindestens ein Pixel merklich von `color` abweicht.
    private func changedRows(_ rep: NSBitmapImageRep, against color: NSColor) -> [Int] {
        let ref = luma(color)
        return (0..<rep.pixelsHigh).filter { y in (0..<rep.pixelsWide).contains { abs(luma(rep, x: $0, y: y) - ref) > 0.01 } }
    }

    /// Fugenfarbe, wie sie über `base` entsteht (Alpha über den Grund gerechnet).
    private func over(_ c: NSColor, _ base: NSColor) -> NSColor {
        let s = c.usingColorSpace(.sRGB)!, b = base.usingColorSpace(.sRGB)!, a = s.alphaComponent
        return NSColor(srgbRed: s.redComponent * a + b.redComponent * (1 - a), green: s.greenComponent * a + b.greenComponent * (1 - a),
                       blue: s.blueComponent * a + b.blueComponent * (1 - a), alpha: 1)
    }

    private func withTheme(_ t: ColorTheme, _ body: () -> Void) {
        let old = Theme.current
        defer { Theme.current = old }
        Theme.current = t
        body()
    }

    private func panelCanvas(_ w: CGFloat, _ h: CGFloat) {
        Theme.panel.setFill()
        NSBezierPath(rect: CGRect(x: 0, y: 0, width: w, height: h)).fill()
    }

    func testSeamDrawsDarkThenLight() {
        for t in ColorTheme.all {
            let rep = render(theme: t, width: 20, height: 20, scale: 1) {
                panelCanvas(20, 20)
                Instrument.hSeam(x: 0, y: 10, width: 20)
            }
            let panel = luma(rep, x: 5, y: 0)
            XCTAssertLessThan(luma(rep, x: 5, y: 10), panel, t.id)
            XCTAssertGreaterThan(luma(rep, x: 5, y: 11), panel, t.id)
            XCTAssertEqual(luma(rep, x: 5, y: 9), panel, accuracy: 0.005, t.id)
            XCTAssertEqual(luma(rep, x: 5, y: 12), panel, accuracy: 0.005, t.id)
        }
    }

    func testVerticalSeamDrawsDarkThenLight() {
        for t in ColorTheme.all {
            let rep = render(theme: t, width: 20, height: 20, scale: 1) {
                panelCanvas(20, 20)
                Instrument.vSeam(x: 10, y: 0, height: 20)
            }
            let panel = luma(rep, x: 0, y: 5)
            XCTAssertLessThan(luma(rep, x: 10, y: 5), panel, t.id)
            XCTAssertGreaterThan(luma(rep, x: 11, y: 5), panel, t.id)
        }
    }

    func testSeamStaysOnePointAtScale() {
        let t = ColorTheme.named("instrument-dark")
        for scale in [0.9, 1.0, 1.1, 1.3] as [CGFloat] {
            let rep = render(theme: t, width: 20, height: 20, scale: scale) {
                panelCanvas(20 / scale, 20 / scale)
                Instrument.hSeam(x: 0, y: 5, width: 15)
            }
            let rows = changedRows(rep, against: t.panel)
            XCTAssertEqual(rows.count, 2, "Maßstab \(scale): genau zwei Zeilen")
            XCTAssertEqual(rows.last.map { $0 - rows[0] }, 1, "Maßstab \(scale): dunkle und helle Zeile grenzen aneinander")
            if let first = rows.first {
                XCTAssertLessThan(luma(rep, x: 3, y: first), luma(t.panel), "Maßstab \(scale): oben dunkel")
                XCTAssertGreaterThan(luma(rep, x: 3, y: first + 1), luma(t.panel), "Maßstab \(scale): unten hell")
            }
        }
    }

    func testSeamVisibleInEveryTheme() {
        // Die dunkle Zeile hebt sich vom Grund ab, die helle von der dunklen; die helle Zeile ist nie dunkler als der Grund.
        for t in ColorTheme.all {
            withTheme(t) {
                let dark = over(Instrument.seamDark, t.panel), light = over(Instrument.seamLight, t.panel)
                XCTAssertGreaterThanOrEqual(dark.contrastRatio(with: t.panel), 1.05, t.id)
                XCTAssertGreaterThanOrEqual(light.contrastRatio(with: dark), 1.15, t.id)
                XCTAssertGreaterThanOrEqual(luma(light), luma(t.panel), t.id)
            }
        }
    }

    func testSeamColorsFollowDarkAndLight() {
        for t in ColorTheme.all {
            withTheme(t) {
                XCTAssertEqual(Instrument.seamDark.alphaComponent, t.dark ? 0.6 : 0.13, accuracy: 0.001, t.id)
                XCTAssertEqual(Instrument.seamLight.alphaComponent, t.dark ? 0.05 : 0.65, accuracy: 0.001, t.id)
            }
        }
    }

    func testPressedKeySinks() {
        let r = CGRect(x: 0, y: 0, width: 40, height: 20)
        _ = render(theme: .named("instrument-dark"), width: 40, height: 20, scale: 1) {
            let up = Instrument.key(r, pressed: false, fill: nil)
            let down = Instrument.key(r, pressed: true, fill: nil)
            XCTAssertEqual(down.minY, up.minY + 1)
            XCTAssertEqual(up.height, r.height - 2, "Kappe ohne die 2 pt Dicke")
            XCTAssertEqual(down.height, up.height, "gleiche Kappe, nur tiefer")
        }
    }

    func testKeyUsesFillAndEdge() {
        let t = ColorTheme.named("instrument-dark")
        let fill = NSColor(hex: 0xff0000)
        var edge = NSColor.clear
        let rep = render(theme: t, width: 40, height: 20, scale: 1) {
            panelCanvas(40, 20)
            _ = Instrument.key(CGRect(x: 0, y: 0, width: 40, height: 20), pressed: false, fill: fill)
            edge = Instrument.keyEdge
        }
        XCTAssertEqual(luma(rep, x: 20, y: 8), luma(fill), accuracy: 0.02, "Kappe trägt die Füllung")
        XCTAssertEqual(luma(rep, x: 20, y: 19), luma(edge), accuracy: 0.02, "Dicke unten in keyEdge")
    }

    func testScreenIsBlackInEveryTheme() {
        for t in ColorTheme.all {
            withTheme(t) {
                XCTAssertGreaterThanOrEqual(Instrument.screenInk.contrastRatio(with: Instrument.screen), 4.5, t.id)
                XCTAssertGreaterThanOrEqual(Instrument.screenMuted.contrastRatio(with: Instrument.screen), 4.5, t.id)
                XCTAssertEqual(Instrument.screen.hexString, t.dark ? "#050505" : "#0d0d0c", t.id)
                XCTAssertEqual(Instrument.screenInk.hexString, "#efe9dc", t.id)
                XCTAssertEqual(Instrument.screenMuted.hexString, "#8f8b81", t.id)
            }
        }
    }

    func testScreenDrawsFaceWithTopHighlight() {
        let t = ColorTheme.named("instrument-dark")
        let face = luma(NSColor(hex: 0x050505))
        let rep = render(theme: t, width: 40, height: 20, scale: 1) {
            panelCanvas(40, 20)
            Instrument.screen(CGRect(x: 0, y: 0, width: 40, height: 20))
        }
        XCTAssertEqual(luma(rep, x: 20, y: 10), face, accuracy: 0.01)
        XCTAssertGreaterThan(luma(rep, x: 20, y: 0), luma(rep, x: 20, y: 10), "oben liegt die helle Haarlinie")
        XCTAssertGreaterThan(luma(rep, x: 0, y: 0), face + 0.02, "Ecke ist abgerundet, dort scheint der Grund durch")
    }
}
