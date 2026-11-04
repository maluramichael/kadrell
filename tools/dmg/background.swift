// Hintergrund des DMG-Fensters, 660 x 420 pt. Aufruf: swift tools/dmg/background.swift <version> <out.png> [maßstab]
// tools/release.sh rendert 1x und @2x, dmgbuild fügt beide zu einem TIFF für Retina zusammen.
import AppKit

let version = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "dev"
let out = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : "dmg-background.png"
let W: CGFloat = 660, H: CGFloat = 420
let S = CGFloat(CommandLine.arguments.count > 3 ? Double(CommandLine.arguments[3]) ?? 1 : 1)
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(W * S), pixelsHigh: Int(H * S), bitsPerSample: 8,
                           samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
rep.size = NSSize(width: W, height: H)
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let cg = NSGraphicsContext.current!.cgContext
func c(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
    CGColor(srgbRed: CGFloat(hex >> 16 & 255) / 255, green: CGFloat(hex >> 8 & 255) / 255, blue: CGFloat(hex & 255) / 255, alpha: a)
}
let space = CGColorSpace(name: CGColorSpace.sRGB)!
// Grund: tiefes Nachtblau wie das Icon, nach unten dunkler.
cg.drawLinearGradient(CGGradient(colorsSpace: space, colors: [c(0x23263A), c(0x12131F)] as CFArray, locations: [0, 1])!,
                      start: CGPoint(x: 0, y: H), end: CGPoint(x: 0, y: 0), options: [])
// Kachel-Gitter im Hintergrund, wie die Terminals im Icon: feine Rahmen, bunte Zeilen, kaum sichtbar.
let accents: [UInt32] = [0x7AA2F7, 0xBB9AF7, 0x9ECE6A, 0xE0AF68, 0xF7768E, 0x7DCFFF]
var seed: UInt64 = 7
func rnd() -> CGFloat { seed = seed &* 6364136223846793005 &+ 1442695040888963407; return CGFloat(seed >> 33) / CGFloat(1 << 31) }
let cols = 6, rows = 4, gap: CGFloat = 10, tw = (W - gap * CGFloat(cols + 1)) / CGFloat(cols), th = (H - gap * CGFloat(rows + 1)) / CGFloat(rows)
for r in 0..<rows { for k in 0..<cols {
    let rect = CGRect(x: gap + CGFloat(k) * (tw + gap), y: gap + CGFloat(r) * (th + gap), width: tw, height: th)
    cg.setStrokeColor(c(0xFFFFFF, 0.045)); cg.setLineWidth(1)
    cg.addPath(CGPath(roundedRect: rect, cornerWidth: 8, cornerHeight: 8, transform: nil)); cg.strokePath()
    var y = rect.maxY - 14
    while y > rect.minY + 8 {
        let w = (0.25 + rnd() * 0.6) * (tw - 20)
        cg.setFillColor(c(accents[Int(rnd() * 6) % 6], 0.07 + rnd() * 0.05))
        cg.fill(CGRect(x: rect.minX + 10, y: y, width: w, height: 3)); y -= 9
    }
}}
// Lichtkegel hinter den beiden Icons.
for x in [CGFloat(170), 490] {  // Icon-Mitten, siehe icon_locations in settings.py
    cg.drawRadialGradient(CGGradient(colorsSpace: space, colors: [c(0x7AA2F7, 0.22), c(0x7AA2F7, 0)] as CFArray, locations: [0, 1])!,
                          startCenter: CGPoint(x: x, y: 195), startRadius: 0, endCenter: CGPoint(x: x, y: 195), endRadius: 120, options: [])
}
// Glaskarte unter Icons und Beschriftung: mittlerer Ton, damit Finder-Labels hell wie dunkel lesbar bleiben.
let card = CGRect(x: 40, y: 92, width: W - 80, height: 220)
cg.addPath(CGPath(roundedRect: card, cornerWidth: 18, cornerHeight: 18, transform: nil))
cg.setFillColor(c(0x767C99, 0.92)); cg.fillPath()
cg.addPath(CGPath(roundedRect: card.insetBy(dx: 0.5, dy: 0.5), cornerWidth: 18, cornerHeight: 18, transform: nil))
cg.setStrokeColor(c(0xFFFFFF, 0.25)); cg.strokePath()
// Drei Winkel von Kadrell nach Applications, nach rechts größer: liest sich wie eine Bewegung. Runde Enden,
// Verlauf durch die Akzentfarben, weicher Schein.
let ay: CGFloat = 195, ax0: CGFloat = 300, ax1: CGFloat = 380
let arrow = CGMutablePath()
for (i, x) in [CGFloat(300), 332, 364].enumerated() {
    let h = 10 + CGFloat(i) * 3
    arrow.move(to: CGPoint(x: x, y: ay + h)); arrow.addLine(to: CGPoint(x: x + h, y: ay)); arrow.addLine(to: CGPoint(x: x, y: ay - h))
}
let stroked = arrow.copy(strokingWithWidth: 4.5, lineCap: .round, lineJoin: .round, miterLimit: 1)
let grad = CGGradient(colorsSpace: space, colors: [c(0x7DCFFF), c(0xBB9AF7), c(0xF7768E)] as CFArray, locations: [0, 0.5, 1])!
cg.saveGState()
cg.setShadow(offset: .zero, blur: 14, color: c(0xBB9AF7, 0.8))
cg.addPath(stroked); cg.setFillColor(c(0xBB9AF7, 0.6)); cg.fillPath()
cg.restoreGState()
cg.saveGState()
cg.addPath(stroked); cg.clip()
cg.drawLinearGradient(grad, start: CGPoint(x: ax0, y: 0), end: CGPoint(x: ax1, y: 0), options: [.drawsBeforeStartLocation, .drawsAfterEndLocation])
cg.restoreGState()
// Texte, Mono wie in der App.
func text(_ s: String, _ size: CGFloat, _ color: CGColor, at p: CGPoint, kern: CGFloat = 0, weight: NSFont.Weight = .regular, center: Bool = false) {
    let a = NSAttributedString(string: s, attributes: [.font: NSFont.monospacedSystemFont(ofSize: size, weight: weight),
                                                     .foregroundColor: NSColor(cgColor: color)!, .kern: kern])
    a.draw(at: CGPoint(x: center ? p.x - a.size().width / 2 : p.x, y: p.y))
}
text("KADRELL", 22, c(0xFFFFFF), at: CGPoint(x: 42, y: 360), kern: 6, weight: .semibold)
text(version, 12, c(0x7AA2F7), at: CGPoint(x: 44 + 22 * 7 * 0.6 + 6 * 7 + 10, y: 366), weight: .medium)
text("alle Claude-Sessions · ein Fenster", 11.5, c(0xA9B1D6), at: CGPoint(x: 42, y: 338))
text("IN PROGRAMME ZIEHEN · DRAG TO APPLICATIONS", 10, c(0xFFFFFF, 0.9), at: CGPoint(x: W / 2, y: 274), kern: 1.5, weight: .medium, center: true)
text("kadrell.malura.de", 10, c(0x565F89), at: CGPoint(x: W / 2, y: 40), center: true)
NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
