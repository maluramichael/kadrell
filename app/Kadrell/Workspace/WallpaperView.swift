import AppKit

/// Grund hinter Baum und Arbeitsfläche: Farbe des Themes, darüber das Hintergrundbild füllend über die ganze Fläche (ohne Balken).
/// Baum und Kacheln liegen mit ihrer Deckkraft darüber, unter 100 % scheint das Bild durch.
final class WallpaperView: NSView {
    /// Bild, einmal auf die Größe der Fläche gerechnet: Neuzeichnen soll es nur kopieren.
    private var cache: (path: String, size: CGSize, image: CGImage?)?

    override func draw(_ dirtyRect: NSRect) {
        Theme.bg.setFill()
        dirtyRect.fill()
        let path = Settings.backgroundImage, px = convertToBacking(bounds).size
        guard !path.isEmpty, px.width >= 1, px.height >= 1 else { return }
        if cache?.path != path || cache?.size != px { cache = (path, px, Self.filled(path, px)) }
        guard let img = cache?.image, let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.draw(img, in: bounds)
    }

    /// „Aspect fill“: die kürzere Seite passt, der Rest wird mittig abgeschnitten.
    private static func filled(_ path: String, _ px: CGSize) -> CGImage? {
        guard let src = NSImage(contentsOfFile: path)?.cgImage(forProposedRect: nil, context: nil, hints: nil),
              let ctx = CGContext(data: nil, width: Int(px.width), height: Int(px.height), bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) else { return nil }
        let s = max(px.width / CGFloat(src.width), px.height / CGFloat(src.height))
        let w = CGFloat(src.width) * s, h = CGFloat(src.height) * s
        ctx.interpolationQuality = .high
        ctx.draw(src, in: CGRect(x: (px.width - w) / 2, y: (px.height - h) / 2, width: w, height: h))
        return ctx.makeImage()
    }
}
