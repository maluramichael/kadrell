import AppKit

/// Runde Ecke oben links an der Arbeitsfläche. Liegt im Fenster über Leiste und Split, 2 pt links und oberhalb der
/// Arbeitsfläche, also genau auf den geraden Fugen von Trenner und Leiste, und führt sie als Viertelbogen um die Ecke:
/// außen dunkel, innen hell, je 1 pt, wie die geraden Fugen. Außerhalb des Bogens Gehäuse wie der Baum, innerhalb
/// durchsichtig. Lässt jeden Klick durch.
final class ModuleCornerView: NSView {
    /// Radius bis zur Innenkante der Fuge, in logischen Punkten. In der App mal `Theme.scale` und auf ganze Punkte
    /// gerundet, sonst hat die Layer-Fläche Bruchteile von Pixeln und zeichnet sich als Viereck ab.
    static let radius: CGFloat = 14
    /// Breite der Fuge (dunkel + hell) am Rand der View.
    private static let seam: CGFloat = 2

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { false }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// Mittelpunkt C = (2 + r, 2 + r), Abstand d davon: d < r durchsichtig, r bis r + 1 hell, r + 1 bis r + 2 dunkel,
    /// darüber Gehäuse. Unter den Fugenstreifen am Rand liegt deckendes Panel wie in Leiste und Trenner, damit die
    /// geraden Fugen darunter nicht doppelt durchscheinen.
    override func draw(_ dirtyRect: NSRect) {
        let r = min(bounds.width, bounds.height) - Self.seam
        let c = CGPoint(x: Self.seam + r, y: Self.seam + r)
        // Kreis und Ringe reichen über die View hinaus, AppKit schneidet seit macOS 14 nicht mehr selbst ab.
        NSBezierPath(rect: bounds).addClip()
        Theme.panel.withAlphaComponent(CGFloat(Settings.tileOpacity)).setFill()
        outside(c, r).fill()
        NSGraphicsContext.saveGraphicsState()
        outside(c, r).addClip()
        Theme.panel.setFill()
        NSBezierPath(rect: CGRect(x: 0, y: 0, width: bounds.width, height: Self.seam)).fill()
        NSBezierPath(rect: CGRect(x: 0, y: 0, width: Self.seam, height: bounds.height)).fill()
        NSGraphicsContext.restoreGraphicsState()
        Instrument.seamLight.setFill()
        ring(c, from: r, to: r + 1).fill()
        Instrument.seamDark.setFill()
        ring(c, from: r + 1, to: r + 2).fill()
    }

    /// Alles in der View außerhalb des Kreises mit Radius `r`.
    private func outside(_ c: CGPoint, _ r: CGFloat) -> NSBezierPath {
        let p = NSBezierPath(rect: bounds)
        p.appendOval(in: CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r))
        p.windingRule = .evenOdd
        return p
    }

    /// Kreisring zwischen den Radien `inner` und `outer`.
    private func ring(_ c: CGPoint, from inner: CGFloat, to outer: CGFloat) -> NSBezierPath {
        let p = NSBezierPath(ovalIn: CGRect(x: c.x - outer, y: c.y - outer, width: 2 * outer, height: 2 * outer))
        p.appendOval(in: CGRect(x: c.x - inner, y: c.y - inner, width: 2 * inner, height: 2 * inner))
        p.windingRule = .evenOdd
        return p
    }
}
