import AppKit

/// Theme-Name einer Extension auf die Farben des Schemas, wie im Baum: Punkte in der Statusfarbe. `warn` bleibt auch
/// als Text das Gelb des Status-Punkts (`waiting`), mit `waitingText` wäre er von `accent` nicht zu unterscheiden.
/// Gilt für rechte Sidebar und Statusleiste.
extension ThemeColor {
    var nsColor: NSColor {
        switch self {
        case .accent: Theme.running
        case .muted: Theme.muted
        case .ok: Theme.idle
        case .warn: Theme.waiting
        case .err: Theme.error
        }
    }
}

/// Zeichnen der rechten Sidebar, im Stil des Baums: gleiche Schriften, Chevrons, Hervorhebung und Tastatur-Rahmen.
extension ExtensionPanelView {
    override func draw(_ dirty: NSRect) {
        Theme.panel.withAlphaComponent(CGFloat(Settings.tileOpacity)).setFill()
        dirty.fill()
        let d = dirty.scaled(1 / Theme.scale)
        Theme.scaled(bounds) { r in
            drawTabs(width: r.width)
            for i in lines.indices where rect(i).intersects(d) { drawLine(i) }
        }
    }

    private func drawTabs(width: CGFloat) {
        Theme.line.setFill()
        CGRect(x: 0, y: Self.tabHeight - 1, width: width, height: 1).fill()
        for (p, r) in zip(panels, tabRects()) {
            let on = p.name == selectedTab
            NSAttributedString(string: Self.label(p), attributes: Theme.attrs(11, on ? Theme.fg : Theme.muted, bold: on))
                .draw(at: CGPoint(x: r.minX + 8, y: r.midY - 8))
            if on { Theme.running.setFill(); CGRect(x: r.minX + 4, y: r.maxY - 3, width: r.width - 8, height: 2).fill() }
        }
    }

    private func drawLine(_ i: Int) {
        let l = lines[i], r = rect(i), x = 12 + CGFloat(l.depth) * 12, hot = cursor == i || hovered == i
        if l.selectable { drawHighlight(r, hot: hot, ring: cursor == i && hasKeyboard) }
        switch l.node {
        case .section(let title, _, _):
            Icons.chevron(in: CGRect(x: x - 6, y: r.midY - 8, width: 16, height: 16), open: isOpen(l), color: Theme.muted)
            text(title, Theme.attrs(11, Theme.running, bold: true), from: x + 14, to: r.maxX - 10, in: r)
        case .item(let title, let detail, let color, _):
            if let color { color.nsColor.setFill(); NSBezierPath(ovalIn: CGRect(x: x, y: r.midY - 4, width: 8, height: 8)).fill() }
            let right = drawDetail(detail, in: r)
            text(title, Theme.attrs(12, hot ? Theme.fg : Theme.sub), from: x + 14, to: right, in: r)
        case .text(let s, let color):
            text(s, Theme.attrs(11, color?.nsColor ?? Theme.muted), from: x, to: r.maxX - 10, in: r)
        case .button(let label, _):
            drawButton(label, in: CGRect(x: x, y: r.minY + 4, width: max(0, r.maxX - 10 - x), height: r.height - 8), hot: hot)
        }
    }

    /// Wie eine Session-Zeile: grau hinterlegt bei Hover und Auswahl, mit Tastatur zusätzlich in Akzentfarbe gerahmt.
    private func drawHighlight(_ r: CGRect, hot: Bool, ring: Bool) {
        if hot { Theme.surface.setFill(); r.fill() }
        if ring { Theme.running.setStroke(); NSBezierPath(rect: r.insetBy(dx: 0.5, dy: 0.5)).stroke() }
    }

    /// Detail rechtsbündig wie die Laufzeit im Baum, höchstens 40 % der Breite. Liefert die rechte Kante für den Text.
    private func drawDetail(_ detail: String?, in r: CGRect) -> CGFloat {
        guard let detail, !detail.isEmpty else { return r.maxX - 10 }
        let s = NSAttributedString(string: detail, attributes: Theme.attrs(10.5, Theme.muted))
        let w = min(s.size().width, r.width * 0.4)
        s.draw(with: CGRect(x: r.maxX - 10 - w, y: r.midY - 7, width: w, height: 14), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        return r.maxX - 18 - w
    }

    private func drawButton(_ label: String, in b: CGRect, hot: Bool) {
        let path = NSBezierPath(roundedRect: b.insetBy(dx: 0.5, dy: 0.5), xRadius: 4, yRadius: 4)
        (hot ? Theme.line : Theme.surface).setFill()
        path.fill()
        Theme.line.setStroke()
        path.stroke()
        let s = NSAttributedString(string: label, attributes: Theme.attrs(11, Theme.fg))
        let w = min(s.size().width, b.width - 12)
        s.draw(with: CGRect(x: b.midX - w / 2, y: b.midY - 8, width: w, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    /// Eine Zeile, am Ende abgeschnitten. Mehrzeilige Texte zeigen nur ihre erste Zeile.
    private func text(_ s: String, _ attrs: [NSAttributedString.Key: Any], from x: CGFloat, to right: CGFloat, in r: CGRect) {
        NSAttributedString(string: s, attributes: attrs)
            .draw(with: CGRect(x: x, y: r.midY - 8, width: max(0, right - x), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }
}
