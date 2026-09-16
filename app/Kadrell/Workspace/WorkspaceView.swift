import AppKit

/// Rechte Seite: die ausgewählten Sessions als Grid oder Stack (i3-Akkordeon). Hält Auswahl, Fokus und
/// Layout-Modus, hängt Terminals in die sichtbaren Kacheln ein. Stack-Zeilen zeichnet sie selbst.
@MainActor
final class WorkspaceView: NSView {
    private(set) var groups: [Group] = []
    private(set) var sessions: [String: Session] = [:]
    /// Geordnete Session-Ids, die rechts zu sehen sind.
    private(set) var selected: [String] = []
    private(set) var focused: String? {
        didSet { if oldValue != focused, let o = oldValue { lastFocused = o } }
    }
    /// Für „zuletzt fokussierte Kachel“ (tmux M-Tab).
    private var lastFocused: String?
    private(set) var mode: LayoutMode = LayoutMode(rawValue: UserDefaults.standard.string(forKey: "workspace.mode") ?? "") ?? .grid
    /// Zoom: nur die Fokus-Kachel, bildschirmfüllend, die Auswahl bleibt.
    private(set) var zen = false
    /// ⌥J/⌥K: eine Session aus dem Baum vorübergehend allein zeigen. Auswahl und Fokus bleiben unangetastet.
    private(set) var preview: String?
    var attach: AttachManager?
    /// Auto-Modus: aus der Auswahl nur wartende bzw. arbeitende Sessions zeigen (Einstellungen).
    private(set) var auto = UserDefaults.standard.bool(forKey: "workspace.auto")
    /// Passt eine Session nicht mehr, bleibt ihre Kachel bis zu diesem Zeitpunkt stehen.
    private var linger: [String: CFTimeInterval] = [:]
    private var lastMatching: Set<String> = []
    private var lastTiles: [String] = []

    private var cells: [String: CellView] = [:]
    private var stackRows: [(CGRect, String)] = []
    private var hoveredCell: String?
    private var hoveredRow: String?
    private var pulse: CGFloat = 1
    private var pulseTask: Task<Void, Never>?
    private weak var lastFirstResponder: NSResponder?
    /// Ziehen einer Kachel oder Stack-Zeile auf eine andere sortiert um.
    private var pressed: (point: CGPoint, key: String)?
    private var dragging = false
    private var dropTarget: String?

    var onChange: (() -> Void)?
    /// Zweiter Parameter: ⌘ gehalten, dann ohne Rückfrage.
    var onCloseSession: ((String, Bool) -> Void)?
    var onRenameSession: ((String) -> Void)?
    /// Ziehen: (gezogen, Ziel). Die Reihenfolge selbst gehört dem Baum, siehe `sortSelected`.
    var onMoveSession: ((String, String) -> Void)?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        selected = UserDefaults.standard.stringArray(forKey: "workspace.selected") ?? []
        focused = selected.first
        pulseTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(80))
                self?.tick()
            }
        }
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    // MARK: Daten

    func reload(groups: [Group], sessions: [Session]) {
        self.groups = groups
        self.sessions = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        selected.removeAll { self.sessions[$0] == nil }
        if let p = preview, self.sessions[p] == nil { preview = nil }
        sortSelected()
        if let f = focused, !selected.contains(f) { focused = selected.first }
        for (k, v) in cells where self.sessions[k] == nil { v.removeFromSuperview(); cells[k] = nil }
        for id in selected {
            let v = cells[id] ?? CellView(session: self.sessions[id]!)
            v.session = self.sessions[id]!
            if v.superview == nil { addSubview(v) }
            cells[id] = v
        }
        relayout()
    }

    func session(_ key: String) -> Session? { sessions[key] }
    func group(forSession key: String) -> Group? { groups.first { $0.sessionIds.contains(key) } }

    /// Sichtbare Sessions starten ihren Claude-Prozess; einmal gestartet läuft er weiter, auch ausgeblendet.
    func shownSessions() -> [Session] { selected.compactMap { sessions[$0] } }

    // MARK: Auswahl

    /// Ersetzt die Auswahl (Klick) oder toggelt jede Id (⌘-Klick). `takeKeyboard: false`: Pfeiltasten im Baum.
    func select(_ ids: [String], add: Bool, takeKeyboard: Bool = true) {
        let ids = ids.filter { sessions[$0] != nil }
        preview = nil
        if add {
            for id in ids {
                if let i = selected.firstIndex(of: id) {
                    selected.remove(at: i)
                    if focused == id { focused = selected.last }
                } else {
                    selected.append(id)
                    focused = id
                }
            }
        } else {
            selected = ids
            focused = ids.first
        }
        zen = false
        sortSelected()
        for (k, v) in cells where !selected.contains(k) { v.removeFromSuperview(); cells[k] = nil }
        for id in selected where cells[id] == nil {
            let v = CellView(session: sessions[id]!)
            addSubview(v)
            cells[id] = v
        }
        // Nur die angeklickten starten sofort, auch beendete; andere beendete bleiben stehen.
        for id in ids where selected.contains(id) { attach?.attachNow(sessions[id]!) }
        persist()
        relayout()
        if takeKeyboard { focusTerminal() }
    }

    /// ⇧-Bereich oder Gruppe mit Modifier: fehlende Sessions dazu, nichts weg.
    func addMissing(_ ids: [String]) { select(ids.filter { !selected.contains($0) }, add: true) }

    func setFocus(_ key: String, takeKeyboard: Bool = true) {
        guard selected.contains(key) else { return }
        focused = key
        relayout()
        if takeKeyboard { focusTerminal() }
    }

    /// ⌘Esc: Fokus-Kachel aus der Auswahl nehmen (im Zen erst zurück ins Layout).
    func removeFocused() {
        if zen { zen = false; relayout(); return }
        guard let f = focused else { return }
        select([f], add: true)
    }

    func moveFocus(_ d: Tiling.Direction) {
        let tiles = tiles
        guard let f = focused, let i = tiles.firstIndex(of: f),
              let j = Tiling.neighbor(of: i, count: tiles.count, mode: zen ? .stack : mode, d) else { return }
        setFocus(tiles[j])
    }

    /// Fokus-Kachel mit ihrem Nachbarn in Richtung `d` tauschen, wie Ziehen: die Reihenfolge gehört dem Baum.
    func swapFocused(_ d: Tiling.Direction) {
        let tiles = tiles
        guard let f = focused, let i = tiles.firstIndex(of: f),
              let j = Tiling.neighbor(of: i, count: tiles.count, mode: mode, d) else { return }
        onMoveSession?(f, tiles[j])
    }

    /// Nächste (+1) oder vorige (-1) Kachel, am Ende wieder vorn.
    func cycleFocus(_ step: Int) {
        let tiles = tiles
        guard let f = focused, let i = tiles.firstIndex(of: f) else { return }
        let n = tiles.count
        setFocus(tiles[((i + step) % n + n) % n])
    }

    /// Vorschau ohne Tastatur und ohne Prozessstart: eine beendete Session zeigt ihre letzten Zeilen.
    func setPreview(_ key: String?) {
        let old = preview
        preview = key.flatMap { sessions[$0] == nil ? nil : $0 }
        if let o = old, o != preview, !selected.contains(o) { cells[o]?.removeFromSuperview(); cells[o] = nil }
        if let p = preview, cells[p] == nil {
            let v = CellView(session: sessions[p]!)
            addSubview(v)
            cells[p] = v
        }
        relayout()
    }

    func focusLast() { if let l = lastFocused { setFocus(l) } }

    func focusTile(_ i: Int) { let t = tiles; if t.indices.contains(i) { setFocus(t[i]) } }

    func toggleAuto() {
        auto.toggle()
        UserDefaults.standard.set(auto, forKey: "workspace.auto")
        linger = [:]
        lastMatching = []
        zen = false
        relayout()
        focusTerminal()
    }

    /// Sync (tmux synchronize-panes): Eingaben im Fokus-Terminal gehen an alle Kacheln. Bewusst nicht gespeichert.
    private(set) var sync = false

    func toggleSync() {
        sync.toggle()
        relayout()
        focusTerminal()
    }

    /// Terminals der übrigen Kacheln, an die im Sync-Modus Eingaben mitgehen.
    func syncTargets(except source: KadrellTerminalView) -> [KadrellTerminalView] {
        guard sync else { return [] }
        return tiles.compactMap { attach?.terminal(for: $0) }.filter { $0 !== source }
    }

    /// Kacheln rechts: die Auswahl, im Auto-Modus gefiltert samt Nachlauf.
    private var tiles: [String] { auto ? selected.filter { matchesAuto($0) || linger[$0] != nil } : selected }

    private func matchesAuto(_ key: String) -> Bool {
        switch sessions[key]?.status {
        case .waiting: Settings.autoWaiting
        case .running: Settings.autoRunning
        default: false
        }
    }

    /// Wer eben noch passte und jetzt nicht mehr, läuft 3 s nach.
    private func updateLinger() {
        guard auto else { return }
        let now = CACurrentMediaTime()
        let matching = Set(selected.filter(matchesAuto))
        for id in lastMatching.subtracting(matching) where linger[id] == nil { linger[id] = now + 3 }
        linger = linger.filter { $0.value > now && !matching.contains($0.key) && selected.contains($0.key) }
        lastMatching = matching
    }

    func setMode(_ m: LayoutMode) {
        mode = m
        UserDefaults.standard.set(m.rawValue, forKey: "workspace.mode")
        relayout()
        focusTerminal()
    }

    func toggleZen() {
        guard focused != nil else { return }
        zen.toggle()
        relayout()
        focusTerminal()
    }

    /// Arbeitsfläche zeigt die Sessions in Baumreihenfolge: Gruppen, darin ihre Sessions.
    private func sortSelected() {
        let order = Dictionary(groups.flatMap(\.sessionIds).enumerated().map { ($1, $0) }, uniquingKeysWith: { a, _ in a })
        selected = selected.enumerated().sorted { (order[$0.1] ?? .max, $0.0) < (order[$1.1] ?? .max, $1.0) }.map(\.1)
    }

    private func persist() { UserDefaults.standard.set(selected, forKey: "workspace.selected") }

    private func focusTerminal() {
        guard let f = focused, let t = attach?.terminal(for: f), t.superview != nil else { window?.makeFirstResponder(self); return }
        window?.makeFirstResponder(t)
    }

    // MARK: Layout

    func relayout() {
        let inset = bounds.insetBy(dx: 6, dy: 6)
        var frames: [String: CGRect] = [:]
        stackRows = []
        updateLinger()
        let tiles = tiles
        // Auto-Modus: ist die Fokus-Kachel weg, bekommt eine neu aufgetauchte (sonst die erste) den Fokus.
        var refocus = false
        if auto, preview == nil, let first = tiles.first, !(focused.map(tiles.contains) ?? false) {
            focused = tiles.first { !lastTiles.contains($0) } ?? first
            refocus = true
        }
        lastTiles = tiles
        let visible: [String]
        if let p = preview {
            visible = [p]
            frames[p] = inset
        } else if zen, let f = focused, tiles.contains(f) {
            visible = [f]
            frames[f] = inset
        } else if mode == .grid {
            visible = tiles
            for (id, r) in zip(tiles, Tiling.grid(count: tiles.count, in: inset)) { frames[id] = r }
        } else {
            let active = focused.flatMap { tiles.firstIndex(of: $0) } ?? 0
            let (rows, body) = Tiling.stack(count: tiles.count, active: active, in: inset, rowHeight: (Tiling.rowHeight * Theme.scale).rounded())
            stackRows = Array(zip(rows, tiles))
            visible = tiles.isEmpty ? [] : [tiles[active]]
            if !tiles.isEmpty { frames[tiles[active]] = body }
        }
        for (key, v) in cells {
            guard let f = frames[key], let s = sessions[key] else {
                v.isHidden = true
                unmountTerminal(for: key)
                continue
            }
            v.isHidden = false
            v.headerHidden = mode == .stack && !zen && preview == nil
            if v.frame != f { v.frame = f }
            let g = group(forSession: key)
            v.groupName = g?.name ?? ""
            v.groupColor = Theme.group(g?.color ?? "#6c7086")
            // Sync: alle Kacheln bekommen Eingaben, also sehen auch alle ausgewählt aus.
            v.focused = (focused == key || sync) && visible.count > 1
            v.zoomed = zen && tiles.count > 1
            v.dropTarget = dragging && dropTarget == key
            v.hovered = hoveredCell == key
            v.attached = attach?.isAttached(key) ?? false
            v.ended = attach?.isEnded(key) ?? false
            v.previewing = preview == key
            v.keyboardFocus = attach?.terminal(for: key).map { $0 === window?.firstResponder } ?? false
            v.lines = attach?.lines(for: key) ?? []
            v.pulse = pulse
            mountTerminal(for: key, in: v, session: s)
            v.needsDisplay = true
        }
        // Terminals nicht sichtbarer Sessions dürfen nirgends hängen.
        for (key, t) in attach?.terminals ?? [:] where frames[key] == nil && t.superview != nil { unmountTerminal(for: key) }
        if let w = window, w.firstResponder === w { w.makeFirstResponder(self) }
        needsDisplay = true
        if refocus { focusTerminal() }
        onChange?()
    }

    private func mountTerminal(for key: String, in cell: CellView, session: Session) {
        guard let t = attach?.terminal(for: key) else { return }
        if t.superview !== cell { cell.addSubview(t) }
        let bg = cell.bodyColor
        if t.nativeBackgroundColor != bg { t.nativeBackgroundColor = bg }
        let body = cell.terminalRect
        if t.frame != body { t.frame = body }
        // Erst im Fenster umschalten, so will es SwiftTerm. Scheitert Metal, bleibt CoreGraphics.
        if t.window != nil, t.isUsingMetalRenderer != Settings.terminalMetal { try? t.setUseMetal(Settings.terminalMetal) }
    }

    private func unmountTerminal(for key: String) {
        guard let t = attach?.terminal(for: key), t.superview != nil else { return }
        if window?.firstResponder === t { window?.makeFirstResponder(self) }
        t.removeFromSuperview()
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        relayout()
    }

    private func tick() {
        // Klick in ein Terminal macht es still zum First Responder: Fokus und Rahmen nachziehen.
        if window?.firstResponder !== lastFirstResponder {
            lastFirstResponder = window?.firstResponder
            if let hit = cells.first(where: { attach?.terminal(for: $0.key) === window?.firstResponder }), focused != hit.key {
                focused = hit.key
                relayout()
            } else {
                for (key, v) in cells {
                    let has = attach?.terminal(for: key).map { $0 === window?.firstResponder } ?? false
                    if v.keyboardFocus != has { v.keyboardFocus = has; v.needsDisplay = true }
                }
            }
        }
        let t = CACurrentMediaTime().truncatingRemainder(dividingBy: 1.2) / 1.2
        pulse = 0.3 + 0.7 * (0.5 + 0.5 * cos(2 * .pi * t))
        for (key, v) in cells where sessions[key]?.status == .running && !v.isHidden {
            v.pulse = pulse
            if !v.headerHidden { v.setNeedsDisplay(v.dotRect) }
        }
        if !stackRows.isEmpty { needsDisplay = true }
        let now = CACurrentMediaTime()
        if linger.values.contains(where: { $0 <= now }) { relayout() }
    }

    // MARK: Zeichnen

    override func draw(_ dirtyRect: NSRect) {
        Theme.bg.setFill()
        dirtyRect.fill()
        if tiles.isEmpty {
            let idle = auto && !selected.isEmpty
            let a = NSAttributedString(string: idle ? "Gerade wartet keine Session" : "Session im Baum wählen", attributes: Theme.attrs(12, Theme.muted))
            let b = NSAttributedString(string: idle ? "Auto-Modus: Kacheln erscheinen, sobald Claude etwas von dir will" : "⌘-Klick für mehrere · ⇧-Klick Bereich · Gruppe = alle · F1 Hilfe", attributes: Theme.attrs(11, Theme.muted.withAlphaComponent(0.7)))
            Theme.scaled(bounds) { r in
                a.draw(at: CGPoint(x: r.midX - a.size().width / 2, y: r.midY - 16))
                b.draw(at: CGPoint(x: r.midX - b.size().width / 2, y: r.midY + 4))
            }
            return
        }
        guard !stackRows.isEmpty, !zen, preview == nil else { return }
        for (r, key) in stackRows { Theme.scaled(r) { drawStackRow($0, key: key) } }
        if dragging, let t = dropTarget, let (r, _) = stackRows.first(where: { $0.1 == t }) {
            Theme.fg.setFill()
            r.frame(withWidth: 2)
        }
    }

    private func drawStackRow(_ r: CGRect, key: String) {
        guard let s = sessions[key] else { return }
        let g = group(forSession: key)
        let color = Theme.group(g?.color ?? "#6c7086")
        let on = focused == key || sync, hover = hoveredRow == key
        color.mixed(on ? 0.22 : 0.1, into: on ? Theme.surface : Theme.panel).setFill()
        r.fill()
        Theme.line.setFill()
        CGRect(x: r.minX, y: r.maxY - 1, width: r.width, height: 1).fill()
        (on ? color : color.mixed(0.45, into: Theme.bg)).setFill()
        CGRect(x: r.minX, y: r.minY, width: on ? 3 : 1, height: r.height).fill()
        let attached = attach?.isAttached(key) ?? false
        let c = attached ? Theme.color(for: s.status) : Theme.detached
        (s.status == .running && attached ? c.withAlphaComponent(pulse) : c).setFill()
        NSBezierPath(ovalIn: CGRect(x: r.minX + 12, y: r.midY - 4, width: 8, height: 8)).fill()
        let age = NSAttributedString(string: s.elapsed(), attributes: Theme.attrs(10.5, Theme.muted))
        let grp = NSAttributedString(string: g?.name ?? "", attributes: Theme.attrs(10.5, color))
        var rx = r.maxX - 10
        if hover {
            Icons.x(in: CGRect(x: rx - 16, y: r.midY - 8, width: 16, height: 16), color: Theme.sub)
            Icons.pen(in: CGRect(x: rx - 36, y: r.midY - 8, width: 16, height: 16), color: Theme.sub)
            rx -= 44
        }
        rx -= age.size().width; age.draw(at: CGPoint(x: rx, y: r.midY - 7))
        rx -= 8 + grp.size().width; grp.draw(at: CGPoint(x: rx, y: r.midY - 7))
        if Settings.stackShowPath {
            let pa = Theme.attrs(10.5, Theme.muted)
            let maxW = max(0, (rx - 16 - r.minX - 28) / 2)
            let path = NSAttributedString(string: Theme.fitPath(s.cwd, width: maxW, attrs: pa), attributes: pa)
            let w = min(path.size().width, maxW)
            rx -= 16 + w
            path.draw(with: CGRect(x: rx, y: r.midY - 7, width: w, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        }
        let title = NSAttributedString(string: s.title, attributes: Theme.attrs(11.5, on || hover ? Theme.fg : Theme.sub, bold: on))
        title.draw(with: CGRect(x: r.minX + 28, y: r.midY - 8, width: max(0, rx - 10 - r.minX - 28), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    // MARK: Events

    private enum Hit { case cell(String), cellClose(String), cellRename(String), row(String), rowClose(String), rowRename(String), none }

    private func hit(at p: CGPoint) -> Hit {
        for (r, key) in stackRows where r.contains(p) {
            let fromRight = (r.maxX - p.x) / Theme.scale
            return fromRight < 30 ? .rowClose(key) : fromRight < 50 ? .rowRename(key) : .row(key)
        }
        for (key, v) in cells where !v.isHidden && v.frame.contains(p) {
            let local = CGPoint(x: p.x - v.frame.minX, y: p.y - v.frame.minY)
            if !v.headerHidden, v.xRect.insetBy(dx: -4, dy: -4).contains(local) { return .cellClose(key) }
            if !v.headerHidden, v.penRect.insetBy(dx: -2, dy: -4).contains(local) { return .cellRename(key) }
            return .cell(key)
        }
        return .none
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for t in trackingAreas { removeTrackingArea(t) }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    override func mouseMoved(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        guard p.x > ThinSplitView.grabWidth / 2 else { return }   // Griffzone des Trenners: Cursor gehört dem Split
        var cell: String?, row: String?
        switch hit(at: p) {
        case .cell(let k): cell = k; NSCursor.arrow.set()
        case .cellClose(let k), .cellRename(let k): cell = k; NSCursor.pointingHand.set()
        case .row(let k): row = k; NSCursor.pointingHand.set()
        case .rowClose(let k), .rowRename(let k): row = k; NSCursor.pointingHand.set()
        case .none: NSCursor.arrow.set()
        }
        guard cell != hoveredCell || row != hoveredRow else { return }
        let old = hoveredCell
        hoveredCell = cell; hoveredRow = row
        for k in [old, cell].compactMap({ $0 }) { cells[k]?.hovered = hoveredCell == k; cells[k]?.needsDisplay = true }
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        if let c = hoveredCell { cells[c]?.hovered = false; cells[c]?.needsDisplay = true }
        hoveredCell = nil; hoveredRow = nil
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        let force = event.modifierFlags.contains(.command)
        pressed = nil
        switch hit(at: p) {
        case .cellClose(let k), .rowClose(let k): onCloseSession?(k, force)
        case .cellRename(let k), .rowRename(let k): onRenameSession?(k)
        case .cell(let k), .row(let k):
            pressed = (p, k)
            // Beendete Session: Klick setzt sie fort.
            if attach?.isAttached(k) == false, let s = sessions[k] { attach?.attachNow(s) }
            setFocus(k)
        case .none: window?.makeFirstResponder(self)
        }
    }

    override func mouseDragged(with event: NSEvent) {
        guard let press = pressed, tiles.count > 1, !zen else { return }
        let p = convert(event.locationInWindow, from: nil)
        if !dragging, hypot(p.x - press.point.x, p.y - press.point.y) > 4 { dragging = true }
        guard dragging else { return }
        NSCursor.closedHand.set()
        var t: String?
        switch hit(at: p) {
        case .cell(let k), .cellClose(let k), .cellRename(let k), .row(let k), .rowClose(let k), .rowRename(let k): t = k == press.key ? nil : k
        case .none: t = nil
        }
        guard t != dropTarget else { return }
        dropTarget = t
        relayout()
    }

    override func mouseUp(with event: NSEvent) {
        let src = pressed?.key, t = dropTarget, wasDragging = dragging
        pressed = nil; dragging = false; dropTarget = nil
        guard wasDragging else { return }
        NSCursor.arrow.set()
        relayout()
        if let src, let t { onMoveSession?(src, t) }
    }
}
