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

    /// Vier senkrechte Striche als Arbeits-Anzeige. `lit` = Anzahl leuchtender Striche (1...4), die übrigen stehen
    /// mit 25 % Deckkraft; nil = alle vier voll. Gerechnet wird in Gerätepixeln: Striche und Lücken sind ganze Pixel,
    /// alle Lücken gleich breit, der Rest der Breite verteilt sich links und rechts. So rastet die Anzeige bei jeder
    /// UI-Größe ein, ohne dass eine Lücke schmaler wird als die anderen.
    @MainActor static func meter(_ r: CGRect, lit: Int?, color: NSColor) {
        let s = Theme.scale, total = (r.width * s).rounded()
        let gap = max(1, (1.5 * s).rounded(.down)), bar = max(1, ((total - 3 * gap) / 4).rounded(.down))
        let x0 = snapped(r.minX) + ((total - 4 * bar - 3 * gap) / 2).rounded(.down) / s
        let y0 = snapped(r.minY), y1 = snapped(r.maxY)
        let dim = color.withAlphaComponent(color.alphaComponent * 0.25)
        for i in 0..<4 {
            let on = lit.map { i < $0 } ?? true
            fill(CGRect(x: x0 + CGFloat(i) * (bar + gap) / s, y: y0, width: bar / s, height: y1 - y0), on ? color : dim)
        }
    }

    // MARK: Fläche und Rand

    /// Rechteck, bei dem nur die Ecke bei (minX, minY) rund ist (in gespiegelten Views oben links). `radius <= 0` = Rechteck.
    /// `openBottom`: ohne Unterkante, zum Nachziehen eines Rands, der unten offen bleibt.
    static func topLeftRounded(_ r: CGRect, radius: CGFloat, openBottom: Bool = false) -> NSBezierPath {
        let path = NSBezierPath()
        let rad = min(max(radius, 0), r.width, r.height)
        path.move(to: CGPoint(x: r.minX, y: r.maxY))
        if rad > 0 {
            path.appendArc(from: CGPoint(x: r.minX, y: r.minY), to: CGPoint(x: r.maxX, y: r.minY), radius: rad)
        } else {
            path.line(to: CGPoint(x: r.minX, y: r.minY))
        }
        path.line(to: CGPoint(x: r.maxX, y: r.minY))
        path.line(to: CGPoint(x: r.maxX, y: r.maxY))
        if !openBottom { path.close() }
        return path
    }

    /// Rand einer eingesetzten Fläche ohne Unterkante: links, rechts und mit `top` oben außen eine Haarlinie `seamDark`,
    /// direkt innen eine Haarlinie `seamLight`. Mit `radius` folgt die Ecke oben links dem Bogen der Modul-Ecke.
    @MainActor static func rim(_ r: CGRect, top: Bool, radius: CGFloat = 0) {
        let left = snapped(r.minX), right = snapped(r.maxX), topEdge = snapped(r.minY), bottom = snapped(r.maxY)
        func stroke(inset: CGFloat, _ color: NSColor) {
            let path = NSBezierPath()
            path.lineWidth = hairline
            path.lineCapStyle = .butt
            let l = left + inset, rt = right - inset, t = topEdge + inset
            path.move(to: CGPoint(x: l, y: bottom))
            if top {
                let rad = radius - inset
                if rad > 0 {
                    path.appendArc(from: CGPoint(x: l, y: t), to: CGPoint(x: rt, y: t), radius: rad)
                } else {
                    path.line(to: CGPoint(x: l, y: t))
                }
                path.line(to: CGPoint(x: rt, y: t))
            } else {
                path.line(to: CGPoint(x: l, y: topEdge))
                path.move(to: CGPoint(x: rt, y: topEdge))
            }
            path.line(to: CGPoint(x: rt, y: bottom))
            color.setStroke()
            path.stroke()
        }
        stroke(inset: hairline / 2, seamDark)
        stroke(inset: 1.5 * hairline, seamLight)
    }

    // MARK: Gruppe

    /// Gesperrtes Versal-Label einer Gruppe, wie im Baum.
    static func groupLabel(_ text: String) -> NSAttributedString {
        var attrs = Theme.attrs(10.5, Theme.sub, bold: true)
        attrs[.kern] = CGFloat(0.8)
        return NSAttributedString(string: text.uppercased(), attributes: attrs)
    }

    /// Farbakzent der Gruppe: 6 × 6 pt mit Radius 1,5, mittig auf `center`.
    @MainActor static func groupAccent(center: CGPoint, color: NSColor) {
        color.setFill()
        NSBezierPath(roundedRect: CGRect(x: center.x - 3, y: center.y - 3, width: 6, height: 6), xRadius: 1.5, yRadius: 1.5).fill()
    }

    /// Füllt mit Alpha über den Untergrund (`NSRectFill` würde ersetzen statt überblenden).
    private static func fill(_ r: CGRect, _ color: NSColor) {
        color.setFill()
        NSBezierPath(rect: r).fill()
    }
}
