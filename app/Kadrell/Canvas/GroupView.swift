import AppKit

/// Zeichnet Panel, Balken, Kopfzeile und die gestrichelte `+`-Kachel einer Gruppe. Keine Events: die Canvas testet selbst.
@MainActor
final class GroupView: NSView {
    var group: Group
    var count = 0
    var lod = 3
    var hovered = false
    var dimmed = false
    /// Kopf- und Fußstreifen in eigenen Koordinaten (von der Canvas gesetzt).
    var headerRect: CGRect = .zero
    var footerRect: CGRect = .zero
    var color: NSColor { NSColor(hexString: group.color) }

    init(group: Group) {
        self.group = group
        super.init(frame: .zero)
        wantsLayer = true
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    /// Trefferflächen der Icons in eigenen Koordinaten.
    var plusRect: CGRect { CGRect(x: headerRect.maxX - 72, y: headerRect.midY - 8, width: 16, height: 16) }
    var penRect: CGRect { CGRect(x: headerRect.maxX - 50, y: headerRect.midY - 8, width: 16, height: 16) }
    var xRect: CGRect { CGRect(x: headerRect.maxX - 28, y: headerRect.midY - 8, width: 16, height: 16) }

    override func draw(_ dirtyRect: NSRect) {
        let b = bounds
        (dimmed ? Theme.panel.withAlphaComponent(0.4) : Theme.panel).setFill()
        b.fill()
        Theme.line.setFill()
        b.insetBy(dx: 0, dy: 0).frame(withWidth: 1)
        color.setFill()
        CGRect(x: 0, y: 0, width: 3, height: b.height).fill()

        // Kopf- und Fußstreifen wie Leisten: eigener Hintergrund, 1 px Trennlinie
        let h = headerRect
        guard h.height > 4 else { return }
        Theme.surface.withAlphaComponent(dimmed ? 0.4 : 1).setFill()
        CGRect(x: 3, y: 0, width: b.width - 3, height: h.height).fill()
        CGRect(x: 3, y: footerRect.minY, width: b.width - 3, height: footerRect.height).fill()
        Theme.line.setFill()
        CGRect(x: 3, y: h.height - 1, width: b.width - 3, height: 1).fill()
        CGRect(x: 3, y: footerRect.minY, width: b.width - 3, height: 1).fill()
        var x = h.minX + 12
        let nameAttrs = Theme.attrs(13, dimmed ? color.withAlphaComponent(0.4) : color, bold: true)
        let name = NSAttributedString(string: "▪ " + group.name, attributes: nameAttrs)
        let iconsW: CGFloat = lod >= 1 ? 78 : 0
        let rightW = iconsW + 20
        let nameW = min(name.size().width, h.width - rightW)
        name.draw(with: CGRect(x: x, y: h.minY + (h.height - 17) / 2, width: nameW, height: 17), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        x += nameW + 8
        if lod >= 2 {
            let path = NSAttributedString(string: Theme.shortPath(group.cwd), attributes: Theme.attrs(11, Theme.muted))
            let w = max(0, h.maxX - rightW - x)
            if w > 20 { path.draw(with: CGRect(x: x, y: h.minY + (h.height - 15) / 2 + 1, width: w, height: 15), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine]) }
        }
        if lod >= 1 {
            let c = hovered ? Theme.sub : Theme.muted
            Icons.plus(in: plusRect, color: c)
            Icons.pen(in: penRect, color: c)
            Icons.x(in: xRect, color: c)
            // Griff unten rechts zum Größerziehen
            let g = resizeRect
            let p = NSBezierPath()
            p.lineWidth = 1
            p.move(to: CGPoint(x: g.maxX - 2, y: g.minY + 6)); p.line(to: CGPoint(x: g.minX + 6, y: g.maxY - 2))
            p.move(to: CGPoint(x: g.maxX - 2, y: g.minY + 11)); p.line(to: CGPoint(x: g.minX + 11, y: g.maxY - 2))
            c.setStroke(); p.stroke()
        }
    }

    var resizeRect: CGRect { CGRect(x: bounds.width - 18, y: footerRect.midY - 8, width: 16, height: 16) }
}

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
}
