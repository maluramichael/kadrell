import AppKit

/// 30 px Leiste: Layout-Toggle links, Breadcrumb in der Mitte, rechts Nutzung, Attach und Uhr.
/// Alles gezeichnet, Hit-Rects von Hand.
@MainActor
final class StatusBarView: NSView {
    var crumb: (group: String, session: String)?
    var crumbGroupAttrs: [NSAttributedString.Key: Any]?
    /// Sessions können nicht geladen werden: steht statt der Session-Zahl in der Mitte, rot.
    var errorText: String?
    var sessionCount = 0
    var openCount = 0
    var attachText = "läuft 0/0"
    var usage = Usage.empty
    var layoutMode: LayoutMode = .grid
    var onToggleLayout: (() -> Void)?
    /// Auto-Modus: nur Sessions, die etwas wollen. An = gefülltes Badge.
    var auto = false
    var onToggleAuto: (() -> Void)?
    /// Sync: Eingaben gehen an alle Kacheln. An = rotes Badge, damit es niemand vergisst.
    var sync = false
    var onToggleSync: (() -> Void)?
    /// Sortierung des Baums: Klick schaltet aus → A–Z → Status weiter. Aktiv = gefülltes Badge.
    var sort: SidebarSort = .off
    var onCycleSort: (() -> Void)?
    /// Zoom aktiv: Badge „ZOOM“ links neben dem Breadcrumb, Klick hebt den Zoom auf.
    var zoomed = false
    var onToggleZoom: (() -> Void)?

    private var hitRects: [(CGRect, () -> Void)] = []
    private var clockTask: Task<Void, Never>?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                self?.needsDisplay = true
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

    /// Gezeichnet in unskalierten Punkten, die Hit-Rects auch.
    override func draw(_ dirtyRect: NSRect) {
        hitRects = []
        Theme.scaled(bounds) { drawBar($0) }
    }

    private func drawBar(_ b: CGRect) {
        Theme.panel.setFill(); b.fill()
        Theme.line.setFill(); CGRect(x: 0, y: b.height - 1, width: b.width, height: 1).fill()
        let f = Theme.attrs(11.5, Theme.sub)
        let fMuted = Theme.attrs(11.5, Theme.muted)
        let fFg = Theme.attrs(11.5, Theme.fg, bold: true)
        let midY = b.midY

        // Links: Layout-Toggle, zeigt den aktuellen Modus, Klick wechselt zum anderen.
        let toggle = CGRect(x: 0, y: 0, width: 36, height: b.height - 1)
        Icons.layout(layoutMode, in: CGRect(x: 11, y: midY - 7, width: 14, height: 14), color: Theme.sub)
        Theme.line.setFill(); CGRect(x: toggle.maxX, y: 0, width: 1, height: b.height - 1).fill()
        hitRects.append((toggle, { [weak self] in self?.onToggleLayout?() }))
        let at = NSAttributedString(string: "AUTO", attributes: Theme.attrs(10, auto ? Theme.bg : Theme.muted, bold: true))
        let autoRect = CGRect(x: toggle.maxX + 1, y: 0, width: at.size().width + 20, height: b.height - 1)
        if auto { Theme.waiting.setFill(); autoRect.insetBy(dx: 5, dy: 6).fill() }
        at.draw(at: CGPoint(x: autoRect.minX + 10, y: midY - 7))
        Theme.line.setFill(); CGRect(x: autoRect.maxX, y: 0, width: 1, height: b.height - 1).fill()
        hitRects.append((autoRect, { [weak self] in self?.onToggleAuto?() }))
        let syt = NSAttributedString(string: "SYNC", attributes: Theme.attrs(10, sync ? Theme.bg : Theme.muted, bold: true))
        let syncRect = CGRect(x: autoRect.maxX + 1, y: 0, width: syt.size().width + 20, height: b.height - 1)
        if sync { Theme.error.setFill(); syncRect.insetBy(dx: 5, dy: 6).fill() }
        syt.draw(at: CGPoint(x: syncRect.minX + 10, y: midY - 7))
        Theme.line.setFill(); CGRect(x: syncRect.maxX, y: 0, width: 1, height: b.height - 1).fill()
        hitRects.append((syncRect, { [weak self] in self?.onToggleSync?() }))
        let sortLabel = switch sort { case .off: "SORT"; case .alpha: "A–Z"; case .status: "STATUS" }
        let st = NSAttributedString(string: sortLabel, attributes: Theme.attrs(10, sort == .off ? Theme.muted : Theme.bg, bold: true))
        let sortRect = CGRect(x: syncRect.maxX + 1, y: 0, width: st.size().width + 20, height: b.height - 1)
        if sort != .off { Theme.sub.setFill(); sortRect.insetBy(dx: 5, dy: 6).fill() }
        st.draw(at: CGPoint(x: sortRect.minX + 10, y: midY - 7))
        Theme.line.setFill(); CGRect(x: sortRect.maxX, y: 0, width: 1, height: b.height - 1).fill()
        hitRects.append((sortRect, { [weak self] in self?.onCycleSort?() }))
        var leftEnd = sortRect.maxX + 8
        if zoomed {
            let zt = NSAttributedString(string: "ZOOM", attributes: Theme.attrs(11, Theme.bg, bold: true))
            let z = CGRect(x: leftEnd, y: midY - 9, width: zt.size().width + 12, height: 18)
            Theme.waiting.setFill(); z.fill()
            zt.draw(at: CGPoint(x: z.minX + 6, y: midY - 8))
            hitRects.append((z, { [weak self] in self?.onToggleZoom?() }))
            leftEnd = z.maxX + 8
        }

        // Rechts: von rechts nach links
        var rx = b.width
        func module(_ parts: [NSAttributedString]) {
            var w: CGFloat = 20
            for p in parts { w += p.size().width }
            w += CGFloat(max(0, parts.count - 1)) * 5
            rx -= w
            Theme.line.setFill(); CGRect(x: rx, y: 0, width: 1, height: b.height - 1).fill()
            var px = rx + 10
            for (i, p) in parts.enumerated() {
                p.draw(at: CGPoint(x: px, y: midY - 8 + (i > 0 ? 1 : 0))); px += p.size().width + 5
            }
        }
        let df = DateFormatter(); df.dateFormat = "HH:mm"
        module([NSAttributedString(string: df.string(from: Date()), attributes: Theme.attrs(11.5, Theme.fg, bold: true))])
        module([NSAttributedString(string: attachText, attributes: f)])
        // Claude-Nutzung: 5 h, 7 Tage, Fable-Woche. Fehlt ein Wert, steht „–%“ statt nichts.
        func pctString(_ v: Int?) -> NSAttributedString {
            guard let v else { return NSAttributedString(string: "–%", attributes: fMuted) }
            let c: NSColor = v >= 90 ? Theme.error : v >= 70 ? Theme.waiting : Theme.idle
            return NSAttributedString(string: "\(v)%", attributes: Theme.attrs(11.5, c))
        }
        module([NSAttributedString(string: "fable", attributes: fMuted), pctString(usage.fable)])
        module([NSAttributedString(string: "7d", attributes: fMuted), pctString(usage.weekly)])
        module([NSAttributedString(string: "5h", attributes: fMuted), pctString(usage.session)])

        // Mitte: Breadcrumb
        let mid: NSAttributedString
        if let c = crumb {
            let m = NSMutableAttributedString(string: c.group + " › ", attributes: crumbGroupAttrs ?? fMuted)
            m.append(NSAttributedString(string: c.session, attributes: fFg))
            if openCount > 1 { m.append(NSAttributedString(string: " · \(openCount) offen", attributes: fMuted)) }
            mid = m
        } else if let e = errorText {
            mid = NSAttributedString(string: "kadrell · \(e)", attributes: Theme.attrs(11.5, Theme.error))
        } else {
            mid = NSAttributedString(string: "kadrell · \(sessionCount) sessions", attributes: fMuted)
        }
        let avail = rx - leftEnd - 16
        let mw = min(mid.size().width, max(0, avail))
        mid.draw(with: CGRect(x: leftEnd + (avail - mw) / 2, y: midY - 8, width: mw, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    override func mouseDown(with event: NSEvent) {
        let v = convert(event.locationInWindow, from: nil)
        let p = CGPoint(x: v.x / Theme.scale, y: v.y / Theme.scale)
        for (r, action) in hitRects where r.contains(p) { action(); return }
    }
}
