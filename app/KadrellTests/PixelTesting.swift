import XCTest
import SwiftUI
@testable import Kadrell

/// Gemeinsame Helfer für Pixel-Tests: Zeichnen in ein gespiegeltes Bitmap (1 pt = 1 px bei Maßstab 1, y wächst nach
/// unten wie in den echten Views) und Farbvergleiche. Schema und Maßstab werden nur fürs Zeichnen gesetzt.
@MainActor
extension XCTestCase {
    func render(theme: ColorTheme, width: Int, height: Int, scale: CGFloat, _ draw: () -> Void) -> NSBitmapImageRep {
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

    func luma(_ c: NSColor) -> CGFloat {
        let s = c.usingColorSpace(.sRGB) ?? c
        return 0.2126 * s.redComponent + 0.7152 * s.greenComponent + 0.0722 * s.blueComponent
    }

    func luma(_ rep: NSBitmapImageRep, x: Int, y: Int) -> CGFloat { luma(rep.colorAt(x: x, y: y)!) }

    /// Farbe eines Pixels in sRGB, deckend.
    func pixel(_ rep: NSBitmapImageRep, x: Int, y: Int) -> NSColor {
        let c = rep.colorAt(x: x, y: y)!.usingColorSpace(.sRGB)!
        return NSColor(srgbRed: c.redComponent, green: c.greenComponent, blue: c.blueComponent, alpha: 1)
    }

    /// Größte Abweichung eines Farbkanals zwischen zwei Farben (sRGB).
    func distance(_ a: NSColor, _ b: NSColor) -> CGFloat {
        let a = a.usingColorSpace(.sRGB)!, b = b.usingColorSpace(.sRGB)!
        return max(abs(a.redComponent - b.redComponent), abs(a.greenComponent - b.greenComponent), abs(a.blueComponent - b.blueComponent))
    }

    /// Farbe, wie sie über `base` entsteht (Alpha über den Grund gerechnet).
    func over(_ c: NSColor, _ base: NSColor) -> NSColor {
        let s = c.usingColorSpace(.sRGB)!, b = base.usingColorSpace(.sRGB)!, a = s.alphaComponent
        return NSColor(srgbRed: s.redComponent * a + b.redComponent * (1 - a), green: s.greenComponent * a + b.greenComponent * (1 - a),
                       blue: s.blueComponent * a + b.blueComponent * (1 - a), alpha: 1)
    }

    func withTheme(_ t: ColorTheme, _ body: () -> Void) {
        let old = Theme.current
        defer { Theme.current = old }
        Theme.current = t
        body()
    }

    func panelCanvas(_ w: CGFloat, _ h: CGFloat) {
        Theme.panel.setFill()
        NSBezierPath(rect: CGRect(x: 0, y: 0, width: w, height: h)).fill()
    }

    /// Rendert eine SwiftUI-View in ihrer natürlichen Größe. Schema und Maßstab gelten nur fürs Rendern. Die View
    /// entsteht erst danach (`@autoclosure`), sonst lesen Modifier wie `raisedPanel()` noch den alten Maßstab.
    /// Native Schalter (`Toggle` als Switch) nehmen ihre Größe erst nach einem Durchlauf der Run-Loop an; bis dahin
    /// misst die Hosting-View zu hoch und zentriert den Inhalt mit leerem Rand oben und unten. Deshalb wird gemessen,
    /// bis die Größe steht.
    func snapshot<V: View>(_ view: @autoclosure () -> V, theme: ColorTheme, scale: CGFloat = 1) -> (rep: NSBitmapImageRep, size: CGSize) {
        let oldTheme = Theme.current, oldScale = Theme.scale
        defer { Theme.current = oldTheme; Theme.scale = oldScale }
        Theme.current = theme
        Theme.scale = scale
        let host = NSHostingView(rootView: view())
        host.layout()
        var size = CGSize.zero
        for _ in 0..<5 where host.fittingSize != size {
            size = host.fittingSize
            host.frame = NSRect(origin: .zero, size: size)
            host.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date() + 0.05)
        }
        let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds)!
        host.cacheDisplay(in: host.bounds, to: rep)
        return (rep, size)
    }

    /// Farbe eines Snapshots am Punkt `p` (Punkte, y nach unten), in sRGB, deckend.
    func pixel(_ s: (rep: NSBitmapImageRep, size: CGSize), at p: CGPoint) -> NSColor {
        let k = CGFloat(s.rep.pixelsWide) / s.size.width
        return pixel(s.rep, x: Int(p.x * k), y: Int(p.y * k))
    }

    // MARK: Dialoge auf der erhabenen Fläche

    /// Im Bild kommt die Akzent-Textfarbe vor, die Akzent-Füllfarbe nicht.
    func assertAccentTextOnly<V: View>(_ view: V, line: UInt = #line) {
        let t = ColorTheme.named("solarized-light")
        XCTAssertNotEqual(t.accentText.hexString, t.accent.hexString, "Schema trennt Text- und Füllfarbe", line: line)
        let s = snapshot(view.raisedPanel(), theme: t, scale: 1)
        var text = 0, fill = 0
        for y in 0..<s.rep.pixelsHigh {
            for x in 0..<s.rep.pixelsWide {
                let c = pixel(s.rep, x: x, y: y)
                if distance(c, t.accentText) < 0.04 { text += 1 }
                if distance(c, t.accent) < 0.04 { fill += 1 }
            }
        }
        XCTAssertGreaterThan(text, 0, "Akzent-Textfarbe kommt vor", line: line)
        XCTAssertEqual(fill, 0, "Akzent-Füllfarbe nicht im Text", line: line)
    }

    /// Rendert `view` auf der erhabenen Fläche in Instrument Light (dort sind `raised`, `bg` und `panel` verschieden)
    /// und prüft, dass am Punkt die Fläche selbst zu sehen ist, kein eigener Grund des Dialogs.
    func assertOnRaised<V: View>(_ view: V, at point: (CGSize) -> CGPoint, file: StaticString = #filePath, line: UInt = #line) {
        let t = ColorTheme.named("instrument-light")
        let s = snapshot(view.raisedPanel(), theme: t, scale: 1)
        let p = point(s.size)
        XCTAssertLessThan(distance(pixel(s, at: p), t.raised), 0.02, "erhabene Fläche bei \(p)", file: file, line: line)
    }

    /// Teile aller Instrument-Flächen, die `view` im Hosting anlegt.
    func surfaceParts<V: View>(_ view: V) -> [InstrumentSurfaceView.Part] {
        hostedSurfaces(view).surfaces.map(\.part)
    }

    /// Hosting-View bei Maßstab 1 in natürlicher Größe samt aller Instrument-Flächen darin (für Lage und Teil).
    func hostedSurfaces<V: View>(_ view: V) -> (host: NSView, surfaces: [InstrumentSurfaceView]) {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let host = NSHostingView(rootView: view)
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date() + 0.05)
        host.layoutSubtreeIfNeeded()
        return (host, surfaces(in: host))
    }

    func surfaces(in view: NSView) -> [InstrumentSurfaceView] {
        view.subviews.flatMap { sub in (sub as? InstrumentSurfaceView).map { [$0] } ?? surfaces(in: sub) }
    }
}

extension InstrumentSurfaceView.Part {
    var isWell: Bool { if case .well = self { true } else { false } }
}
