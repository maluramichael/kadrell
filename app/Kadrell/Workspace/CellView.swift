import AppKit

/// Eine Session-Kachel: Titelzeile plus Körper. Das echte Terminal hängt die Arbeitsfläche als Subview ein,
/// ohne Terminal zeigt der Körper Snapshot-Zeilen oder ein Statuslabel.
@MainActor
final class CellView: NSView {
    var session: Session
    var groupName = ""
    var groupColor = Theme.muted
    /// Fokus-Kachel der Arbeitsfläche: Rahmen in Gruppenfarbe.
    var focused = false
    /// Das Terminal dieser Kachel hat gerade die Tastatur.
    var keyboardFocus = false
    var hovered = false
    var attached = false
    var lines: [String] = []
    var pulse: CGFloat = 1
    /// Stack: die Titelzeile zeichnet die Arbeitsfläche als Stack-Zeile, die Kachel nur den Körper.
    var headerHidden = false

    init(session: Session) {
        self.session = session
        super.init(frame: .zero)
        wantsLayer = true
        layer?.masksToBounds = true
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

    /// Nur ein eingehängtes Terminal nimmt Events; alles andere geht an die Arbeitsfläche.
    override func hitTest(_ point: NSPoint) -> NSView? {
        let p = convert(point, from: superview)
        guard bounds.contains(p) else { return nil }
        for sub in subviews { if let v = sub.hitTest(p) { return v } }
        return nil
    }

    var headerHeight: CGFloat { headerHidden ? 0 : Tiling.rowHeight }
    var headerRect: CGRect { CGRect(x: 0, y: 0, width: bounds.width, height: headerHeight) }
    var bodyRect: CGRect { CGRect(x: 0, y: headerHeight, width: bounds.width, height: max(0, bounds.height - headerHeight)) }
    /// Terminal liegt 2 px innerhalb des Rahmens, sonst übermalt es den Rahmen links, rechts und unten.
    var terminalRect: CGRect { CGRect(x: 2, y: headerHeight + (headerHidden ? 2 : 0), width: max(0, bounds.width - 4), height: max(0, bounds.height - headerHeight - 2 - (headerHidden ? 2 : 0))).integral }
    var xRect: CGRect { CGRect(x: bounds.width - 24, y: 5, width: 16, height: 16) }
    var dotRect: CGRect { CGRect(x: 9, y: 9, width: 8, height: 8) }
    var statusColor: NSColor { attached || !session.canAttach ? Theme.color(for: session.status) : Theme.detached }

    override func draw(_ dirtyRect: NSRect) {
        Theme.bg.setFill()
        bounds.fill()
        if !headerHidden { drawHeader() }
        drawBody()
        drawBorder()
    }

    private func drawHeader() {
        let b = bounds, head = headerRect
        let c = statusColor
        let dot = session.status == .running && session.canAttach ? c.withAlphaComponent(pulse) : c
        let headBase = keyboardFocus ? groupColor.mixed(0.22, into: Theme.surface) : session.status == .waiting ? Theme.waiting.mixed(0.12, into: Theme.surface) : Theme.surface
        headBase.setFill()
        head.fill()
        Theme.line.setFill()
        CGRect(x: 0, y: head.maxY - 1, width: b.width, height: 1).fill()
        dot.setFill()
        NSBezierPath(ovalIn: dotRect).fill()
        let meta = NSAttributedString(string: session.elapsed(), attributes: Theme.attrs(10.5, Theme.muted))
        let metaW = meta.size().width
        let iconW: CGFloat = hovered ? 24 : 0
        let title = NSAttributedString(string: session.title, attributes: Theme.attrs(11.5, Theme.fg, bold: true))
        let group = NSAttributedString(string: groupName, attributes: Theme.attrs(10.5, groupColor))
        let titleW = min(title.size().width, max(0, b.width - 24 - metaW - iconW - 16 - group.size().width - 8))
        title.draw(with: CGRect(x: 24, y: 5, width: titleW, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        group.draw(with: CGRect(x: 24 + titleW + 8, y: 6, width: max(0, b.width - 24 - titleW - 8 - metaW - iconW - 16), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        meta.draw(at: CGPoint(x: b.width - iconW - 9 - metaW, y: 6))
        if hovered { Icons.x(in: xRect, color: Theme.sub) }
    }

    private func drawBody() {
        let body = bodyRect
        let terminalMounted = subviews.contains { $0 is KadrellTerminalView }
        if session.isPending {
            drawLabel("STARTET …", in: body)
        } else if session.isInteractive {
            drawLabel("LÄUFT IN ANDEREM TERMINAL", in: body)
        } else if session.isStale {
            drawHatch(in: body)
            drawLabel("PROZESS WEG · ⌘P › NEU STARTEN", in: body)
        } else if session.isDone {
            drawLabel(session.state == "stopped" ? "GESTOPPT · ⌘P › FORTSETZEN" : "BEENDET · ⌘P › FORTSETZEN", in: body)
        } else if !attached {
            drawHatch(in: body)
            if !lines.isEmpty { drawLines(in: body.insetBy(dx: 10, dy: 8)) }
            drawLabel(lines.isEmpty ? "NICHT ANGEHÄNGT" : "ATTACH FEHLGESCHLAGEN", in: body)
        } else if !terminalMounted {
            drawLines(in: body.insetBy(dx: 10, dy: 8))
        }
    }

    private func drawBorder() {
        let color: NSColor
        if focused { color = groupColor }
        else if session.status == .waiting, session.canAttach { color = Theme.waiting }
        else if session.status == .error { color = Theme.error }
        else if hovered { color = Theme.sub }
        else { color = Theme.line }
        color.setFill()
        bounds.frame(withWidth: focused ? 2 : 1)
    }

    private func drawLines(in r: CGRect) {
        let lineH: CGFloat = 12 * 1.45
        guard r.height > 10, !lines.isEmpty else { return }
        let fit = max(0, Int(r.height / lineH))
        guard fit > 0 else { return }
        var y = r.minY
        for line in lines.suffix(fit) {
            let color: NSColor = line.hasPrefix(">") ? Theme.fg : line.hasPrefix("⏺") || line.hasPrefix("●") ? Theme.sub : Theme.muted
            let a = NSAttributedString(string: line, attributes: Theme.attrs(12, color, bold: line.hasPrefix(">")))
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

    private func drawLabel(_ text: String, in r: CGRect) {
        guard r.height > 24 else { return }
        let para = NSMutableParagraphStyle(); para.alignment = .center; para.lineBreakMode = .byTruncatingTail
        let a = NSAttributedString(string: text, attributes: [.font: Theme.font(11), .foregroundColor: Theme.muted, .kern: 0.5, .paragraphStyle: para])
        let h = a.size().height
        a.draw(with: CGRect(x: r.minX + 6, y: r.maxY - 10 - h, width: r.width - 12, height: h), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }
}
