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
    func snapshot<V: View>(_ view: @autoclosure () -> V, theme: ColorTheme, scale: CGFloat = 1) -> (rep: NSBitmapImageRep, size: CGSize) {
        let oldTheme = Theme.current, oldScale = Theme.scale
        defer { Theme.current = oldTheme; Theme.scale = oldScale }
        Theme.current = theme
        Theme.scale = scale
        let host = NSHostingView(rootView: view())
        host.layout()
        let size = host.fittingSize
        host.frame = NSRect(origin: .zero, size: size)
        host.layout()
        let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds)!
        host.cacheDisplay(in: host.bounds, to: rep)
        return (rep, size)
    }

    /// Farbe eines Snapshots am Punkt `p` (Punkte, y nach unten), in sRGB, deckend.
    func pixel(_ s: (rep: NSBitmapImageRep, size: CGSize), at p: CGPoint) -> NSColor {
        let k = CGFloat(s.rep.pixelsWide) / s.size.width
        return pixel(s.rep, x: Int(p.x * k), y: Int(p.y * k))
    }
}
