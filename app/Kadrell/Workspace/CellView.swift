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
    /// Beim Ziehen einer anderen Kachel: hier landet sie.
    var dropTarget = false
    var attached = false
    /// Claude hat sich beendet oder wurde gestoppt; ohne das Flag startet der Prozess gerade.
    var ended = false
    /// Vorschau (⌥J/⌥K) startet keinen Prozess: ohne Terminal kein „STARTET …“.
    var previewing = false
    /// Das Terminal hängt gerade in einem anderen Fenster.
    var elsewhere = false
    var lines: [String] = []
    var pulse: CGFloat = 1
    /// Stack: die Titelzeile zeichnet die Arbeitsfläche als Stack-Zeile, die Kachel nur den Körper.
    var headerHidden = false
    /// Gezoomt, obwohl mehrere Sessions offen sind: Badge „Z“ in der Titelzeile wie in tmux.
    var zoomed = false

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

    var headerHeight: CGFloat { headerHidden ? 0 : (Tiling.rowHeight * Theme.scale).rounded() }
    var headerRect: CGRect { CGRect(x: 0, y: 0, width: bounds.width, height: headerHeight) }
    var bodyRect: CGRect { CGRect(x: 0, y: headerHeight, width: bounds.width, height: max(0, bounds.height - headerHeight)) }
    /// Terminal liegt 2 px innerhalb des Rahmens, sonst übermalt es den Rahmen links, rechts und unten. Dazu der Innenabstand.
    var terminalRect: CGRect {
        let p = CGFloat(Settings.terminalPadding), top = headerHeight + (headerHidden ? 2 : 0) + p
        return CGRect(x: 2 + p, y: top, width: max(0, bounds.width - 4 - 2 * p), height: max(0, bounds.height - top - 2 - p)).integral
    }
    /// Logische Punkte der Titelzeile (vor `Theme.scale`).
    private var xRectLogical: CGRect { CGRect(x: bounds.width / Theme.scale - 24, y: 5, width: 16, height: 16) }
    private let dotRectLogical = CGRect(x: 9, y: 9, width: 8, height: 8)
    private var penRectLogical: CGRect { xRectLogical.offsetBy(dx: -20, dy: 0) }
    var xRect: CGRect { xRectLogical.scaled(Theme.scale) }
    var penRect: CGRect { penRectLogical.scaled(Theme.scale) }
    var dotRect: CGRect { dotRectLogical.scaled(Theme.scale) }
    var statusColor: NSColor { attached ? Theme.color(for: session.status) : Theme.detached }
    /// Hintergrund der Kachel, leicht in Gruppenfarbe getönt, damit Gruppen auf einen Blick auseinanderfallen.
    /// Mit Deckkraft unter 100 % scheint das Hintergrundbild der Arbeitsfläche durch, auch durchs Terminal.
    var bodyColor: NSColor { groupColor.mixed(0.05, into: Theme.bg).withAlphaComponent(CGFloat(Settings.tileOpacity)) }

    override func draw(_ dirtyRect: NSRect) {
        bodyColor.setFill()
        bounds.fill()
        if !headerHidden { Theme.scaled(headerRect) { drawHeader($0) } }
        Theme.scaled(bodyRect) { drawBody($0) }
    }

    /// Rahmen als Layer-Rand: liegt über dem Terminal und blendet bei Fokuswechsel in 120 ms über statt zu springen.
    override func viewWillDraw() {
        super.viewWillDraw()
        guard let layer else { return }
        let c = borderColor.cgColor, w: CGFloat = focused || dropTarget ? 2 : 1
        guard layer.borderColor != c || layer.borderWidth != w else { return }
        let anim = CABasicAnimation(keyPath: "borderColor")
        anim.fromValue = layer.borderColor
        anim.toValue = c
        anim.duration = 0.12
        layer.add(anim, forKey: "borderColor")
        layer.borderColor = c
        layer.borderWidth = w
    }

    private func drawHeader(_ head: CGRect) {
        let b = head
        let c = statusColor
        let dot = session.status == .running && attached ? c.withAlphaComponent(pulse) : c
        let headBase = groupColor.mixed(keyboardFocus || focused ? 0.22 : 0.12, into: Theme.surface)
        headBase.setFill()
        head.fill()
        Theme.line.setFill()
        CGRect(x: 0, y: head.maxY - 1, width: b.width, height: 1).fill()
        Icons.statusDot(in: dotRectLogical, status: session.status, attached: attached, color: dot)
        let meta = NSAttributedString(string: session.elapsed(), attributes: Theme.attrs(10.5, Theme.muted))
        let metaW = meta.size().width
        var iconW: CGFloat = hovered ? 44 : 0
        if zoomed {
            let z = CGRect(x: b.width - iconW - 9 - metaW - 8 - 16, y: 5, width: 16, height: 16)
            Theme.waiting.setFill(); z.fill()
            let zt = NSAttributedString(string: "Z", attributes: Theme.attrs(11, Theme.pillText(on: Theme.waiting), bold: true))
            zt.draw(at: CGPoint(x: z.midX - zt.size().width / 2, y: z.minY + 1))
            iconW += 24
        }
        let title = NSAttributedString(string: session.title, attributes: Theme.attrs(11.5, Theme.fg, bold: true))
        let group = NSAttributedString(string: groupName, attributes: Theme.attrs(10.5, groupColor))
        let branch = NSAttributedString(string: session.branch ?? "", attributes: Theme.attrs(10.5, Theme.muted))
        // Der Titel hat Vorrang: Branch und Gruppe erscheinen nur, solange daneben noch Platz ist.
        let avail = max(0, b.width - 24 - metaW - iconW - 16)
        let titleW = min(title.size().width, avail)
        if titleW > 0 { title.draw(with: CGRect(x: 24, y: 5, width: titleW, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine]) }
        var x = 24 + titleW + 8
        var rest = avail - titleW - 8
        let branchW = branch.length == 0 ? 0 : branch.size().width + 8
        if branchW > 0 && rest >= branchW {
            branch.draw(at: CGPoint(x: x, y: 6))
            x += branchW; rest -= branchW
        }
        if rest >= min(group.size().width, 40) {
            group.draw(with: CGRect(x: x, y: 6, width: rest, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        }
        meta.draw(at: CGPoint(x: b.width - (hovered ? 44 : 0) - 9 - metaW, y: 6))
        if hovered { Icons.x(in: xRectLogical, color: Theme.sub); Icons.pen(in: penRectLogical, color: Theme.sub) }
    }

    // MARK: Accessibility

    private var a11y: [A11yElement] = []
    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .group }
    override func accessibilityLabel() -> String? { session.title }

    /// Titelzeile als Knopf (Fokus) mit Titel, Status, Gruppe und Branch, dazu Umbenennen und Schließen. Im Stack
    /// zeichnet die Arbeitsfläche die Titelzeile, dann nur das Terminal.
    override func accessibilityChildren() -> [Any]? {
        let terminal = super.accessibilityChildren() ?? []
        guard !headerHidden, let ws = superview as? WorkspaceView else { return terminal }
        let key = session.id
        let label = [session.title, attached ? session.status.spoken : ended ? String(localized: "beendet") : String(localized: "nicht gestartet"),
                     groupName.isEmpty ? nil : groupName, session.branch]
        a11y = [
            a11y.reuse("header").update(parent: self, role: .button, label: label.compactMap { $0 }.joined(separator: ", "), frame: headerRect,
                                        press: { [weak ws] in ws?.activate(key) }),
            a11y.reuse("rename").update(parent: self, role: .button, label: String(localized: "Umbenennen"), frame: penRect,
                                        press: { [weak ws] in ws?.onRenameSession?(key) }),
            a11y.reuse("close").update(parent: self, role: .button, label: String(localized: "Schließen"), frame: xRect,
                                       press: { [weak ws] in ws?.onCloseSession?(key, false) }),
        ]
        return a11y + terminal
    }

    private func drawBody(_ body: CGRect) {
        let terminalMounted = subviews.contains { $0 is KadrellTerminalView }
        if ended {
            drawHatch(in: body)
            if !lines.isEmpty { drawLines(in: body.insetBy(dx: 10, dy: 8)) }
            drawLabel(String(localized: "BEENDET · KLICK SETZT FORT"), in: body)
        } else if !attached, previewing {
            drawLabel(String(localized: "VORSCHAU · NICHT GESTARTET · ⏎ ODER KLICK STARTET"), in: body)
        } else if !attached {
            Icons.spinner(in: CGRect(x: body.midX - 12, y: body.midY - 12, width: 24, height: 24), color: Theme.sub, width: 2)
            drawLabel(String(localized: "STARTET …"), in: body)
        } else if !terminalMounted, elsewhere {
            drawLabel(String(localized: "IN ANDEREM FENSTER · KLICK HOLT HIERHER"), in: body)
        } else if !terminalMounted {
            drawLines(in: body.insetBy(dx: 10, dy: 8))
        }
    }

    private var borderColor: NSColor {
        if dropTarget { return Theme.fg }
        if focused { return groupColor }
        if session.status == .waiting, attached { return Theme.waiting }
        if session.status == .error { return Theme.error }
        return groupColor.mixed(hovered ? 0.7 : 0.45, into: Theme.bg)
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
