import AppKit

/// Welt, Zoom, Pan, Hit-Test und Animation. Kacheln und Gruppen sind Subviews, deren Frames pro
/// Frame aus dem Welt-Layout und dem aktuellen Maßstab gesetzt werden (kein Layer-Scaling).
@MainActor
final class CanvasView: NSView {
    private(set) var groups: [Group] = []
    private(set) var sessions: [String: Session] = [:]
    var attach: AttachManager?

    private(set) var scale: CGFloat = 1
    private(set) var offset = CGPoint.zero
    private(set) var focusedKey: String?
    private(set) var layout = Layout()
    private var groupViews: [String: GroupView] = [:]
    private var cellViews: [String: CellView] = [:]
    private var hoveredCell: String?
    private var hoveredGroup: String?
    private var highlightKeys: Set<String>?
    private var pulse: CGFloat = 1

    private var displayLink: CADisplayLink?
    private var anim: (from: (CGFloat, CGPoint), to: (CGFloat, CGPoint), start: CFTimeInterval, duration: CFTimeInterval, completion: (() -> Void)?)?
    /// Rad und Pinch setzen nur ein Ziel; der Display-Link fährt weich hinterher (kein SIGWINCH-Gewitter).
    private var smoothTarget: (scale: CGFloat, offset: CGPoint)?
    var isAnimating: Bool { anim != nil || smoothTarget != nil }
    private var drag: (start: CGPoint, offset: CGPoint, moved: Bool)?
    private var groupDrag: (id: String, frame: CGRect, resize: Bool)?
    private var pulseTask: Task<Void, Never>?
    private var wheelMonitor: Any?
    private var keyMonitor: Any?

    var onFocusChange: ((String?) -> Void)?
    var onViewChange: (() -> Void)?
    var onNewSession: ((String?) -> Void)?
    var onEditGroup: ((String) -> Void)?
    /// Zweiter Parameter: ⌘ gehalten, dann ohne Rückfrage ausführen.
    var onCloseGroup: ((String, Bool) -> Void)?
    var onCloseSession: ((String, Bool) -> Void)?
    var onActivateSession: ((Session) -> Void)?
    var onHelp: (() -> Void)?
    /// Gruppe per Drag verschoben oder in der Größe geändert (Weltkoordinaten), zum Persistieren.
    var onGroupFrameChange: ((String, CGRect) -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        pulseTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(80))
                self?.tickPulse()
            }
        }
        // ⌘ + Rad zoomt auch über einem eingehängten Terminal (das sonst selbst scrollt).
        wheelMonitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
            guard let self, event.modifierFlags.contains(.command), event.window === self.window else { return event }
            self.scrollWheel(with: event)
            return nil
        }
        // ⌘Esc verlässt den Fokus, egal ob Terminal oder Canvas die Tastatur hat. Esc allein geht an Claude.
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === self.window else { return event }
            if event.keyCode == 53, event.modifierFlags.contains(.command) { self.escapeStep(); return nil }
            if event.keyCode == 122 { self.onHelp?(); return nil }   // F1
            return event
        }
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    // MARK: Daten

    func reload(groups: [Group], sessions: [Session]) {
        self.groups = groups
        self.sessions = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        let groupIds = Set(groups.map(\.id))
        for (id, v) in groupViews where !groupIds.contains(id) { v.removeFromSuperview(); groupViews[id] = nil }
        for g in groups {
            if let v = groupViews[g.id] { v.group = g } else {
                let v = GroupView(group: g)
                groupViews[g.id] = v
                addSubview(v, positioned: .below, relativeTo: nil)
            }
        }
        let keys = Set(self.sessions.keys)
        for (k, v) in cellViews where !keys.contains(k) { v.removeFromSuperview(); cellViews[k] = nil }
        for s in sessions {
            if let v = cellViews[s.id] { v.session = s } else {
                let v = CellView(session: s)
                cellViews[s.id] = v
                addSubview(v)
            }
        }
        if let f = focusedKey, self.sessions[f] == nil { focusedKey = nil; onFocusChange?(nil) }
        applyLayout()
    }

    func group(forSession key: String) -> Group? { groups.first { $0.sessionIds.contains(key) } }
    func session(_ key: String) -> Session? { sessions[key] }
    var sessionCount: Int { sessions.count }
    /// Session-Keys sortiert nach Abstand zur Viewport-Mitte (Reihenfolge der Attach-Queue).
    func sessionsByDistanceToCenter() -> [Session] {
        let c = CGPoint(x: bounds.midX, y: bounds.midY)
        return sessions.values.sorted {
            let a = cellViews[$0.id]?.frame ?? .zero, b = cellViews[$1.id]?.frame ?? .zero
            return hypot(a.midX - c.x, a.midY - c.y) < hypot(b.midX - c.x, b.midY - c.y)
        }
    }

    func setHighlight(_ keys: Set<String>?) {
        highlightKeys = keys
        applyLayout()
    }

    // MARK: Layout

    private func layoutInputs() -> [Layout.GroupInput] {
        groups.map { g in
            Layout.GroupInput(id: g.id, cellKeys: g.sessionIds.filter { sessions[$0] != nil },
                              frame: dragFrames[g.id] ?? g.frame?.rect ?? CGRect(origin: .zero, size: Layout.defaultGroupSize))
        }
    }

    /// Während eines Drags gilt das temporäre Frame, bis der Store es übernimmt.
    private var dragFrames: [String: CGRect] = [:]

    private func computeLayout(scale s: CGFloat) -> Layout {
        let aspect = bounds.height > 0 ? bounds.width / bounds.height : 1.6
        return Layout.compute(groups: layoutInputs(), scale: s, columns: Settings.columns, focused: focusedKey, viewportAspect: aspect)
    }

    private func toScreen(_ r: CGRect) -> CGRect {
        CGRect(x: r.minX * scale + offset.x, y: r.minY * scale + offset.y, width: r.width * scale, height: r.height * scale)
    }

    func applyLayout() {
        layout = computeLayout(scale: scale)
        let baseLod = Layout.lod(cellScreenWidth: 320 * scale)
        for g in groups {
            guard let v = groupViews[g.id], let r = layout.groups[g.id] else { continue }
            let f = toScreen(r).integral
            if v.frame != f { v.frame = f }
            v.headerRect = toScreen(layout.headers[g.id] ?? .zero).offsetBy(dx: -f.minX, dy: -f.minY)
            v.count = g.sessionIds.filter { sessions[$0] != nil }.count
            v.lod = baseLod
            v.hovered = hoveredGroup == g.id
            v.dimmed = false
            v.needsDisplay = true
        }
        for (key, v) in cellViews {
            guard let r = layout.cells[key], let s = sessions[key] else { v.isHidden = true; continue }
            v.isHidden = false
            let f = toScreen(r).integral
            if v.frame != f { v.frame = f }
            let g = group(forSession: key)
            v.groupName = g?.name ?? ""
            v.groupColor = NSColor(hexString: g?.color ?? "#6c7086")
            v.lod = focusedKey == key ? 3 : Layout.lod(cellScreenWidth: f.width)
            v.focused = focusedKey == key
            v.hovered = hoveredCell == key
            v.attached = attach?.isAttached(key) ?? false
            v.keyboardFocus = attach?.terminal(for: key).map { $0 === window?.firstResponder } ?? false
            v.lines = attach?.lines(for: key) ?? []
            v.highlight = highlightKeys.map { $0.contains(key) }
            v.pulse = pulse
            v.textScale = (Settings.zoomMode == .geometric && focusedKey != key) ? scale : 1
            mountTerminal(for: key, in: v, session: s)
            v.needsDisplay = true
        }
        // Verschwindet ein Terminal (Session beendet, Attach-Client weg), fällt der First Responder aufs
        // Fenster zurück und Esc käme nirgends an. Dann holt sich die Canvas die Tastatur zurück.
        if let w = window, w.firstResponder === w { w.makeFirstResponder(self) }
        onViewChange?()
        ensureFocusedFills()
    }

    /// Layout-Modus: Terminal nur eingehängt, wenn die Kachel ≥ 320 px breit ist und keine Animation läuft;
    /// Schrift immer 12 pt. Geometrischer Modus: in Ruhe Schrift = 12 × Maßstab und Frame = Kachel, damit
    /// Spalten und Zeilen konstant bleiben (SwiftTerm zeichnet nur innerhalb von `visibleRect`, deshalb darf
    /// der Frame nie größer als die Kachel sein); während der Bewegung bleibt das Terminal eingehängt und
    /// wird nur per Layer skaliert. Unter 3 px Schrift zeigt die Kachel den skalierten Snapshot.
    private func mountTerminal(for key: String, in cell: CellView, session: Session) {
        guard let t = attach?.terminal(for: key) else { return }
        func unmount() {
            guard t.superview != nil else { return }
            if window?.firstResponder === t { window?.makeFirstResponder(self) }
            t.layer?.transform = CATransform3DIdentity
            t.removeFromSuperview()
        }
        if Settings.zoomMode == .geometric, focusedKey != key {
            let fontSize = 12 * scale
            guard fontSize >= 3, cell.lod >= 1 else { unmount(); return }
            if isAnimating {
                guard t.superview === cell, t.restFontScale > 0 else { return }
                let k = scale / t.restFontScale
                t.frame.origin = cell.terminalRect.origin
                t.layer?.transform = CATransform3DMakeScale(k, k, 1)
                return
            }
            if t.superview !== cell { cell.addSubview(t) }
            t.layer?.transform = CATransform3DIdentity
            t.restFontScale = scale
            t.passthroughMouse = false
            if abs(t.font.pointSize - fontSize) > 0.05 { t.font = Theme.font(fontSize) }
            let body = cell.terminalRect
            if t.frame != body { t.frame = body }
            return
        }
        let want = cell.lod >= 3 && !isAnimating
        if want {
            if t.superview !== cell { cell.addSubview(t) }
            t.layer?.transform = CATransform3DIdentity
            t.restFontScale = 1
            t.passthroughMouse = false
            if abs(t.font.pointSize - 12) > 0.05 { t.font = Theme.font(12) }
            let body = cell.terminalRect
            if t.frame != body { t.frame = body }
        } else {
            unmount()
        }
    }

    override func setFrameSize(_ newSize: NSSize) {
        let changed = newSize != frame.size
        super.setFrameSize(newSize)
        guard changed, !groups.isEmpty else { return }
        if let f = focusedKey { fit(pad: 0, animated: false) { $0.cells[f] } } else { fitAll(animated: false) }
    }

    /// Im Fokus füllt die Kachel exakt den Viewport. Driftet sie (Resize, Zoom-Reste), einmal nachpassen.
    private var refitting = false
    private func ensureFocusedFills() {
        guard let f = focusedKey, !isAnimating, !refitting, let v = cellViews[f] else { return }
        let want = bounds
        if abs(v.frame.minX - want.minX) > 1 || abs(v.frame.minY - want.minY) > 1 ||
            abs(v.frame.width - want.width) > 2 || abs(v.frame.height - want.height) > 2 {
            refitting = true
            fit(pad: 0, animated: false) { $0.cells[f] }
            refitting = false
        }
    }

    // MARK: Ansicht

    private func setView(scale s: CGFloat, offset o: CGPoint) {
        scale = Layout.clamp(s)
        offset = o
        applyLayout()
        needsDisplay = true
    }

    func zoom(by factor: CGFloat, at p: CGPoint) {
        if focusedKey != nil { unfocus() }
        let s = Layout.clamp(scale * factor)
        let k = s / scale
        setView(scale: s, offset: CGPoint(x: p.x - (p.x - offset.x) * k, y: p.y - (p.y - offset.y) * k))
    }

    /// Wie `zoom`, aber weich: rechnet vom bisherigen Ziel aus, damit schnelle Rad-Ereignisse sich summieren.
    func zoomSmooth(by factor: CGFloat, at p: CGPoint) {
        let (s0, o0) = smoothTarget ?? (scale, offset)
        let s = Layout.clamp(s0 * factor)
        let k = s / s0
        setSmoothTarget(s, CGPoint(x: p.x - (p.x - o0.x) * k, y: p.y - (p.y - o0.y) * k))
    }

    func panSmooth(by d: CGPoint) {
        let (s0, o0) = smoothTarget ?? (scale, offset)
        setSmoothTarget(s0, CGPoint(x: o0.x + d.x, y: o0.y + d.y))
    }

    private func setSmoothTarget(_ s: CGFloat, _ o: CGPoint) {
        if focusedKey != nil { unfocus() }
        anim = nil
        smoothTarget = (s, o)
        if displayLink == nil {
            let link = displayLink(target: self, selector: #selector(tick(_:)))
            link.add(to: .main, forMode: .common)
            displayLink = link
        }
    }

    func pan(by d: CGPoint) { setView(scale: scale, offset: CGPoint(x: offset.x + d.x, y: offset.y + d.y)) }

    /// Fit im Zielmaßstab rechnen: das Layout hängt vom Maßstab ab, deshalb iterieren.
    private func fit(pad: CGFloat, animated: Bool, completion: (() -> Void)? = nil, rect: (Layout) -> CGRect?) {
        var s = scale, o = offset
        for _ in 0..<4 {
            guard let r = rect(computeLayout(scale: s)), r.width > 0, r.height > 0 else { return }
            (s, o) = Layout.fit(r, in: bounds.size, pad: pad)
        }
        animate(to: s, offset: o, duration: animated ? 0.35 : 0, completion: completion)
    }

    func fitAll(animated: Bool = true) {
        unfocus()
        fit(pad: 12, animated: animated) { $0.content }
    }

    func fitGroup(_ id: String, animated: Bool = true) {
        unfocus()
        fit(pad: 6, animated: animated) { $0.groups[id] }
    }

    func focus(_ key: String) {
        guard let s = sessions[key] else { return }
        focusedKey = key
        attach?.attachNow(s)
        onFocusChange?(key)
        fit(pad: 0, animated: true, completion: { [weak self] in
            guard let self, self.focusedKey == key, let t = self.attach?.terminal(for: key) else { return }
            self.window?.makeFirstResponder(t)
        }) { $0.cells[key] }
    }

    func unfocus() {
        guard focusedKey != nil else { return }
        focusedKey = nil
        window?.makeFirstResponder(self)
        onFocusChange?(nil)
    }

    func escapeStep() {
        if let f = focusedKey {
            let g = group(forSession: f)
            unfocus()
            if let g { fitGroup(g.id) } else { fitAll() }
        } else {
            fitAll()
        }
    }

    // MARK: Animation

    private func animate(to s: CGFloat, offset o: CGPoint, duration: CFTimeInterval, completion: (() -> Void)? = nil) {
        stopAnimation()
        if duration <= 0 {
            setView(scale: s, offset: o)
            completion?()
            return
        }
        anim = ((scale, offset), (Layout.clamp(s), o), CACurrentMediaTime(), duration, completion)
        let link = displayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
        applyLayout()
    }

    private func stopAnimation() {
        displayLink?.invalidate()
        displayLink = nil
        anim = nil
        smoothTarget = nil
    }

    @objc private func tick(_ link: CADisplayLink) {
        if let t = smoothTarget {
            let f: CGFloat = 0.28
            let ds = t.scale - scale, dx = t.offset.x - offset.x, dy = t.offset.y - offset.y
            if abs(ds) < scale * 0.001, abs(dx) < 0.3, abs(dy) < 0.3 {
                smoothTarget = nil
                setView(scale: t.scale, offset: t.offset)
                if anim == nil { displayLink?.invalidate(); displayLink = nil }
            } else {
                setView(scale: scale + ds * f, offset: CGPoint(x: offset.x + dx * f, y: offset.y + dy * f))
            }
            return
        }
        guard let a = anim else { stopAnimation(); return }
        let k = min(1, (CACurrentMediaTime() - a.start) / a.duration)
        let e = 1 - pow(1 - k, 3)
        let s = a.from.0 + (a.to.0 - a.from.0) * e
        let o = CGPoint(x: a.from.1.x + (a.to.1.x - a.from.1.x) * e, y: a.from.1.y + (a.to.1.y - a.from.1.y) * e)
        if k >= 1 {
            stopAnimation()
            setView(scale: a.to.0, offset: a.to.1)
            a.completion?()
        } else {
            setView(scale: s, offset: o)
        }
    }

    private weak var lastFirstResponder: NSResponder?
    private func tickPulse() {
        // Klick in ein Terminal macht es still zum First Responder: Rahmen der Kachel nachziehen.
        if window?.firstResponder !== lastFirstResponder {
            lastFirstResponder = window?.firstResponder
            for (key, v) in cellViews {
                let has = attach?.terminal(for: key).map { $0 === window?.firstResponder } ?? false
                if v.keyboardFocus != has { v.keyboardFocus = has; v.needsDisplay = true }
            }
        }
        let t = CACurrentMediaTime().truncatingRemainder(dividingBy: 1.2) / 1.2
        pulse = 0.3 + 0.7 * (0.5 + 0.5 * cos(2 * .pi * t))
        for (key, v) in cellViews where sessions[key]?.status == .running && v.lod >= 2 && !v.isHidden {
            v.pulse = pulse
            v.setNeedsDisplay(v.dotRect)
        }
        for (_, v) in cellViews where v.lod == 0 && !v.isHidden { v.pulse = pulse; v.needsDisplay = true }
    }

    // MARK: Hit-Test

    enum Hit { case cell(String), cellClose(String), header(String), pen(String), close(String), plus(String), resize(String), none }

    func hit(at p: CGPoint) -> Hit {
        for (key, v) in cellViews where !v.isHidden && v.frame.contains(p) {
            let local = CGPoint(x: p.x - v.frame.minX, y: p.y - v.frame.minY)
            if v.lod >= 2, v.xRect.insetBy(dx: -4, dy: -4).contains(local), hoveredCell == key { return .cellClose(key) }
            return .cell(key)
        }
        for (id, v) in groupViews where v.frame.contains(p) {
            let local = CGPoint(x: p.x - v.frame.minX, y: p.y - v.frame.minY)
            if v.lod >= 1 {
                if v.plusRect.insetBy(dx: -4, dy: -4).contains(local) { return .plus(id) }
                if v.penRect.insetBy(dx: -4, dy: -4).contains(local) { return .pen(id) }
                if v.xRect.insetBy(dx: -4, dy: -4).contains(local) { return .close(id) }
                if v.resizeRect.insetBy(dx: -4, dy: -4).contains(local) { return .resize(id) }
            }
            if v.headerRect.insetBy(dx: 0, dy: -6).contains(local) { return .header(id) }
        }
        return .none
    }

    // MARK: Events

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for t in trackingAreas { removeTrackingArea(t) }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    override func mouseMoved(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        var cell: String?, group: String?
        for (key, v) in cellViews where !v.isHidden && v.frame.contains(p) { cell = key; break }
        if cell == nil { for (id, v) in groupViews where v.frame.contains(p) { group = id; break } }
        // Cursor nach Ziel: Icons Zeigehand, Kacheln Pfeil, Griff Kreuz, Kopf und Leere Greifhand.
        switch hit(at: p) {
        case .pen, .close, .plus, .cellClose: NSCursor.pointingHand.set()
        case .cell: NSCursor.arrow.set()
        case .resize: NSCursor.crosshair.set()
        case .header, .none: NSCursor.openHand.set()
        }
        if cell != hoveredCell || group != hoveredGroup {
            let old = (hoveredCell, hoveredGroup)
            hoveredCell = cell; hoveredGroup = group
            for k in [old.0, cell].compactMap({ $0 }) { cellViews[k]?.hovered = hoveredCell == k; cellViews[k]?.needsDisplay = true }
            for g in [old.1, group].compactMap({ $0 }) { groupViews[g]?.hovered = hoveredGroup == g; groupViews[g]?.needsDisplay = true }
        }
    }

    override func mouseExited(with event: NSEvent) {
        if let c = hoveredCell { cellViews[c]?.hovered = false; cellViews[c]?.needsDisplay = true }
        if let g = hoveredGroup { groupViews[g]?.hovered = false; groupViews[g]?.needsDisplay = true }
        hoveredCell = nil; hoveredGroup = nil
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        let p = convert(event.locationInWindow, from: nil)
        drag = (p, offset, false)
        groupDrag = nil
        switch hit(at: p) {
        case .header(let g): if let f = layout.groups[g] { groupDrag = (g, f, false) }
        case .resize(let g): if let f = layout.groups[g] { groupDrag = (g, f, true) }
        default: break
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard var d = drag else { return }
        let p = convert(event.locationInWindow, from: nil)
        let dx = p.x - d.start.x, dy = p.y - d.start.y
        if !d.moved, hypot(dx, dy) < 4 { return }
        d.moved = true
        drag = d
        stopAnimation()
        if let gd = groupDrag {
            // Gruppe verschieben oder an der Ecke größer ziehen (Deltas in Weltpunkte umrechnen)
            var f = gd.frame
            if gd.resize {
                f.size.width = max(Layout.minGroupSize.width, gd.frame.width + dx / scale)
                f.size.height = max(Layout.minGroupSize.height, gd.frame.height + dy / scale)
            } else {
                f.origin = CGPoint(x: gd.frame.minX + dx / scale, y: gd.frame.minY + dy / scale)
            }
            dragFrames[gd.id] = f
            applyLayout()
            return
        }
        setView(scale: scale, offset: CGPoint(x: d.offset.x + dx, y: d.offset.y + dy))
    }

    override func mouseUp(with event: NSEvent) {
        let wasDrag = drag?.moved ?? false
        drag = nil
        if let gd = groupDrag {
            groupDrag = nil
            if wasDrag, let f = dragFrames[gd.id] {
                if let i = groups.firstIndex(where: { $0.id == gd.id }) { groups[i].frame = GroupFrame(f) }
                dragFrames[gd.id] = nil
                onGroupFrameChange?(gd.id, f)
                applyLayout()
                return
            }
        }
        if wasDrag { return }
        let p = convert(event.locationInWindow, from: nil)
        let force = event.modifierFlags.contains(.command)
        switch hit(at: p) {
        case .cellClose(let k): onCloseSession?(k, force)
        case .cell(let k):
            guard k != focusedKey, let s = sessions[k] else { return }
            if s.isDone || s.isStale { onActivateSession?(s) } else { focus(k) }
        case .header(let g): fitGroup(g)
        case .pen(let g): onEditGroup?(g)
        case .close(let g): onCloseGroup?(g, force)
        case .plus(let g): onNewSession?(g)
        case .resize, .none: break
        }
    }

    override func scrollWheel(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        if event.modifierFlags.contains(.shift) {
            let d = event.scrollingDeltaX != 0 ? event.scrollingDeltaX : event.scrollingDeltaY
            panSmooth(by: CGPoint(x: d, y: 0))
            return
        }
        // Rad und Trackpad zoomen immer um den Zeiger (Shift pannt, Drag pannt).
        let k: CGFloat = event.hasPreciseScrollingDeltas ? 0.005 : 0.15
        zoomSmooth(by: exp(event.scrollingDeltaY * k), at: p)
    }

    override func magnify(with event: NSEvent) {
        zoomSmooth(by: 1 + event.magnification, at: convert(event.locationInWindow, from: nil))
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { escapeStep(); return }
        if event.modifierFlags.contains(.command) { super.keyDown(with: event); return }
        let center = CGPoint(x: bounds.midX, y: bounds.midY)
        switch event.charactersIgnoringModifiers {
        case "f": fitAll()
        case "+", "=": zoomSmooth(by: 1.3, at: center)
        case "-": zoomSmooth(by: 1 / 1.3, at: center)
        default: super.keyDown(with: event)
        }
    }


    // MARK: Hintergrund: feines Linienraster in Weltkoordinaten


    override func draw(_ dirtyRect: NSRect) {
        Theme.bg.setFill()
        dirtyRect.fill()
        guard scale > 0 else { return }
        // Linienraster in Weltpunkten, Abstand so, dass auf dem Bildschirm mindestens 36 px bleiben.
        var step: CGFloat = 40
        while step * scale < 36 { step *= 2 }
        let grid = NSBezierPath()
        var wx = floor((dirtyRect.minX - offset.x) / scale / step) * step
        while wx * scale + offset.x <= dirtyRect.maxX {
            let x = (wx * scale + offset.x).rounded() + 0.5
            grid.move(to: CGPoint(x: x, y: dirtyRect.minY)); grid.line(to: CGPoint(x: x, y: dirtyRect.maxY))
            wx += step
        }
        var wy = floor((dirtyRect.minY - offset.y) / scale / step) * step
        while wy * scale + offset.y <= dirtyRect.maxY {
            let y = (wy * scale + offset.y).rounded() + 0.5
            grid.move(to: CGPoint(x: dirtyRect.minX, y: y)); grid.line(to: CGPoint(x: dirtyRect.maxX, y: y))
            wy += step
        }
        grid.lineWidth = 1
        Theme.line.withAlphaComponent(0.22).setStroke()
        grid.stroke()

        drawHints()
    }

    /// Blasse Tastenhinweise oben rechts, untereinander.
    private func drawHints() {
        let lines = [("F1", "Hilfe"), ("⌘P", "Omnisuche"), ("⌘N", "neue Session"), ("⌘Esc", "zurück")]
        let key = Theme.attrs(11, Theme.muted.withAlphaComponent(0.7), bold: true)
        let txt = Theme.attrs(11, Theme.muted.withAlphaComponent(0.55))
        var y: CGFloat = 10
        for (k, t) in lines {
            let a = NSAttributedString(string: k, attributes: key), b = NSAttributedString(string: t, attributes: txt)
            b.draw(at: CGPoint(x: bounds.maxX - 14 - b.size().width, y: y))
            a.draw(at: CGPoint(x: bounds.maxX - 14 - b.size().width - 8 - a.size().width, y: y))
            y += 16
        }
    }
}
