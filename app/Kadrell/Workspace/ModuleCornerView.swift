import AppKit

/// Runde Ecke oben links in der Arbeitsfläche: Das Gehäuse (Farbe wie der Baum) füllt die Ecke außerhalb des
/// Viertelkreises, entlang des Bogens läuft die Fuge, dunkel außen und hell direkt innen, je 1 pt. So sitzt die
/// Arbeitsfläche wie ein Modul im Gehäuse. Liegt zuoberst über den Kacheln und lässt jeden Klick durch.
final class ModuleCornerView: NSView {
    /// Radius in logischen Punkten, in der App mal `Theme.scale`.
    static let radius: CGFloat = 14

    override var isFlipped: Bool { true }
    override var isOpaque: Bool { false }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func draw(_ dirtyRect: NSRect) {
        let r = min(bounds.width, bounds.height)
        let housing = NSBezierPath()
        housing.move(to: .zero)
        housing.line(to: CGPoint(x: r, y: 0))
        housing.appendArc(withCenter: CGPoint(x: r, y: r), radius: r, startAngle: 270, endAngle: 180, clockwise: true)
        housing.close()
        Theme.panel.withAlphaComponent(CGFloat(Settings.tileOpacity)).setFill()
        housing.fill()
        arc(radius: r - 0.5, center: r, color: Instrument.seamDark)
        arc(radius: r - 1.5, center: r, color: Instrument.seamLight)
    }

    /// Viertelbogen um die Gegenecke, 1 pt breit.
    private func arc(radius: CGFloat, center c: CGFloat, color: NSColor) {
        let path = NSBezierPath()
        path.appendArc(withCenter: CGPoint(x: c, y: c), radius: radius, startAngle: 270, endAngle: 180, clockwise: true)
        path.lineWidth = 1
        color.setStroke()
        path.stroke()
    }
}
