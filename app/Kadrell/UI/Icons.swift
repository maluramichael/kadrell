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

    /// Layout-Symbol für die Leiste: Grid = vier Quadrate, Stack = Rahmen mit zwei Linien.
    @MainActor static func layout(_ mode: LayoutMode, in r: CGRect, color: NSColor) {
        let p = NSBezierPath()
        p.lineWidth = 1.2
        let i = r.insetBy(dx: 1, dy: 1)
        if mode == .grid {
            let w = (i.width - 2) / 2, h = (i.height - 2) / 2
            for (c, rr) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                p.appendRect(CGRect(x: i.minX + CGFloat(c) * (w + 2), y: i.minY + CGFloat(rr) * (h + 2), width: w, height: h))
            }
        } else {
            p.appendRect(i)
            p.move(to: CGPoint(x: i.minX, y: i.minY + i.height / 3)); p.line(to: CGPoint(x: i.maxX, y: i.minY + i.height / 3))
            p.move(to: CGPoint(x: i.minX, y: i.minY + 2 * i.height / 3)); p.line(to: CGPoint(x: i.maxX, y: i.minY + 2 * i.height / 3))
        }
        color.setStroke()
        p.stroke()
    }
}
