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
    /// Sessions, die gerade auf dich warten (Status waiting, angehängt). Modul „N warten“ nur bei > 0.
    var waitingCount = 0
    var onSelectWaiting: (() -> Void)?
    /// Neue Werte zählen vom alten Stand hoch bzw. herunter.
    var usage = Usage.empty {
        didSet { if usage != oldValue { usageFrom = oldValue; usageAt = CACurrentMediaTime(); animate(0.5) } }
    }
    private var usageFrom = Usage.empty
    private var usageAt: CFTimeInterval = 0
    var layoutMode: LayoutMode = .grid
    /// Klick aufs Layout-Symbol: Auswahl aller Layouts.
    var onPickLayout: ((LayoutMode) -> Void)?
    /// Spalten im Grid, 0 = automatisch. Nur im Grid sichtbar: ‹ weniger, › mehr.
    var gridColumns = 0
    var onGridColumns: ((Int) -> Void)?
    /// Frei: Teilung an der Fokus-Kachel, „r“ → , „d“ ↓, „a“ längere Seite. Klick schaltet reihum.
    var split: Character = "a"
    var onSplit: ((Character) -> Void)?
    /// Auto-Modus: nur Sessions, die etwas wollen. An = gefülltes Badge.
    var auto = false { didSet { toggled(auto != oldValue, "auto") } }
    var onToggleAuto: (() -> Void)?
    /// Sync: Eingaben gehen an alle Kacheln. An = rotes Badge, damit es niemand vergisst.
    var sync = false { didSet { toggled(sync != oldValue, "sync") } }
    var onToggleSync: (() -> Void)?
    /// Sortierung des Baums: Klick schaltet aus → A–Z → Status weiter. Aktiv = gefülltes Badge.
    var sort: SidebarSort = .off { didSet { toggled(sort != oldValue, "sort") } }
    var onCycleSort: (() -> Void)?
    /// Zoom aktiv: Badge „ZOOM“ links neben dem Breadcrumb, Klick hebt den Zoom auf.
    var zoomed = false
    var onToggleZoom: (() -> Void)?

    /// Trefferflächen mit Label und Wert, dieselbe Liste liefert die Knöpfe für VoiceOver.
    private var hitRects: [(rect: CGRect, label: String, value: String?, action: @MainActor () -> Void)] = []
    private var a11y: [A11yElement] = []
    private var clockTask: Task<Void, Never>?
    /// Umgeschaltete Badges: die Füllung wächst aus der Mitte auf.
    private var toggledAt: [String: CFTimeInterval] = [:]
    private var animUntil: CFTimeInterval = 0
    private var animTask: Task<Void, Never>?

    private func toggled(_ changed: Bool, _ key: String) {
        guard changed else { return }
        toggledAt[key] = CACurrentMediaTime()
        animate(0.2)
    }

    /// Kurz flüssig neu zeichnen, die Uhr allein tickt nur jede Sekunde.
    private func animate(_ duration: CFTimeInterval) {
        animUntil = max(animUntil, CACurrentMediaTime() + duration)
        guard animTask == nil else { return }
        animTask = Task { [weak self] in
            while let self, CACurrentMediaTime() < self.animUntil {
                self.needsDisplay = true
                try? await Task.sleep(for: .milliseconds(16))
            }
            self?.needsDisplay = true
            self?.animTask = nil
        }
    }

    /// Badge-Füllung, während des Umschaltens von der Mitte aus breiter.
    private func badge(_ r: CGRect, _ key: String) -> CGRect {
        let b = r.insetBy(dx: 5, dy: 6)
        guard let t = toggledAt[key], let p = Feedback.progress(since: t, duration: 0.2) else { return b }
        return b.insetBy(dx: b.width * (1 - p) / 2, dy: b.height * (1 - p) / 2)
    }

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
        hitRects.append((toggle, "Layout", layoutMode.title, { [weak self] in self?.showLayoutMenu(at: CGPoint(x: toggle.minX, y: toggle.maxY)) }))
        var x = toggle.maxX + 1
        if layoutMode == .grid {
            // ‹ AUTO › bzw. ‹ 3 SP ›: die Pfeile ändern die Spaltenzahl, unter 1 wird es wieder automatisch.
            let value = NSAttributedString(string: gridColumns == 0 ? "AUTO SP" : "\(gridColumns) SP", attributes: Theme.attrs(10, gridColumns == 0 ? Theme.muted : Theme.fg, bold: true))
            let less = NSAttributedString(string: "‹", attributes: Theme.attrs(12, Theme.sub, bold: true))
            let more = NSAttributedString(string: "›", attributes: Theme.attrs(12, Theme.sub, bold: true))
            let lessRect = CGRect(x: x, y: 0, width: less.size().width + 14, height: b.height - 1)
            less.draw(at: CGPoint(x: lessRect.minX + 7, y: midY - 9))
            value.draw(at: CGPoint(x: lessRect.maxX, y: midY - 7))
            let moreRect = CGRect(x: lessRect.maxX + value.size().width, y: 0, width: more.size().width + 14, height: b.height - 1)
            more.draw(at: CGPoint(x: moreRect.minX + 7, y: midY - 9))
            let cols = gridColumns
            hitRects.append((lessRect, "Weniger Spalten", gridColumns == 0 ? "automatisch" : "\(gridColumns)", { [weak self] in self?.onGridColumns?(max(0, cols - 1)) }))
            hitRects.append((moreRect, "Mehr Spalten", gridColumns == 0 ? "automatisch" : "\(gridColumns)", { [weak self] in self?.onGridColumns?(min(12, cols + 1)) }))
            Theme.line.setFill(); CGRect(x: moreRect.maxX, y: 0, width: 1, height: b.height - 1).fill()
            x = moreRect.maxX + 1
        }
        if layoutMode == .custom {
            let label = NSAttributedString(string: split == "r" ? "TEILT →" : split == "d" ? "TEILT ↓" : "TEILT AUTO", attributes: Theme.attrs(10, split == "a" ? Theme.muted : Theme.fg, bold: true))
            let r = CGRect(x: x, y: 0, width: label.size().width + 20, height: b.height - 1)
            label.draw(at: CGPoint(x: r.minX + 10, y: midY - 7))
            let next: Character = split == "a" ? "r" : split == "r" ? "d" : "a"
            hitRects.append((r, { [weak self] in self?.onSplit?(next) }))
            Theme.line.setFill(); CGRect(x: r.maxX, y: 0, width: 1, height: b.height - 1).fill()
            x = r.maxX + 1
        }
        let at = NSAttributedString(string: "AUTO", attributes: Theme.attrs(10, auto ? Theme.bg : Theme.muted, bold: true))
        let autoRect = CGRect(x: x, y: 0, width: at.size().width + 20, height: b.height - 1)
        if auto { Theme.waiting.setFill(); badge(autoRect, "auto").fill() }
        at.draw(at: CGPoint(x: autoRect.minX + 10, y: midY - 7))
        Theme.line.setFill(); CGRect(x: autoRect.maxX, y: 0, width: 1, height: b.height - 1).fill()
        hitRects.append((autoRect, "Auto-Modus", auto ? "an" : "aus", { [weak self] in self?.onToggleAuto?() }))
        let syt = NSAttributedString(string: "SYNC", attributes: Theme.attrs(10, sync ? Theme.bg : Theme.muted, bold: true))
        let syncRect = CGRect(x: autoRect.maxX + 1, y: 0, width: syt.size().width + 20, height: b.height - 1)
        if sync { Theme.error.setFill(); badge(syncRect, "sync").fill() }
        syt.draw(at: CGPoint(x: syncRect.minX + 10, y: midY - 7))
        Theme.line.setFill(); CGRect(x: syncRect.maxX, y: 0, width: 1, height: b.height - 1).fill()
        hitRects.append((syncRect, "Sync", sync ? "an" : "aus", { [weak self] in self?.onToggleSync?() }))
        let sortLabel = switch sort { case .off: "SORT"; case .alpha: "A–Z"; case .status: "STATUS" }
        let st = NSAttributedString(string: sortLabel, attributes: Theme.attrs(10, sort == .off ? Theme.muted : Theme.bg, bold: true))
        let sortRect = CGRect(x: syncRect.maxX + 1, y: 0, width: st.size().width + 20, height: b.height - 1)
        if sort != .off { Theme.sub.setFill(); badge(sortRect, "sort").fill() }
        st.draw(at: CGPoint(x: sortRect.minX + 10, y: midY - 7))
        Theme.line.setFill(); CGRect(x: sortRect.maxX, y: 0, width: 1, height: b.height - 1).fill()
        hitRects.append((sortRect, "Sortierung", sort == .off ? "aus" : sortLabel, { [weak self] in self?.onCycleSort?() }))
        var leftEnd = sortRect.maxX + 8
        if zoomed {
            let zt = NSAttributedString(string: "ZOOM", attributes: Theme.attrs(11, Theme.bg, bold: true))
            let z = CGRect(x: leftEnd, y: midY - 9, width: zt.size().width + 12, height: 18)
            Theme.waiting.setFill(); z.fill()
            zt.draw(at: CGPoint(x: z.minX + 6, y: midY - 8))
            hitRects.append((z, "Zoom aufheben", nil, { [weak self] in self?.onToggleZoom?() }))
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
        if waitingCount > 0 {
            let wt = NSAttributedString(string: "\(waitingCount) warten", attributes: Theme.attrs(11, Theme.bg, bold: true))
            let ww = wt.size().width + 20
            rx -= ww
            Theme.line.setFill(); CGRect(x: rx, y: 0, width: 1, height: b.height - 1).fill()
            let wr = CGRect(x: rx + 4, y: midY - 9, width: ww - 8, height: 18)
            Theme.waiting.setFill(); wr.fill()
            wt.draw(at: CGPoint(x: wr.minX + 10, y: midY - 8))
            hitRects.append((wr, "Wartende Sessions", "\(waitingCount)", { [weak self] in self?.onSelectWaiting?() }))
        }
        module([NSAttributedString(string: attachText, attributes: f)])
        // Claude-Nutzung: 5 h, 7 Tage, Fable-Woche. Fehlt ein Wert, steht „–%“ statt nichts.
        let countUp = Feedback.progress(since: usageAt, duration: 0.5)
        func pctString(_ target: Int?, from: Int?) -> NSAttributedString {
            guard let target else { return NSAttributedString(string: "–%", attributes: fMuted) }
            var v = target
            if let countUp, let from { v = from + Int((CGFloat(target - from) * countUp).rounded()) }
            let c: NSColor = v >= 90 ? Theme.error : v >= 70 ? Theme.waiting : Theme.idle
            return NSAttributedString(string: "\(v)%", attributes: Theme.attrs(11.5, c))
        }
        module([NSAttributedString(string: "fable", attributes: fMuted), pctString(usage.fable, from: usageFrom.fable)])
        module([NSAttributedString(string: "7d", attributes: fMuted), pctString(usage.weekly, from: usageFrom.weekly)])
        module([NSAttributedString(string: "5h", attributes: fMuted), pctString(usage.session, from: usageFrom.session)])

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

    private func showLayoutMenu(at p: CGPoint) {
        let menu = NSMenu()
        for m in LayoutMode.allCases {
            let item = NSMenuItem(title: m.title, action: #selector(pickLayout(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = m.rawValue
            item.state = m == layoutMode ? .on : .off
            item.image = Icons.layoutImage(m)
            menu.addItem(item)
        }
        menu.popUp(positioning: nil, at: CGPoint(x: p.x * Theme.scale, y: p.y * Theme.scale), in: self)
    }

    @objc private func pickLayout(_ item: NSMenuItem) {
        if let m = (item.representedObject as? String).flatMap(LayoutMode.init) { onPickLayout?(m) }
    }

    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .toolbar }
    override func accessibilityLabel() -> String? { "Statusleiste" }

    /// Knöpfe aus den Trefferflächen des letzten Zeichnens.
    override func accessibilityChildren() -> [Any]? {
        a11y = hitRects.map { h in
            a11y.reuse(h.label).update(parent: self, role: .button, label: h.label, value: h.value, frame: h.rect.scaled(Theme.scale), press: h.action)
        }
        return a11y
    }

    override func mouseDown(with event: NSEvent) {
        let v = convert(event.locationInWindow, from: nil)
        let p = CGPoint(x: v.x / Theme.scale, y: v.y / Theme.scale)
        for h in hitRects where h.rect.contains(p) { h.action(); return }
    }
}
