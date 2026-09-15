import AppKit

/// 30 px Leiste: Gruppen-Chips, Breadcrumb, Zähler, Buttons, Zoom, Attach, Uhr. Alles gezeichnet, Hit-Rects von Hand.
@MainActor
final class StatusBarView: NSView {
    var crumb: (group: String, session: String)?
    var crumbGroupAttrs: [NSAttributedString.Key: Any]?
    var sessionCount = 0
    var zoomPercent = 100
    var attachText = "attach 0/0"
    var usage = Usage.empty

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

    override func draw(_ dirtyRect: NSRect) {
        hitRects = []
        let b = bounds
        Theme.panel.setFill(); b.fill()
        Theme.line.setFill(); CGRect(x: 0, y: b.height - 1, width: b.width, height: 1).fill()
        let f = Theme.attrs(11.5, Theme.sub)
        let fMuted = Theme.attrs(11.5, Theme.muted)
        let fFg = Theme.attrs(11.5, Theme.fg, bold: true)
        let midY = b.midY

        let leftEnd: CGFloat = 8

        // Rechts: von rechts nach links
        var rx = b.width
        func module(_ parts: [NSAttributedString], action: (() -> Void)? = nil, dots: [(NSColor, Int)] = []) {
            var w: CGFloat = 20
            for p in parts { w += p.size().width }
            w += CGFloat(max(0, parts.count - 1)) * 5
            for (_, n) in dots { w += 11 + NSAttributedString(string: "\(n)", attributes: f).size().width }
            w += CGFloat(max(0, dots.count - 1)) * 6
            rx -= w
            Theme.line.setFill(); CGRect(x: rx, y: 0, width: 1, height: b.height - 1).fill()
            var px = rx + 10
            for (i, (c, n)) in dots.enumerated() {
                c.setFill(); CGRect(x: px, y: midY - 3.5, width: 7, height: 7).fill()
                px += 11
                let s = NSAttributedString(string: "\(n)", attributes: f)
                s.draw(at: CGPoint(x: px, y: midY - 8)); px += s.size().width + (i < dots.count - 1 ? 6 : 0)
            }
            for (i, p) in parts.enumerated() {
                p.draw(at: CGPoint(x: px, y: midY - 8 + (i > 0 ? 1 : 0))); px += p.size().width + 5
            }
            if let action { hitRects.append((CGRect(x: rx, y: 0, width: w, height: b.height), action)) }
        }
        let df = DateFormatter(); df.dateFormat = "HH:mm"
        module([NSAttributedString(string: df.string(from: Date()), attributes: Theme.attrs(11.5, Theme.fg, bold: true))])
        module([NSAttributedString(string: attachText, attributes: f)])
        module([NSAttributedString(string: "\(zoomPercent)%", attributes: f)])
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
            mid = m
        } else {
            mid = NSAttributedString(string: "kadrell · \(sessionCount) sessions", attributes: fMuted)
        }
        let avail = rx - leftEnd - 16
        let mw = min(mid.size().width, max(0, avail))
        mid.draw(with: CGRect(x: leftEnd + 8, y: midY - 8, width: mw, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        for (r, action) in hitRects where r.contains(p) { action(); return }
    }
}
