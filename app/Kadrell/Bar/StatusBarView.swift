import AppKit

/// 30 px Leiste: Layout-Toggle links, Breadcrumb in der Mitte, rechts Nutzung, Attach und Uhr.
/// Alles gezeichnet, Hit-Rects von Hand.
@MainActor
final class StatusBarView: NSView, NSViewToolTipOwner {
    var crumb: (group: String, session: String)?
    var crumbGroupAttrs: [NSAttributedString.Key: Any]?
    /// Sessions können nicht geladen werden: steht statt der Session-Zahl in der Mitte, rot.
    var errorText: String?
    /// claude läuft, ist aber älter als von Kadrell getestet: blockiert nichts, nur ein Hinweis-Badge rechts.
    var versionWarning: String?
    /// Einmaliger Tipp (zweite Session, Bedeutung von Gelb), verschwindet mit einem Klick darauf.
    var tip: String?
    var onDismissTip: (() -> Void)?
    var sessionCount = 0
    var openCount = 0
    var attachText = String(localized: "läuft \(0)/\(0)")
    /// Sessions, die gerade auf dich warten (Status waiting, angehängt). Modul „N warten“ nur bei > 0.
    var waitingCount = 0
    var onSelectWaiting: (() -> Void)?
    /// Neuere Version als die laufende gefunden (`UpdateChecker`), Klick zeigt Changelog und Download-Link.
    var updateAvailable: UpdateManifest?
    var onShowUpdate: (() -> Void)?
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
    private var hitRects: [HitRegion] = []
    /// Tooltip-Text nur für Module ohne eigenen Klick (Nutzungszahlen, Kürzel „läuft x/y“); die anklickbaren
    /// Module bekommen ihren Tooltip aus `hitRects` (Label + Wert).
    private var moduleTips: [(rect: CGRect, text: String)] = []
    /// Registrierte Tooltip-Rects fürs Nachschlagen in `view(_:stringForToolTip:point:userData:)`, nur neu
    /// gesetzt, wenn sich seit dem letzten Zeichnen etwas geändert hat: sonst reißt `removeAllToolTips()` bei
    /// jedem Uhr-Tick (jede Sekunde) den Hover-Timer ab, und der Tooltip erscheint nie.
    private var toolTipEntries: [(rect: CGRect, text: String)] = []
    private var toolTipsKey = ""
    private var a11y: [A11yElement] = []
    private var clockTask: Task<Void, Never>?
    private static let clock: DateFormatter = { let df = DateFormatter(); df.dateFormat = "HH:mm"; return df }()
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
                try? await Task.sleep(for: .seconds(1))
                guard let self else { return }
                // Fenster verdeckt/versteckt: die Uhr darf ruhig eine Sekunde nachhängen, sie zeichnet niemand.
                guard self.window?.occlusionState.contains(.visible) == true else { continue }
                self.needsDisplay = true
            }
        }
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

    /// Gezeichnet in unskalierten Punkten, die Hit-Rects auch.
    override func draw(_ dirtyRect: NSRect) {
        hitRects = []
        moduleTips = []
        Theme.scaled(bounds) { drawBar($0) }
        updateToolTips()
    }

    private func updateToolTips() {
        toolTipEntries = moduleTips + hitRects.map { h in (h.rect, h.value.map { "\(h.label): \($0)" } ?? h.label) }
        let key = toolTipEntries.map { "\($0.rect)|\($0.text)" }.joined(separator: "\n")
        guard key != toolTipsKey else { return }
        toolTipsKey = key
        removeAllToolTips()
        for t in toolTipEntries { addToolTip(t.rect.scaled(Theme.scale), owner: self, userData: nil) }
    }

    func view(_ view: NSView, stringForToolTip tag: NSView.ToolTipTag, point: NSPoint, userData data: UnsafeMutableRawPointer?) -> String {
        toolTipEntries.first { $0.rect.scaled(Theme.scale).contains(point) }?.text ?? ""
    }

    private func drawBar(_ b: CGRect) {
        Theme.panel.setFill(); b.fill()
        Theme.line.setFill(); CGRect(x: 0, y: b.height - 1, width: b.width, height: 1).fill()
        let leftEnd = drawLeft(b)
        var rx = b.width
        module(b, &rx, [NSAttributedString(string: Self.clock.string(from: Date()), attributes: Theme.attrs(11.5, Theme.fg, bold: true))])
        drawNotices(b, &rx)
        drawUsage(b, &rx)
        drawCrumb(b, from: leftEnd, to: rx)
    }

    private func divider(_ x: CGFloat, _ b: CGRect) { Theme.line.setFill(); CGRect(x: x, y: 0, width: 1, height: b.height - 1).fill() }

    // MARK: Links

    /// Layout-Toggle, Spalten bzw. Teilrichtung, AUTO/SYNC/SORT und ZOOM. Gibt das Ende der linken Seite zurück.
    private func drawLeft(_ b: CGRect) -> CGFloat {
        // Layout-Toggle: zeigt den aktuellen Modus, Klick öffnet die Auswahl.
        let toggle = CGRect(x: 0, y: 0, width: 36, height: b.height - 1)
        Icons.layout(layoutMode, in: CGRect(x: 11, y: b.midY - 7, width: 14, height: 14), color: Theme.sub)
        divider(toggle.maxX, b)
        hitRects.append(HitRegion(rect: toggle, label: String(localized: "Layout"), value: layoutMode.title) { [weak self] in self?.showLayoutMenu(at: CGPoint(x: toggle.minX, y: toggle.maxY)) })
        var x = toggle.maxX + 1
        if layoutMode == .grid { drawGridColumns(b, &x) }
        if layoutMode == .custom { drawSplit(b, &x) }
        let onOff = { (on: Bool) in on ? String(localized: "an") : String(localized: "aus") }
        segment(b, &x, "AUTO", fill: auto ? Theme.waiting : nil, key: "auto", String(localized: "Auto-Modus"), onOff(auto)) { [weak self] in self?.onToggleAuto?() }
        segment(b, &x, "SYNC", fill: sync ? Theme.error : nil, key: "sync", "Sync", onOff(sync)) { [weak self] in self?.onToggleSync?() }
        let sortLabel = switch sort { case .off: "SORT"; case .alpha: "A–Z"; case .status: "STATUS" }
        segment(b, &x, sortLabel, fill: sort == .off ? nil : Theme.sub, key: "sort", String(localized: "Sortierung"), sort == .off ? String(localized: "aus") : sortLabel) { [weak self] in self?.onCycleSort?() }
        return drawZoom(b, x: x + 7)   // 8 pt hinter der letzten Trennlinie
    }

    /// Schalter mit Trennlinie rechts. `fill` = an: gefülltes Badge (wächst beim Umschalten aus der Mitte), sonst gedämpfter Text.
    private func segment(_ b: CGRect, _ x: inout CGFloat, _ text: String, color: NSColor? = nil, fill: NSColor? = nil, key: String = "",
                         _ label: String, _ value: String?, action: @escaping @MainActor () -> Void) {
        let t = NSAttributedString(string: text, attributes: Theme.attrs(10, color ?? fill.map { Theme.pillText(on: $0) } ?? Theme.muted, bold: true))
        let r = CGRect(x: x, y: 0, width: t.size().width + 20, height: b.height - 1)
        if let fill { fill.setFill(); badge(r, key).fill() }
        t.draw(at: CGPoint(x: r.minX + 10, y: b.midY - 7))
        divider(r.maxX, b)
        hitRects.append(HitRegion(rect: r, label: label, value: value, action: action))
        x = r.maxX + 1
    }

    /// ‹ AUTO › bzw. ‹ 3 SP ›: die Pfeile ändern die Spaltenzahl, unter 1 wird es wieder automatisch.
    private func drawGridColumns(_ b: CGRect, _ x: inout CGFloat) {
        let cols = gridColumns, midY = b.midY
        let value = NSAttributedString(string: cols == 0 ? String(localized: "AUTO SP") : String(localized: "\(cols) SP"), attributes: Theme.attrs(10, cols == 0 ? Theme.muted : Theme.fg, bold: true))
        let less = NSAttributedString(string: "‹", attributes: Theme.attrs(12, Theme.sub, bold: true))
        let more = NSAttributedString(string: "›", attributes: Theme.attrs(12, Theme.sub, bold: true))
        let lessRect = CGRect(x: x, y: 0, width: less.size().width + 14, height: b.height - 1)
        less.draw(at: CGPoint(x: lessRect.minX + 7, y: midY - 9))
        value.draw(at: CGPoint(x: lessRect.maxX, y: midY - 7))
        let moreRect = CGRect(x: lessRect.maxX + value.size().width, y: 0, width: more.size().width + 14, height: b.height - 1)
        more.draw(at: CGPoint(x: moreRect.minX + 7, y: midY - 9))
        let spoken = cols == 0 ? String(localized: "automatisch") : "\(cols)"
        hitRects.append(HitRegion(rect: lessRect, label: String(localized: "Weniger Spalten"), value: spoken) { [weak self] in self?.onGridColumns?(max(0, cols - 1)) })
        hitRects.append(HitRegion(rect: moreRect, label: String(localized: "Mehr Spalten"), value: spoken) { [weak self] in self?.onGridColumns?(min(12, cols + 1)) })
        divider(moreRect.maxX, b)
        x = moreRect.maxX + 1
    }

    /// Teilrichtung der nächsten Kachel im Layout „Frei“, Klick schaltet auto → rechts → unten weiter.
    private func drawSplit(_ b: CGRect, _ x: inout CGFloat) {
        let next: Character = split == "a" ? "r" : split == "r" ? "d" : "a"
        let (text, spoken) = switch split {
        case "r": (String(localized: "TEILT →"), String(localized: "rechts"))
        case "d": (String(localized: "TEILT ↓"), String(localized: "unten"))
        default: (String(localized: "TEILT AUTO"), String(localized: "automatisch"))
        }
        segment(b, &x, text, color: split == "a" ? Theme.muted : Theme.fg, String(localized: "Nächste Kachel teilt"), spoken) { [weak self] in self?.onSplit?(next) }
    }

    private func drawZoom(_ b: CGRect, x: CGFloat) -> CGFloat {
        guard zoomed else { return x }
        let zt = NSAttributedString(string: "ZOOM", attributes: Theme.attrs(11, Theme.pillText(on: Theme.waiting), bold: true))
        let z = CGRect(x: x, y: b.midY - 9, width: zt.size().width + 12, height: 18)
        Theme.waiting.setFill(); z.fill()
        zt.draw(at: CGPoint(x: z.minX + 6, y: b.midY - 8))
        hitRects.append(HitRegion(rect: z, label: String(localized: "Zoom aufheben")) { [weak self] in self?.onToggleZoom?() })
        return z.maxX + 8
    }

    // MARK: Rechts, von rechts nach links

    /// Modul links von `rx`. `pill`: gefüllte Fläche mit 4 pt Abstand, sonst die ganze Modulhöhe.
    private func module(_ b: CGRect, _ rx: inout CGFloat, _ parts: [NSAttributedString], tip: String? = nil, pill: NSColor? = nil, textY: CGFloat = -8,
                        _ label: String? = nil, value: String? = nil, action: (@MainActor () -> Void)? = nil) {
        let w = parts.reduce(20) { $0 + $1.size().width } + CGFloat(max(0, parts.count - 1)) * 5
        rx -= w
        divider(rx, b)
        var rect = CGRect(x: rx, y: 0, width: w, height: b.height - 1), px = rx + 10
        if let pill { rect = CGRect(x: rx + 4, y: b.midY - 9, width: w - 8, height: 18); pill.setFill(); rect.fill(); px += 4 }
        for (i, p) in parts.enumerated() {
            p.draw(at: CGPoint(x: px, y: b.midY + textY + (i > 0 ? 1 : 0))); px += p.size().width + 5
        }
        if let tip { moduleTips.append((rect, tip)) }
        if let label, let action { hitRects.append(HitRegion(rect: rect, label: label, value: value, action: action)) }
    }

    /// Update, wartende Sessions, laufende Prozesse, Tipp und Versionswarnung.
    private func drawNotices(_ b: CGRect, _ rx: inout CGFloat) {
        if let update = updateAvailable {
            // Dezent statt der waiting-Farbe: kein Alarm, nur ein Hinweis.
            module(b, &rx, [NSAttributedString(string: String(localized: "\(update.version) verfügbar"), attributes: Theme.attrs(11, Theme.fg))], pill: Theme.surface,
                   String(localized: "Update verfügbar"), value: update.version) { [weak self] in self?.onShowUpdate?() }
        }
        if waitingCount > 0 {
            module(b, &rx, [NSAttributedString(string: String(localized: "\(waitingCount) warten"), attributes: Theme.attrs(11, Theme.pillText(on: Theme.waiting), bold: true))], pill: Theme.waiting,
                   String(localized: "Wartende Sessions"), value: "\(waitingCount)") { [weak self] in self?.onSelectWaiting?() }
        }
        module(b, &rx, [NSAttributedString(string: attachText, attributes: Theme.attrs(11.5, Theme.sub))], tip: String(localized: "Laufende Claude-Prozesse / Sessions"))
        if let tip {
            module(b, &rx, [NSAttributedString(string: tip + "  ×", attributes: Theme.attrs(10.5, Theme.waiting))], textY: -7,
                   String(localized: "Tipp"), value: tip) { [weak self] in self?.onDismissTip?() }
        }
        if versionWarning != nil {
            module(b, &rx, [NSAttributedString(string: String(localized: "claude alt · claude update"), attributes: Theme.attrs(10.5, Theme.waiting, bold: true))], textY: -7,
                   String(localized: "Ältere claude-Version"), value: versionWarning) { NSPasteboard.general.copy(ClaudeCLI.updateCommand) }
        }
    }

    /// Claude-Nutzung: 5 h, 7 Tage, Fable-Woche. Fehlt ein Wert, steht „–%“ statt nichts.
    private func drawUsage(_ b: CGRect, _ rx: inout CGFloat) {
        let fMuted = Theme.attrs(11.5, Theme.muted)
        let countUp = Feedback.progress(since: usageAt, duration: 0.5)
        func pctString(_ target: Int?, from: Int?) -> NSAttributedString {
            guard let target else { return NSAttributedString(string: "–%", attributes: fMuted) }
            var v = target
            if let countUp, let from { v = from + Int((CGFloat(target - from) * countUp).rounded()) }
            let c: NSColor = v >= 90 ? Theme.error : v >= 70 ? Theme.waiting : Theme.idle
            return NSAttributedString(string: "\(v)%", attributes: Theme.attrs(11.5, c))
        }
        /// Tooltip: Bezeichnung plus Prozentwert, oder Hinweis, wenn er fehlt.
        func usageTip(_ label: String, _ pct: Int?) -> String {
            guard let pct else { return String(localized: "\(label): Nutzung nicht abrufbar") }
            return String(localized: "\(label): \(pct) % verbraucht")
        }
        let rows: [(String, Int?, Int?, String)] = [
            ("fable", usage.fable, usageFrom.fable, String(localized: "Fable-Kontingent")),
            ("7d", usage.weekly, usageFrom.weekly, String(localized: "Claude-Nutzung der letzten 7 Tage")),
            ("5h", usage.session, usageFrom.session, String(localized: "Claude-Nutzung der letzten 5 Stunden")),
        ]
        for (name, pct, from, label) in rows {
            module(b, &rx, [NSAttributedString(string: name, attributes: fMuted), pctString(pct, from: from)], tip: usageTip(label, pct))
        }
    }

    // MARK: Mitte

    /// Breadcrumb mittig zwischen linker und rechter Seite, gekürzt, wenn der Platz fehlt.
    private func drawCrumb(_ b: CGRect, from leftEnd: CGFloat, to rx: CGFloat) {
        let fMuted = Theme.attrs(11.5, Theme.muted)
        let mid: NSAttributedString
        if let c = crumb {
            let m = NSMutableAttributedString(string: c.group + " › ", attributes: crumbGroupAttrs ?? fMuted)
            m.append(NSAttributedString(string: c.session, attributes: Theme.attrs(11.5, Theme.fg, bold: true)))
            if openCount > 1 { m.append(NSAttributedString(string: String(localized: " · \(openCount) offen"), attributes: fMuted)) }
            mid = m
        } else if let e = errorText {
            mid = NSAttributedString(string: String(localized: "kadrell · \(e)"), attributes: Theme.attrs(11.5, Theme.error))
        } else {
            mid = NSAttributedString(string: String(localized: "kadrell · \(sessionCount) sessions"), attributes: fMuted)
        }
        let avail = rx - leftEnd - 16
        let mw = min(mid.size().width, max(0, avail))
        mid.draw(with: CGRect(x: leftEnd + (avail - mw) / 2, y: b.midY - 8, width: mw, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
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
    override func accessibilityLabel() -> String? { String(localized: "Statusleiste") }

    /// Knöpfe aus den Trefferflächen des letzten Zeichnens.
    override func accessibilityChildren() -> [Any]? {
        a11y = hitRects.accessibilityElements(parent: self, reusing: a11y)
        return a11y
    }

    override func mouseDown(with event: NSEvent) {
        let v = convert(event.locationInWindow, from: nil)
        let p = CGPoint(x: v.x / Theme.scale, y: v.y / Theme.scale)
        hitRects.first(at: p)?.action()
    }
}
