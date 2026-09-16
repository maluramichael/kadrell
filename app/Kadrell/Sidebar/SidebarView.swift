import AppKit

/// Linke Seite: Baum Gruppe › Sessions, handgezeichnet wie die Leiste. Liegt in einem NSScrollView und
/// setzt seine Höhe selbst.
@MainActor
final class SidebarView: NSView {
    /// Aussehen der Zeilen, umschaltbar in den Einstellungen.
    var renderer: any SidebarRenderer = SidebarStyle.tinted.renderer
    /// Laufzeit („12m“) rechts in jeder Session-Zeile.
    var showAge = true
    /// Sortiert nur die Anzeige. Ziehen ist dann aus, weil die Handreihenfolge unsichtbar bliebe.
    var sort: SidebarSort = .off

    private(set) var groups: [Group] = []
    private(set) var sessions: [String: Session] = [:]
    var selected: Set<String> = []
    var focused: String?
    var attach: AttachManager?
    /// Eingeschaltet: jede Session-Zeile bekommt eine zweite Zeile mit `messages[id]`.
    var showMessages = false
    var messages: [String: String] = [:]
    /// Sessions mit Antworten seit dem letzten Fokus: Titel fett.
    var unread: Set<String> = []
    private var collapsed: Set<String> = []
    private var rows: [Row] = []
    private var hovered: Int?
    /// Knopf der schwebenden Toolbar unter der Maus, Index von links.
    private var hoveredButton: Int?
    /// Letzte einzeln angeklickte Session: Startpunkt für ⇧-Bereiche.
    private var anchor: String?
    private var pulseTask: Task<Void, Never>?
    /// Gedrückte Zeile: Klick wird erst beim Loslassen ausgewertet, Ziehen sortiert um. Zeilen als Wert,
    /// nicht als Index: ein Poll kann den Baum zwischen Drücken und Loslassen neu aufbauen.
    private var pressed: (point: CGPoint, row: Row, flags: NSEvent.ModifierFlags)?
    private var dragging = false
    private var dropTarget: Row?
    /// ⌘ gehalten: der „+“-Knopf einer Gruppe zeigt ein Terminal-Icon und öffnet ein Terminal ohne Claude.
    private var cmdDown = false
    private var flagsMonitor: Any?

    enum SelectMode { case replace, toggle, add, cursor }
    /// Klick = nur diese, ⌘-Klick = dazu oder weg, ⇧-Klick = Bereich seit dem letzten Klick dazu,
    /// ↑↓ (cursor) = nur diese, die Tastatur bleibt im Baum.
    var onSelect: (([String], SelectMode) -> Void)?
    var onNewSession: ((String) -> Void)?
    /// ⌘ über der Gruppen-Toolbar: der „+“-Knopf öffnet ein Terminal ohne Claude statt einer Claude-Session.
    var onNewTerminal: ((String) -> Void)?
    /// Ordner aus dem Finder in den Baum gezogen: neue Session dort, in dessen Gruppe oder einer neuen.
    var onDropFolder: ((String) -> Void)?
    var onEditGroup: ((String) -> Void)?
    /// Favorit an/aus: eine favorisierte Gruppe bleibt auch ohne Sessions in der Liste.
    var onToggleFavorite: ((String) -> Void)?
    /// Zweiter Parameter: ⌥ gehalten, dann ohne Rückfrage. Nicht ⌘: das bedeutet hier „zur Auswahl dazu“.
    var onCloseGroup: ((String, Bool) -> Void)?
    var onCloseSession: ((String, Bool) -> Void)?
    var onRenameSession: ((String) -> Void)?
    /// Ziehen: (gezogen, Ziel), Session innerhalb ihrer Gruppe bzw. Gruppe vor/hinter eine andere.
    var onMoveSession: ((String, String) -> Void)?
    var onMoveGroup: ((String, String) -> Void)?
    /// Rechtsklick auf eine Session-Zeile: liefert das Kontextmenü, oder nil (Gruppenzeile, daneben).
    var onContextMenu: ((String) -> NSMenu?)?

    private enum Row {
        case group(Group), session(Session, Group)
        var key: String { switch self { case .group(let g): "g:" + g.id; case .session(let s, _): "s:" + s.id } }
    }
    private func index(of r: Row) -> Int? { rows.firstIndex { $0.key == r.key } }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        registerForDraggedTypes([.fileURL])
        pulseTask = Task { [weak self] in
            var ticks = 0
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(80))
                guard let self else { continue }
                ticks += 1
                // Laufzeiten („12m“) einmal pro Minute nachziehen, sonst nur die pulsenden Punkte laufender Sessions.
                if ticks % 750 == 0 { self.needsDisplay = true; continue }
                for (i, row) in self.rows.enumerated() {
                    guard case .session(let s, _) = row, s.status == .running else { continue }
                    self.setNeedsDisplay(self.renderer.dotRect(self.rowRect(i)).insetBy(dx: -1, dy: -1).scaled(Theme.scale))
                }
            }
        }
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

    /// Im NSScrollView passt niemand die Breite des Dokuments an: hier selbst dem Clip-View folgen,
    /// sonst wandern Icons und Laufzeiten rechts aus dem sichtbaren Bereich.
    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        guard let clip = superview as? NSClipView else { return }
        clip.postsFrameChangedNotifications = true
        NotificationCenter.default.addObserver(forName: NSView.frameDidChangeNotification, object: clip, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.fitWidth() }
        }
        fitWidth()
    }

    private func fitWidth() {
        guard let clip = superview else { return }
        let w = clip.bounds.width
        if frame.width != w { setFrameSize(NSSize(width: w, height: max(frame.height, clip.bounds.height))); needsDisplay = true }
    }

    func reload(groups: [Group], sessions: [Session]) {
        self.groups = sort.apply(groups, sessions: Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) }))
        self.sessions = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        rows = []
        for g in self.groups {
            rows.append(.group(g))
            guard !collapsed.contains(g.id) else { continue }
            for s in g.sessionIds.compactMap({ self.sessions[$0] }) { rows.append(.session(s, g)) }
        }
        let h = rows.reduce(renderer.topInset + 6) { $0 + rowHeight($1) } + CGFloat(max(groups.count - 1, 0)) * renderer.groupGap
        let want = max((h * Theme.scale).rounded(.up), superview?.bounds.height ?? 0)
        if frame.height != want { setFrameSize(NSSize(width: frame.width, height: want)) }
        needsDisplay = true
    }

    /// Ist irgendeine Gruppe offen, gehen alle zu, sonst alle auf.
    func toggleAllGroups() {
        collapsed = groups.contains { !collapsed.contains($0.id) } ? Set(groups.map(\.id)) : []
        reload(groups: groups, sessions: Array(sessions.values))
    }

    private func rowHeight(_ r: Row) -> CGFloat {
        if case .group = r { renderer.groupRow } else { renderer.sessionRow + (showMessages ? 14 : 0) }
    }

    private func rowRect(_ i: Int) -> CGRect {
        var y = renderer.topInset
        for (j, r) in rows.enumerated() {
            if case .group = r, j > 0 { y += renderer.groupGap }
            let h = rowHeight(r)
            if j == i { return CGRect(x: 0, y: y, width: bounds.width / Theme.scale, height: h) }
            y += h
        }
        return .zero
    }

    private func rowIndex(at p: CGPoint) -> Int? { rows.indices.first { rowRect($0).contains(p) } }
    /// Sichtbare Sessions in Baumreihenfolge (Bereichsauswahl läuft über Gruppen hinweg).
    private var sessionIds: [String] { rows.compactMap { if case .session(let s, _) = $0 { s.id } else { nil } } }
    private func sessionIndex(_ id: String) -> Int? { sessionIds.firstIndex(of: id) }

    // MARK: Schwebende Toolbar

    /// Gruppe: Favorit, neue Session, bearbeiten, schließen. Session: umbenennen, schließen.
    private func buttonCount(_ row: Row) -> Int { if case .group = row { 4 } else { 2 } }

    /// Liegt über dem rechten Ende der Kopfzeile, verdeckt Laufzeit und Zähler statt sie zu verschieben.
    private func toolbarRect(_ i: Int) -> CGRect {
        let r = rowRect(i)
        let head = min(r.height, 24)
        let w = CGFloat(buttonCount(rows[i])) * 20 + 4
        return CGRect(x: r.maxX - 6 - w, y: r.minY + (head - 22) / 2, width: w, height: 22)
    }

    private func buttonRect(_ i: Int, _ k: Int) -> CGRect {
        let t = toolbarRect(i)
        return CGRect(x: t.minX + 2 + CGFloat(k) * 20, y: t.minY + 1, width: 20, height: 20)
    }

    private func button(at p: CGPoint, row i: Int) -> Int? {
        (0..<buttonCount(rows[i])).first { buttonRect(i, $0).contains(p) }
    }

    private func drawToolbar(_ i: Int) {
        let t = toolbarRect(i)
        let path = NSBezierPath(roundedRect: t.insetBy(dx: 0.5, dy: 0.5), xRadius: 5, yRadius: 5)
        NSGraphicsContext.saveGraphicsState()
        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.45)
        shadow.shadowBlurRadius = 8
        shadow.shadowOffset = NSSize(width: 0, height: -2)
        shadow.set()
        Theme.bg.setFill()
        path.fill()
        NSGraphicsContext.restoreGraphicsState()
        Theme.line.setStroke()
        path.lineWidth = 1
        path.stroke()
        let n = buttonCount(rows[i])
        for k in 0..<n {
            let b = buttonRect(i, k)
            let hot = hoveredButton == k
            if hot { Theme.line.setFill(); NSBezierPath(roundedRect: b, xRadius: 4, yRadius: 4).fill() }
            let close = k == n - 1
            let color = hot ? (close ? Theme.error : Theme.fg) : Theme.muted
            let ic = b.insetBy(dx: 2, dy: 2)
            switch (rows[i], k) {
            case (.group(let g), 0):
                Icons.heart(in: ic.insetBy(dx: 1, dy: 1), color: g.isFavorite ? Theme.group(g.color) : color, filled: g.isFavorite)
            case (.group, 1): cmdDown ? Icons.computer(in: ic, color: color) : Icons.plus(in: ic, color: color)
            case (.group, 2), (.session, 0): Icons.pen(in: ic, color: color)
            default: Icons.x(in: ic, color: color)
            }
        }
    }

    // MARK: Zeichnen

    /// Zeilen, Zeichnen und Trefferflächen in unskalierten Punkten, siehe `local`.
    override func draw(_ dirty: NSRect) {
        Theme.panel.setFill()
        dirty.fill()
        Theme.scaled(bounds) { _ in drawRows(dirty.scaled(1 / Theme.scale)) }
    }

    private func local(_ event: NSEvent) -> CGPoint {
        let p = convert(event.locationInWindow, from: nil)
        return CGPoint(x: p.x / Theme.scale, y: p.y / Theme.scale)
    }

    private func drawRows(_ dirtyRect: CGRect) {
        for (i, row) in rows.enumerated() {
            let r = rowRect(i)
            guard r.intersects(dirtyRect) else { continue }
            switch row {
            case .group(let g): drawGroup(g, in: r, first: i == 0, hover: hovered == i)
            case .session(let s, let g): drawSession(s, group: g, in: r, hover: hovered == i)
            }
        }
        if let i = hovered, !dragging, i < rows.count, toolbarRect(i).intersects(dirtyRect) { drawToolbar(i) }
        if let y = dropLineY() { Theme.fg.setFill(); CGRect(x: 0, y: y - 1, width: bounds.width / Theme.scale, height: 2).fill() }
    }

    /// Einfügelinie: über dem Ziel beim Ziehen nach oben, darunter (bei Gruppen unter deren letzter Zeile) nach unten.
    private func dropLineY() -> CGFloat? {
        guard dragging, let tr = dropTarget, let t = index(of: tr), let sr = pressed?.row, let src = index(of: sr) else { return nil }
        guard t > src else { return rowRect(t).minY }
        guard case .group = rows[t] else { return rowRect(t).maxY }
        let end = rows.indices.dropFirst(t + 1).first { if case .group = rows[$0] { true } else { false } } ?? rows.count
        return rowRect(end - 1).maxY
    }

    private func dotColor(_ s: Session) -> NSColor {
        let attached = attach?.isAttached(s.id) ?? false
        let c = attached ? Theme.color(for: s.status) : Theme.detached
        guard s.status == .running, attached else { return c }
        let t = CACurrentMediaTime().truncatingRemainder(dividingBy: 1.2) / 1.2
        return c.withAlphaComponent(0.3 + 0.7 * (0.5 + 0.5 * cos(2 * .pi * t)))
    }

    private func drawGroup(_ g: Group, in r: CGRect, first: Bool, hover: Bool) {
        let members = g.sessionIds.compactMap { sessions[$0] }
        let dots = members.map { attach?.isAttached($0.id) ?? false ? Theme.color(for: $0.status) : Theme.detached }
        let waiting = members.filter { $0.status == .waiting && (attach?.isAttached($0.id) ?? false) }.count
        renderer.drawGroup(SidebarGroupItem(group: g, color: Theme.group(g.color), dots: dots, open: !collapsed.contains(g.id),
                                            selected: g.sessionIds.contains { selected.contains($0) }, hover: hover, first: first,
                                            waitingCount: waiting), in: r)
    }

    private func drawSession(_ s: Session, group g: Group, in r: CGRect, hover: Bool) {
        renderer.drawSession(SidebarSessionItem(session: s, color: Theme.group(g.color), dot: dotColor(s),
                                                selected: selected.contains(s.id), focused: focused == s.id,
                                                keyFocus: window?.firstResponder === self, hover: hover,
                                                message: showMessages ? messages[s.id] : nil, showAge: showAge,
                                                unread: unread.contains(s.id)), in: r)
    }

    // MARK: Events

    /// Nur per ⌘1, nicht per Klick: sonst zeigt die alte Fokus-Zeile zwischen Drücken und Loslassen kurz den
    /// Tastatur-Rahmen, danach holt sich die Kachel die Tastatur ohnehin zurück (Flackern).
    override var acceptsFirstResponder: Bool { NSApp.currentEvent?.type != .leftMouseDown }
    override func becomeFirstResponder() -> Bool { needsDisplay = true; return true }
    override func resignFirstResponder() -> Bool { needsDisplay = true; return true }

    /// ⌘1 gibt dem Baum die Tastatur: ↑↓ wandert über die sichtbaren Sessions, ab der fokussierten.
    override func keyDown(with event: NSEvent) {
        let step: Int
        switch event.keyCode {
        case 125: step = 1
        case 126: step = -1
        default: super.keyDown(with: event); return
        }
        guard let id = sessionId(after: focused, step: step) else { return }
        anchor = id
        onSelect?([id], .cursor)
        reveal(id)
    }

    /// Nachbar in Baumreihenfolge über die sichtbaren Sessions, am Rand bleibt es stehen. Ohne Start die erste.
    func sessionId(after id: String?, step: Int) -> String? {
        let ids = sessionIds
        guard !ids.isEmpty else { return nil }
        return ids[id.flatMap { ids.firstIndex(of: $0) }.map { min(max($0 + step, 0), ids.count - 1) } ?? 0]
    }

    func reveal(_ id: String) {
        if let r = rows.firstIndex(where: { $0.key == "s:" + id }) { scrollToVisible(rowRect(r).scaled(Theme.scale)) }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for t in trackingAreas { removeTrackingArea(t) }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    // MARK: Ordner hineinziehen

    private func droppedFolder(_ info: NSDraggingInfo) -> String? {
        let urls = info.draggingPasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL]
        return urls?.lazy.compactMap { FolderIndex.folder(for: $0.path) }.first
    }

    override func draggingEntered(_ info: NSDraggingInfo) -> NSDragOperation { droppedFolder(info) == nil ? [] : .copy }

    override func performDragOperation(_ info: NSDraggingInfo) -> Bool {
        guard let dir = droppedFolder(info) else { return false }
        onDropFolder?(dir)
        return true
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            if let m = flagsMonitor { NSEvent.removeMonitor(m); flagsMonitor = nil }
        } else if flagsMonitor == nil {
            // ⌘ drücken/loslassen erreicht die Sidebar nicht als First Responder (das Terminal hat die Tastatur),
            // daher ein lokaler Monitor, damit der „+“-Knopf beim Hovern sofort das Icon wechselt.
            flagsMonitor = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged) { [weak self] e in
                self?.setCmdDown(e.modifierFlags.contains(.command)); return e
            }
        }
    }

    private func setCmdDown(_ down: Bool) {
        guard down != cmdDown else { return }
        cmdDown = down
        if let i = hovered, case .group = rows[i] { needsDisplay = true }
    }

    override func mouseMoved(with event: NSEvent) {
        guard convert(event.locationInWindow, from: nil).x < bounds.width - ThinSplitView.grabWidth / 2 else { return }   // Griffzone des Trenners
        setCmdDown(event.modifierFlags.contains(.command))
        let p = local(event)
        let i = rowIndex(at: p)
        (i == nil ? NSCursor.arrow : NSCursor.pointingHand).set()
        let k = i.flatMap { button(at: p, row: $0) }
        guard i != hovered || k != hoveredButton else { return }
        hovered = i
        hoveredButton = k
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        hovered = nil
        hoveredButton = nil
        needsDisplay = true
    }

    /// Rechtsklick (bzw. Ctrl-Klick): Kontextmenü der Session unter dem Zeiger, sonst keins.
    override func menu(for event: NSEvent) -> NSMenu? {
        guard let i = rowIndex(at: local(event)), case .session(let s, _) = rows[i] else { return nil }
        return onContextMenu?(s.id)
    }

    override func mouseDown(with event: NSEvent) {
        let p = local(event)
        pressed = nil
        guard let i = rowIndex(at: p) else { return }
        let force = event.modifierFlags.contains(.option)
        // Toolbar nur, wo sie sichtbar ist: ohne Hover (Fenster nicht aktiv) wählt der Klick die Zeile.
        if hovered == i, toolbarRect(i).contains(p) {
            switch (rows[i], button(at: p, row: i)) {
            case (.group(let g), 0): onToggleFavorite?(g.id)
            case (.group(let g), 1): event.modifierFlags.contains(.command) ? onNewTerminal?(g.id) : onNewSession?(g.id)
            case (.group(let g), 2): onEditGroup?(g.id)
            case (.group(let g), 3): onCloseGroup?(g.id, force)
            case (.session(let s, _), 0): onRenameSession?(s.id)
            case (.session(let s, _), 1): onCloseSession?(s.id, force)
            default: break
            }
            return
        }
        switch rows[i] {
        case .group(let g):
            if p.x < 26 {
                if collapsed.contains(g.id) { collapsed.remove(g.id) } else { collapsed.insert(g.id) }
                reload(groups: groups, sessions: Array(sessions.values))
                return
            }
        case .session: break
        }
        pressed = (p, rows[i], event.modifierFlags)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let press = pressed, sort == .off else { return }
        let p = local(event)
        if !dragging, hypot(p.x - press.point.x, p.y - press.point.y) > 4 { dragging = true; hovered = nil }
        guard dragging else { return }
        autoscroll(with: event)
        NSCursor.closedHand.set()
        dropTarget = target(for: press.row, at: p)
        needsDisplay = true
    }

    /// Gültiges Ziel: eine Session nur innerhalb ihrer Gruppe, eine Gruppe landet auf der Kopfzeile einer anderen.
    private func target(for src: Row, at p: CGPoint) -> Row? {
        guard let j = rowIndex(at: p), rows[j].key != src.key else { return nil }
        switch (src, rows[j]) {
        case (.session(_, let g), .session(_, let h)): return g.id == h.id ? rows[j] : nil
        case (.group(let g), _):
            guard let head = rows[...j].last(where: { if case .group = $0 { true } else { false } }),
                  case .group(let h) = head, h.id != g.id else { return nil }
            return head
        default: return nil
        }
    }

    override func mouseUp(with event: NSEvent) {
        guard let press = pressed else { return }
        let wasDragging = dragging, t = dropTarget
        pressed = nil; dragging = false; dropTarget = nil
        needsDisplay = true
        if wasDragging {
            NSCursor.arrow.set()
            guard let t else { return }
            switch (press.row, t) {
            case (.session(let s, _), .session(let u, _)): onMoveSession?(s.id, u.id)
            case (.group(let g), .group(let h)): onMoveGroup?(g.id, h.id)
            default: break
            }
            return
        }
        let shift = press.flags.contains(.shift), cmd = press.flags.contains(.command)
        switch press.row {
        case .group(let g):
            onSelect?(g.sessionIds, shift || cmd ? .add : .replace)
        case .session(let s, _):
            if shift, let a = anchor, let ai = sessionIndex(a), let bi = sessionIndex(s.id) {
                onSelect?(sessionIds[min(ai, bi)...max(ai, bi)].map { $0 }, .add)
                return
            }
            anchor = s.id
            onSelect?([s.id], cmd ? .toggle : .replace)
        }
    }
}
