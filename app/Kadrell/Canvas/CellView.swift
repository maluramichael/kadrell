import AppKit

/// LOD-Renderer einer Session-Kachel. Das echte Terminal wird von der Canvas als Subview eingehängt.
@MainActor
final class CellView: NSView {
    var session: Session
    var groupName = ""
    var groupColor = Theme.muted
    var lod = 3
    var focused = false
    var hovered = false
    var attached = false
    var lines: [String] = []
    /// nil = keine Suche aktiv; sonst gedimmt, wenn nicht getroffen.
    var highlight: Bool?
    var pulse: CGFloat = 1

    init(session: Session) {
        self.session = session
        super.init(frame: .zero)
        wantsLayer = true
        layer?.masksToBounds = true
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

    /// Nur ein eingehängtes Terminal nimmt Events; alles andere geht an die Canvas.
    override func hitTest(_ point: NSPoint) -> NSView? {
        let p = convert(point, from: superview)
        for sub in subviews { if let v = sub.hitTest(p) { return v } }
        return nil
    }

    var headerRect: CGRect { CGRect(x: 0, y: 0, width: bounds.width, height: Layout.cellHeaderScreen) }
    var bodyRect: CGRect { CGRect(x: 0, y: Layout.cellHeaderScreen, width: bounds.width, height: max(0, bounds.height - Layout.cellHeaderScreen)) }
    var xRect: CGRect { CGRect(x: bounds.width - 24, y: 5, width: 16, height: 16) }
    var dotRect: CGRect { CGRect(x: 9, y: 9, width: 8, height: 8) }
    var statusColor: NSColor { attached || !session.canAttach ? Theme.color(for: session.status) : Theme.detached }

    override func draw(_ dirtyRect: NSRect) {
        let b = bounds
        let dim: CGFloat = highlight == false ? 0.25 : 1
        let status = session.status
        let c = statusColor
        let dot = session.status == .running && session.canAttach ? c.withAlphaComponent(pulse) : c

        if lod == 0 {
            c.mixed(0.35, into: Theme.bg).withAlphaComponent(dim).setFill()
            b.fill()
            dot.withAlphaComponent(dim * dot.alphaComponent).setFill()
            NSBezierPath(ovalIn: CGRect(x: b.midX - 5, y: b.midY - 5, width: 10, height: 10)).fill()
            drawBorder(dim)
            return
        }
        if lod == 1 {
            c.mixed(0.18, into: Theme.bg).withAlphaComponent(dim).setFill()
            b.fill()
            let pad = b.width * 0.08
            let title = NSAttributedString(string: session.name, attributes: Theme.attrs(12, Theme.fg.withAlphaComponent(dim), bold: true, truncate: false))
            let para = NSMutableParagraphStyle(); para.alignment = .center; para.lineBreakMode = .byTruncatingTail
            let t = NSMutableAttributedString(attributedString: title)
            t.addAttribute(.paragraphStyle, value: para, range: NSRange(location: 0, length: t.length))
            let maxH: CGFloat = 12 * 1.25 * 3
            let r = t.boundingRect(with: CGSize(width: b.width - 2 * pad, height: maxH), options: [.usesLineFragmentOrigin])
            let g = NSAttributedString(string: groupName, attributes: Theme.attrs(10, Theme.muted.withAlphaComponent(dim)))
            let total = min(r.height, maxH) + 6 + 12
            let y = max(pad, b.midY - total / 2)
            t.draw(with: CGRect(x: pad, y: y, width: b.width - 2 * pad, height: min(r.height, maxH)), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
            let gs = g.size()
            g.draw(at: CGPoint(x: b.midX - min(gs.width, b.width - 2 * pad) / 2, y: y + min(r.height, maxH) + 6))
            drawBorder(dim)
            return
        }

        // LOD 2/3: Kopfzeile + Zeilen (oder Terminal-Subview)
        Theme.bg.withAlphaComponent(dim).setFill()
        b.fill()
        let head = headerRect
        (status == .waiting ? Theme.waiting.mixed(0.12, into: Theme.surface) : Theme.surface).withAlphaComponent(dim).setFill()
        head.fill()
        Theme.line.withAlphaComponent(dim).setFill()
        CGRect(x: 0, y: head.maxY - 1, width: b.width, height: 1).fill()
        dot.withAlphaComponent(dot.alphaComponent * dim).setFill()
        NSBezierPath(ovalIn: dotRect).fill()
        let meta = NSAttributedString(string: focused ? "⌘⎋ zurück · " + session.elapsed() : session.elapsed(), attributes: Theme.attrs(10.5, Theme.muted.withAlphaComponent(dim)))
        let metaW = meta.size().width
        let iconW: CGFloat = hovered ? 24 : 0
        let title = NSAttributedString(string: session.name, attributes: Theme.attrs(11.5, Theme.fg.withAlphaComponent(dim), bold: true))
        title.draw(with: CGRect(x: 24, y: 5, width: max(0, b.width - 24 - metaW - iconW - 16), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        meta.draw(at: CGPoint(x: b.width - iconW - 9 - metaW, y: 6))
        if hovered { Icons.x(in: xRect, color: Theme.muted) }

        let body = bodyRect
        let terminalMounted = subviews.contains { $0 is KadrellTerminalView }
        if session.isInteractive {
            drawLabel("LÄUFT IN ANDEREM TERMINAL", in: body, dim: dim)
        } else if session.isStale {
            drawHatch(in: body)
            drawLabel("PROZESS WEG · KLICK STARTET NEU", in: body, dim: dim)
        } else if session.isDone {
            drawLabel(session.state == "stopped" ? "GESTOPPT · KLICK SETZT FORT" : "BEENDET · KLICK SETZT FORT", in: body, dim: dim)
        } else if !attached {
            drawHatch(in: body)
            if !lines.isEmpty { drawLines(in: body.insetBy(dx: 10, dy: 8), dim: dim) }
            drawLabel(lines.isEmpty ? "NICHT ANGEHÄNGT" : "ATTACH FEHLGESCHLAGEN", in: body, dim: dim)
        } else if !terminalMounted {
            drawLines(in: body.insetBy(dx: 10, dy: 8), dim: dim)
        }
        drawBorder(dim)
    }

    private func drawBorder(_ dim: CGFloat) {
        let color: NSColor
        if focused { color = groupColor }
        else if highlight == true { color = .white }
        else if session.status == .waiting, session.canAttach { color = Theme.waiting }
        else if session.status == .error { color = Theme.error }
        else if hovered { color = Theme.sub }
        else { color = Theme.line }
        color.withAlphaComponent(dim).setFill()
        let w: CGFloat = (focused || highlight == true) ? 2 : 1
        bounds.frame(withWidth: w)
    }

    private func drawLines(in r: CGRect, dim: CGFloat) {
        guard r.height > 10, !lines.isEmpty else { return }
        let lineH: CGFloat = 12 * 1.45
        let fit = max(0, Int(r.height / lineH))
        guard fit > 0 else { return }
        let shown = lines.suffix(fit)
        // Beim Terminal-LOD von oben füllen, darunter die letzten Zeilen von unten.
        var y = lod >= 3 ? r.minY : r.maxY - CGFloat(shown.count) * lineH
        for line in shown {
            let color: NSColor = line.hasPrefix(">") ? Theme.fg : line.hasPrefix("⏺") || line.hasPrefix("●") ? Theme.sub : Theme.muted
            let a = NSAttributedString(string: line, attributes: Theme.attrs(12, color.withAlphaComponent(dim), bold: line.hasPrefix(">")))
            a.draw(with: CGRect(x: r.minX, y: y, width: r.width, height: lineH), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
            y += lineH
        }
    }

    private func drawHatch(in r: CGRect) {
        NSColor(srgbRed: 17 / 255, green: 17 / 255, blue: 27 / 255, alpha: 0.72).setFill()
        r.fill()
        NSGraphicsContext.saveGraphicsState()
        NSBezierPath(rect: r).addClip()
        NSColor.black.withAlphaComponent(0.35).setStroke()
        let p = NSBezierPath()
        p.lineWidth = 6
        var x = r.minX - r.height
        while x < r.maxX + r.height {
            p.move(to: CGPoint(x: x, y: r.maxY)); p.line(to: CGPoint(x: x + r.height, y: r.minY))
            x += 12 * 1.4142
        }
        p.stroke()
        NSGraphicsContext.restoreGraphicsState()
    }

    private func drawLabel(_ text: String, in r: CGRect, dim: CGFloat) {
        guard r.height > 24 else { return }
        let a = NSAttributedString(string: text, attributes: [.font: Theme.font(11), .foregroundColor: Theme.muted.withAlphaComponent(dim), .kern: 0.5])
        let s = a.size()
        a.draw(at: CGPoint(x: r.midX - s.width / 2, y: r.maxY - 10 - s.height))
    }
}
