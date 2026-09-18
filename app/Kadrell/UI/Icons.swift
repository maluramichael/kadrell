import AppKit

/// Dünne Strichgrafiken, keine Systembuttons.
enum Icons {
    @MainActor static func pen(in r: CGRect, color: NSColor) {
        let p = NSBezierPath()
        p.lineWidth = 1.5
        p.lineCapStyle = .square
        func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: r.minX + x / 16 * r.width, y: r.minY + y / 16 * r.height) }
        p.move(to: pt(11.5, 2.5)); p.line(to: pt(13.5, 4.5)); p.line(to: pt(5, 13)); p.line(to: pt(3, 13)); p.line(to: pt(3, 11)); p.close()
        p.move(to: pt(10, 4)); p.line(to: pt(12, 6))
        color.setStroke()
        p.stroke()
    }

    @MainActor static func plus(in r: CGRect, color: NSColor) {
        let p = NSBezierPath()
        p.lineWidth = 1.5
        p.lineCapStyle = .square
        let i = r.insetBy(dx: 3, dy: 3)
        p.move(to: CGPoint(x: i.midX, y: i.minY)); p.line(to: CGPoint(x: i.midX, y: i.maxY))
        p.move(to: CGPoint(x: i.minX, y: i.midY)); p.line(to: CGPoint(x: i.maxX, y: i.midY))
        color.setStroke()
        p.stroke()
    }

    @MainActor static func x(in r: CGRect, color: NSColor) {
        let p = NSBezierPath()
        p.lineWidth = 1.5
        p.lineCapStyle = .square
        let i = r.insetBy(dx: 4, dy: 4)
        p.move(to: CGPoint(x: i.minX, y: i.minY)); p.line(to: CGPoint(x: i.maxX, y: i.maxY))
        p.move(to: CGPoint(x: i.maxX, y: i.minY)); p.line(to: CGPoint(x: i.minX, y: i.maxY))
        color.setStroke()
        p.stroke()
    }

    /// Kreis-Umriss: Kachel aus der Ansicht nehmen (deselektieren), ohne die Session zu schließen.
    @MainActor static func circle(in r: CGRect, color: NSColor) {
        let p = NSBezierPath(ovalIn: r.insetBy(dx: 4, dy: 4))
        p.lineWidth = 1.5
        color.setStroke()
        p.stroke()
    }

    /// Monitor mit Fuß: Terminal ohne Claude (⌘T).
    @MainActor static func computer(in r: CGRect, color: NSColor) {
        func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: r.minX + x / 16 * r.width, y: r.minY + y / 16 * r.height) }
        let p = NSBezierPath(roundedRect: CGRect(origin: pt(1.5, 2), size: CGSize(width: r.width * 13 / 16, height: r.height * 9 / 16)), xRadius: 1.5, yRadius: 1.5)
        p.move(to: pt(8, 11)); p.line(to: pt(8, 14))
        p.move(to: pt(4.5, 14)); p.line(to: pt(11.5, 14))
        p.lineWidth = 1.3
        color.setStroke()
        p.stroke()
    }

    /// Server-Rack: zwei Einschübe mit Status-LED, für Remote-Sessions und Host-Gruppen.
    @MainActor static func server(in r: CGRect, color: NSColor) {
        func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: r.minX + x / 16 * r.width, y: r.minY + y / 16 * r.height) }
        let w = r.width * 12 / 16, h = r.height * 5 / 16
        let p = NSBezierPath()
        p.append(NSBezierPath(roundedRect: CGRect(origin: pt(2, 2), size: CGSize(width: w, height: h)), xRadius: 1, yRadius: 1))
        p.append(NSBezierPath(roundedRect: CGRect(origin: pt(2, 9), size: CGSize(width: w, height: h)), xRadius: 1, yRadius: 1))
        p.lineWidth = 1.3
        color.setStroke()
        p.stroke()
        color.setFill()
        NSBezierPath(ovalIn: CGRect(origin: pt(10.5, 3.5), size: CGSize(width: r.width / 8, height: r.height / 8))).fill()
        NSBezierPath(ovalIn: CGRect(origin: pt(10.5, 10.5), size: CGSize(width: r.width / 8, height: r.height / 8))).fill()
    }

    /// Herz für Favoriten: gefüllt, solange die Gruppe einer ist, sonst nur der Umriss.
    @MainActor static func heart(in r: CGRect, color: NSColor, filled: Bool) {
        func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: r.minX + x / 16 * r.width, y: r.minY + y / 16 * r.height) }
        let p = NSBezierPath()
        p.lineWidth = 1.5
        p.move(to: pt(8, 13.5))
        p.curve(to: pt(1.8, 6.2), controlPoint1: pt(4.2, 11), controlPoint2: pt(1.8, 8.6))
        p.curve(to: pt(8, 5.2), controlPoint1: pt(1.8, 2.6), controlPoint2: pt(6, 2.4))
        p.curve(to: pt(14.2, 6.2), controlPoint1: pt(10, 2.4), controlPoint2: pt(14.2, 2.6))
        p.curve(to: pt(8, 13.5), controlPoint1: pt(14.2, 8.6), controlPoint2: pt(11.8, 11))
        p.close()
        if filled { color.setFill(); p.fill() } else { color.setStroke(); p.stroke() }
    }

    /// Statuspunkt: normalerweise ein gefüllter Kreis in `color`, wie bisher. Ist „Farben nicht unterscheiden“
    /// (Bedienungshilfen › Anzeige) eingeschaltet, trägt zusätzlich die Form den Status, für Rot-Grün-Schwäche
    /// und alle, die sich nicht auf Farbe allein verlassen wollen: Ring = nicht gestartet, zwei Balken = wartet,
    /// Ausrufezeichen = Fehler. Fertig/arbeitet bleiben ein gefüllter Punkt (arbeitet pulsiert zusätzlich).
    @MainActor static func statusDot(in r: CGRect, status: SessionStatus, attached: Bool, color: NSColor,
                                     differentiate: Bool = NSWorkspace.shared.accessibilityDisplayShouldDifferentiateWithoutColor) {
        guard differentiate else {
            color.setFill(); NSBezierPath(ovalIn: r).fill(); return
        }
        guard attached else {
            let p = NSBezierPath(ovalIn: r.insetBy(dx: 1, dy: 1))
            p.lineWidth = 1.5
            color.setStroke(); p.stroke()
            return
        }
        color.setFill()
        NSBezierPath(ovalIn: r).fill()
        guard status == .waiting || status == .error else { return }
        let mark = Theme.pillText(on: color)
        mark.setFill()
        if status == .waiting {
            CGRect(x: r.minX + r.width * 0.30, y: r.minY + r.height * 0.22, width: r.width * 0.16, height: r.height * 0.56).fill()
            CGRect(x: r.minX + r.width * 0.54, y: r.minY + r.height * 0.22, width: r.width * 0.16, height: r.height * 0.56).fill()
        } else {
            CGRect(x: r.midX - r.width * 0.08, y: r.minY + r.height * 0.20, width: r.width * 0.16, height: r.height * 0.42).fill()
            NSBezierPath(ovalIn: CGRect(x: r.midX - r.width * 0.08, y: r.minY + r.height * 0.68, width: r.width * 0.16, height: r.width * 0.16)).fill()
        }
    }

    /// Chevron nach unten (offen) oder rechts (zu).
    @MainActor static func chevron(in r: CGRect, open: Bool, color: NSColor) {
        let p = NSBezierPath()
        p.lineWidth = 1.5
        let i = r.insetBy(dx: 4.5, dy: 4.5)
        if open {
            p.move(to: CGPoint(x: i.minX, y: i.midY - 2)); p.line(to: CGPoint(x: i.midX, y: i.midY + 2)); p.line(to: CGPoint(x: i.maxX, y: i.midY - 2))
        } else {
            p.move(to: CGPoint(x: i.midX - 2, y: i.minY)); p.line(to: CGPoint(x: i.midX + 2, y: i.midY)); p.line(to: CGPoint(x: i.midX - 2, y: i.maxY))
        }
        color.setStroke()
        p.stroke()
    }

    /// Ladeanzeige: Kreisbogen, der sich mit der Zeit dreht (Aufrufer zeichnet regelmäßig neu).
    @MainActor static func spinner(in r: CGRect, color: NSColor, width: CGFloat = 1.5) {
        let t = CACurrentMediaTime().truncatingRemainder(dividingBy: 1) * 360
        let p = NSBezierPath()
        p.lineWidth = width
        p.lineCapStyle = .round
        p.appendArc(withCenter: CGPoint(x: r.midX, y: r.midY), radius: min(r.width, r.height) / 2 - width / 2, startAngle: -t, endAngle: -t - 270, clockwise: true)
        color.setStroke()
        p.stroke()
    }

    /// Layout-Symbol für die Leiste: Grid = vier Quadrate, Stack = Rahmen mit zwei Linien.
    /// Layout-Symbol als Bild fürs Menü.
    @MainActor static func layoutImage(_ mode: LayoutMode) -> NSImage {
        NSImage(size: NSSize(width: 16, height: 16), flipped: true) { r in
            layout(mode, in: r.insetBy(dx: 1, dy: 1), color: .labelColor)
            return true
        }
    }

    @MainActor static func layout(_ mode: LayoutMode, in r: CGRect, color: NSColor) {
        let p = NSBezierPath()
        p.lineWidth = 1.2
        let i = r.insetBy(dx: 1, dy: 1)
        switch mode {
        case .grid:
            let w = (i.width - 2) / 2, h = (i.height - 2) / 2
            for (c, rr) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                p.appendRect(CGRect(x: i.minX + CGFloat(c) * (w + 2), y: i.minY + CGFloat(rr) * (h + 2), width: w, height: h))
            }
        case .stack:
            p.appendRect(i)
            p.move(to: CGPoint(x: i.minX, y: i.minY + i.height / 3)); p.line(to: CGPoint(x: i.maxX, y: i.minY + i.height / 3))
            p.move(to: CGPoint(x: i.minX, y: i.minY + 2 * i.height / 3)); p.line(to: CGPoint(x: i.maxX, y: i.minY + 2 * i.height / 3))
        case .main:
            p.appendRect(i)
            let x = i.minX + i.width * 0.55
            p.move(to: CGPoint(x: x, y: i.minY)); p.line(to: CGPoint(x: x, y: i.maxY))
            p.move(to: CGPoint(x: x, y: i.midY)); p.line(to: CGPoint(x: i.maxX, y: i.midY))
        case .spiral:
            p.appendRect(i)
            p.move(to: CGPoint(x: i.midX, y: i.minY)); p.line(to: CGPoint(x: i.midX, y: i.maxY))
            p.move(to: CGPoint(x: i.midX, y: i.midY)); p.line(to: CGPoint(x: i.maxX, y: i.midY))
            let q = i.midX + i.width / 4
            p.move(to: CGPoint(x: q, y: i.midY)); p.line(to: CGPoint(x: q, y: i.maxY))
        case .custom:
            p.appendRect(i)
            let x = i.minX + i.width * 0.4
            p.move(to: CGPoint(x: x, y: i.minY)); p.line(to: CGPoint(x: x, y: i.maxY))
            p.move(to: CGPoint(x: i.minX, y: i.midY)); p.line(to: CGPoint(x: x, y: i.midY))
        case .scroll:
            // Zwei Spalten, die dritte ragt rechts hinaus.
            let w = (i.width - 2) * 0.42
            p.appendRect(CGRect(x: i.minX, y: i.minY, width: w, height: i.height))
            p.appendRect(CGRect(x: i.minX + w + 2, y: i.minY, width: w, height: i.height))
            let x = i.minX + 2 * w + 4
            p.move(to: CGPoint(x: i.maxX, y: i.minY)); p.line(to: CGPoint(x: x, y: i.minY)); p.line(to: CGPoint(x: x, y: i.maxY)); p.line(to: CGPoint(x: i.maxX, y: i.maxY))
        }
        color.setStroke()
        p.stroke()
    }
}
