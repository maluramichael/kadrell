import AppKit

/// Bauteile des Instrument-Looks: Fuge, Taste und Display. Gezeichnet wird in gespiegelten Views innerhalb von
/// `Theme.scaled`, die Koordinaten sind also unskalierte Punkte und y wächst nach unten.
enum Instrument {
    // MARK: Farben

    /// Dunkle Hälfte der Fuge: Schwarz mit Alpha, in hellen Schemata deutlich schwächer.
    static var seamDark: NSColor { NSColor.black.withAlphaComponent(Theme.current.dark ? 0.6 : 0.13) }
    /// Helle Hälfte der Fuge: Weiß mit Alpha, in hellen Schemata deutlich stärker.
    static var seamLight: NSColor { NSColor.white.withAlphaComponent(Theme.current.dark ? 0.05 : 0.65) }
    static var keyCap: NSColor { Theme.surface }
    /// Kante und Dicke der Taste. Startwerte, die Sichtprüfung der Leiste kann sie nachziehen.
    static var keyEdge: NSColor {
        Theme.current.dark ? Theme.bg.mixed(0.3, into: .black) : Theme.line.mixed(0.75, into: .black)
    }
    /// Display-Grund und Schrift sind in allen Schemata gleich, damit Status-Anzeigen überall wie ein Gerät aussehen.
    static var screenFace: NSColor { NSColor(hex: Theme.current.dark ? 0x050505 : 0x0d0d0c) }
    static var screenInk: NSColor { NSColor(hex: 0xefe9dc) }
    static var screenMuted: NSColor { NSColor(hex: 0x8f8b81) }

    // MARK: Maße

    /// Ein Gerätepixel in Punkten: bei jeder UI-Größe die feinste sichtbare Linie.
    static var hairline: CGFloat { 1 / Theme.scale }
    private static let keyRadius: CGFloat = 5
    private static let keyDepth: CGFloat = 2

    /// Rastet eine Position aufs Gerätepixel ein, sonst verschmiert eine 1-Pixel-Linie bei 90 bis 130 % über drei Zeilen.
    static func snapped(_ v: CGFloat) -> CGFloat { (v * Theme.scale).rounded() / Theme.scale }

    // MARK: Fuge

    /// Waagrechte Fuge: dunkle Linie bei `y`, helle direkt darunter.
    @MainActor static func hSeam(x: CGFloat, y: CGFloat, width: CGFloat) {
        let y = snapped(y)
        fill(CGRect(x: x, y: y, width: width, height: hairline), seamDark)
        fill(CGRect(x: x, y: y + hairline, width: width, height: hairline), seamLight)
    }

    /// Senkrechte Fuge: dunkle Linie bei `x`, helle direkt rechts davon.
    @MainActor static func vSeam(x: CGFloat, y: CGFloat, height: CGFloat) {
        let x = snapped(x)
        fill(CGRect(x: x, y: y, width: hairline, height: height), seamDark)
        fill(CGRect(x: x + hairline, y: y, width: hairline, height: height), seamLight)
    }

    // MARK: Taste

    /// Zeichnet eine Taste in `r` und gibt das Rechteck der Kappe zurück (dort sitzt der Text). Unten trägt `r` die
    /// Dicke von 2 pt, gedrückt sitzt die Kappe 1 pt tiefer und die Dicke ist nur noch 1 pt.
    @MainActor @discardableResult
    static func key(_ r: CGRect, pressed: Bool, fill color: NSColor?) -> CGRect {
        let sink: CGFloat = pressed ? 1 : 0
        let cap = CGRect(x: r.minX, y: r.minY + sink, width: r.width, height: r.height - keyDepth)
        let body = CGRect(x: cap.minX, y: cap.minY, width: cap.width, height: cap.height + keyDepth - sink)
        keyEdge.setFill()
        NSBezierPath(roundedRect: body, xRadius: keyRadius, yRadius: keyRadius).fill()
        (color ?? keyCap).setFill()
        NSBezierPath(roundedRect: cap.insetBy(dx: 1, dy: 1), xRadius: keyRadius - 1, yRadius: keyRadius - 1).fill()
        return cap
    }

    // MARK: Display

    /// Schwarzes Display mit feiner heller Haarlinie am oberen Rand.
    @MainActor static func screen(_ r: CGRect) {
        let face = NSBezierPath(roundedRect: r, xRadius: keyRadius, yRadius: keyRadius)
        screenFace.setFill()
        face.fill()
        NSGraphicsContext.saveGraphicsState()
        face.addClip()
        fill(CGRect(x: r.minX, y: r.minY, width: r.width, height: hairline), NSColor.white.withAlphaComponent(0.06))
        NSGraphicsContext.restoreGraphicsState()
    }

    /// Füllt mit Alpha über den Untergrund (`NSRectFill` würde ersetzen statt überblenden).
    private static func fill(_ r: CGRect, _ color: NSColor) {
        color.setFill()
        NSBezierPath(rect: r).fill()
    }
}
