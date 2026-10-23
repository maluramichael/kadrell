import AppKit

/// Theme-Name einer Extension auf die Farben des Schemas, wie im Baum: Punkte in der Statusfarbe. `warn` bleibt auch
/// als Text das Gelb des Status-Punkts (`waiting`), mit `waitingText` wäre er von `accent` nicht zu unterscheiden.
/// Gilt für rechte Sidebar und Statusleiste.
extension ThemeColor {
    var nsColor: NSColor {
        switch self {
        case .accent: Theme.accent
        case .muted: Theme.muted
        case .ok: Theme.idle
        case .warn: Theme.waiting
        case .err: Theme.error
        }
    }

    /// Farbe für Text: Akzent und Fehler in ihrer lesbaren Text-Rolle (≥ 4,5:1 auf jeder Fläche), sonst wie `nsColor`.
    var textColor: NSColor {
        switch self {
        case .accent: Theme.accentText
        case .err: Theme.errorText
        default: nsColor
        }
    }
}

/// Zeichnen der rechten Sidebar, im Stil des Baums: gleiche Schriften, Chevrons, Hervorhebung und Tastatur-Rahmen.
/// Tabs und Knöpfe sind Tasten, unter der Tab-Reihe liegt eine Fuge.
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

    /// Jeder Tab ist eine Taste, der gewählte bleibt eingesunken. Die Trefferflächen bleiben `tabRects()`.
    private func drawTabs(width: CGFloat) {
        Instrument.hSeam(x: 0, y: Self.tabHeight - 2, width: width)
        for (p, r) in zip(panels, tabRects()) {
            let on = p.name == selectedTab
            let cap = Instrument.key(CGRect(x: r.minX + 2, y: 5, width: r.width - 4, height: 22), pressed: on, fill: nil)
            let s = NSAttributedString(string: Self.label(p), attributes: Theme.attrs(11, on ? KeyRole.neutral.label : Theme.muted, bold: on))
            centered(s, on: cap)
        }
    }

    private func drawLine(_ i: Int) {
        let l = lines[i], r = rect(i), x = 12 + CGFloat(l.depth) * 12, hot = cursor == i || hovered == i
        if l.selectable { drawHighlight(r, hot: hot, ring: cursor == i && hasKeyboard) }
        drawNode(l, in: r, x: x, hot: hot, pressed: pressed == i)
    }

    private func drawNode(_ l: Line, in r: CGRect, x: CGFloat, hot: Bool, pressed: Bool) {
        switch l.node {
        case .section(let title, _, _):
            Icons.chevron(in: CGRect(x: x - 6, y: r.midY - 8, width: 16, height: 16), open: isOpen(l), color: Theme.muted)
            var attrs = Theme.attrs(11, Theme.accentText, bold: true)
            attrs[.kern] = Instrument.labelKern
            text(title.uppercased(), attrs, from: x + 14, to: r.maxX - 10, in: r)
        case .item(let title, let detail, let color, _):
            if let color { color.nsColor.setFill(); NSBezierPath(ovalIn: CGRect(x: x, y: r.midY - 4, width: 8, height: 8)).fill() }
            let right = drawDetail(detail, in: r)
            text(title, Theme.attrs(12, hot ? Theme.fg : Theme.sub), from: x + 14, to: right, in: r)
        case .text(let s, let color):
            text(s, Theme.attrs(11, color?.textColor ?? Theme.muted), from: x, to: r.maxX - 10, in: r)
        case .button(let label, _):
            drawButton(label, in: CGRect(x: x, y: r.minY + 4, width: max(0, r.maxX - 10 - x), height: r.height - 8), pressed: pressed)
        }
    }

    /// Wie eine Session-Zeile im Baum: Fläche bei Hover und Auswahl, mit Tastatur zusätzlich Umriss und Akzentring.
    private func drawHighlight(_ r: CGRect, hot: Bool, ring: Bool) {
        if hot || ring { Instrument.rowHighlight(r, outline: ring, ring: ring) }
    }

    /// Detail rechtsbündig wie die Laufzeit im Baum, höchstens 40 % der Breite. Liefert die rechte Kante für den Text.
    private func drawDetail(_ detail: String?, in r: CGRect) -> CGFloat {
        guard let detail, !detail.isEmpty else { return r.maxX - 10 }
        let s = NSAttributedString(string: detail, attributes: Theme.attrs(10.5, Theme.muted))
        let w = min(s.size().width, r.width * 0.4)
        s.draw(with: CGRect(x: r.maxX - 10 - w, y: r.midY - 7, width: w, height: 14), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        return r.maxX - 18 - w
    }

    /// Neutrale Taste, solange die Maus gedrückt ist eingesunken.
    private func drawButton(_ label: String, in b: CGRect, pressed: Bool) {
        let cap = Instrument.key(b, pressed: pressed, fill: nil)
        centered(NSAttributedString(string: label, attributes: Theme.attrs(11, KeyRole.neutral.label)), on: cap)
    }

    /// Eine Zeile mittig auf einer Tastenkappe, zu lang wird sie am Ende abgeschnitten.
    private func centered(_ s: NSAttributedString, on cap: CGRect) {
        let w = min(s.size().width, max(0, cap.width - 12))
        s.draw(with: CGRect(x: cap.midX - w / 2, y: cap.midY - 8, width: w, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    /// Eine Zeile, am Ende abgeschnitten. Mehrzeilige Texte zeigen nur ihre erste Zeile.
    private func text(_ s: String, _ attrs: [NSAttributedString.Key: Any], from x: CGFloat, to right: CGFloat, in r: CGRect) {
        NSAttributedString(string: s, attributes: attrs)
            .draw(with: CGRect(x: x, y: r.midY - 8, width: max(0, right - x), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }
}
