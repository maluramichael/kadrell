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
    // Nicht Teil von `Row`: `invalidateChangedRows` sieht diese Änderungen nicht, deshalb selbst neu zeichnen.
    var selected: Set<String> = [] { didSet { if selected != oldValue { needsDisplay = true } } }
    var focused: String? { didSet { if focused != oldValue { needsDisplay = true } } }
    var attach: AttachManager?
    /// Eingeschaltet: jede Session-Zeile bekommt eine zweite Zeile mit `messages[id]`.
    var showMessages = false
    var messages: [String: String] = [:] { didSet { if messages != oldValue { needsDisplay = true } } }
    /// Sessions, die fertig geworden sind oder warten, ohne dass du hingesehen hast: Titel hervorgehoben, Marke „neu“.
    var unread: Set<String> = [] { didSet { if unread != oldValue { needsDisplay = true } } }
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
    /// Statuswechsel: Punkt blitzt auf (fertig) bzw. pulsiert zweimal (wartet).
    private var flashes: [String: (start: CFTimeInterval, waiting: Bool)] = [:]
    /// Neu aufgetauchte Sessions gleiten von links ein. nil bis zum ersten Laden: beim Start gleitet nichts.
    private var appeared: [String: CFTimeInterval] = [:]
    private var knownIds: Set<String>?
    private static let flashDuration: CFTimeInterval = 0.8, appearDuration: CFTimeInterval = 0.25
    private var flagsMonitor: Any?

    enum SelectMode { case replace, toggle, add, cursor }
    /// Klick = nur diese (schon ausgewählt: weg), ⌘-Klick = dazu oder weg, ⇧-Klick = Bereich seit dem letzten Klick dazu,
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

    private enum Row: Equatable {
        case group(Group), session(Session, Group)
        /// Identität der Zeile, unabhängig vom Inhalt; zugleich Schlüssel des Accessibility-Elements.
        var key: String { switch self { case .group(let g): "g:" + g.id; case .session(let s, _): Row.key(session: s.id) } }
        static func key(session id: String) -> String { "s:" + id }
    }
    private func index(of r: Row) -> Int? { rows.firstIndex { $0.key == r.key } }
    private func sessionRow(_ id: String) -> Int? { rows.firstIndex { $0.key == Row.key(session: id) } }
    private func isAttached(_ id: String) -> Bool { attach?.isAttached(id) ?? false }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        registerForDraggedTypes([.fileURL])
        pulseTask = Task { [weak self] in
            var lastFull = CACurrentMediaTime(), wasAnimating = false
            while !Task.isCancelled {
                // Solange etwas aufblitzt oder einfährt, flüssig zeichnen, sonst reicht der langsame Takt.
                try? await Task.sleep(for: .milliseconds(wasAnimating ? 16 : 80))
                guard let self else { return }
                // Fenster verdeckt/versteckt: nichts zu zeichnen, kein Puls nötig, wieder der langsame Takt.
                guard self.window?.occlusionState.contains(.visible) == true else { wasAnimating = false; continue }
                let now = CACurrentMediaTime(), animating = self.pruneAnimations(now)
                if animating || wasAnimating { wasAnimating = animating; self.needsDisplay = true; continue }
                // Laufzeiten („12m“) einmal pro Minute nachziehen, sonst nur die pulsenden Punkte laufender Sessions.
                if now - lastFull >= 60 { lastFull = now; self.needsDisplay = true; continue }
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
        let ids = Set(sessions.map(\.id)), now = CACurrentMediaTime()
        if let known = knownIds { for id in ids.subtracting(known) { appeared[id] = now } }
        knownIds = ids
        // uniquingKeysWith wie in WorkspaceView: eine doppelte Id darf nicht abstürzen.
        self.sessions = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        self.groups = sort.apply(groups, sessions: self.sessions)
        let oldRows = rows, previousHeight = frame.height
        rows = []
        for g in self.groups {
            rows.append(.group(g))
            guard !collapsed.contains(g.id) else { continue }
            for s in g.sessionIds.compactMap({ self.sessions[$0] }) { rows.append(.session(s, g)) }
        }
        rebuildRowOffsets()
        let h = rows.reduce(renderer.topInset + 6) { $0 + rowHeight($1) } + CGFloat(max(groups.count - 1, 0)) * renderer.groupGap
        let want = max((h * Theme.scale).rounded(.up), superview?.bounds.height ?? 0)
        if frame.height != want { setFrameSize(NSSize(width: frame.width, height: want)) }
        // Höhe unverändert: nur die Zeilen neu zeichnen, deren Inhalt sich geändert hat, statt die ganze Sidebar.
        if want != previousHeight { needsDisplay = true } else { invalidateChangedRows(old: oldRows) }
    }

    /// Zeilenhöhen sind O(1) abrufbar statt bei jedem Aufruf neu aufsummiert (Puls, Zeichnen, Maus laufen oft pro Sekunde
    /// über alle Zeilen). Nur die Breite bleibt live: sie folgt `bounds`, auch zwischen zwei `reload`s (siehe `fitWidth`).
    private var rowOffsets: [(y: CGFloat, height: CGFloat)] = []

    private func rebuildRowOffsets() {
        var y = renderer.topInset
        rowOffsets = rows.enumerated().map { j, r in
            if case .group = r, j > 0 { y += renderer.groupGap }
            let h = rowHeight(r)
            defer { y += h }
            return (y, h)
        }
    }

    private func invalidateChangedRows(old: [Row]) {
        guard old.count == rows.count else { needsDisplay = true; return }
        for (i, row) in rows.enumerated() where row != old[i] { setNeedsDisplay(rowRect(i).scaled(Theme.scale)) }
    }

    func flash(waiting: Set<String>, done: Set<String>) {
        let now = CACurrentMediaTime()
        for id in done { flashes[id] = (now, false) }
        for id in waiting { flashes[id] = (now, true) }
    }

    /// Abgelaufene Animationen entfernen; true, solange noch eine läuft.
    private func pruneAnimations(_ now: CFTimeInterval) -> Bool {
        flashes = flashes.filter { Feedback.progress(since: $0.value.start, duration: Self.flashDuration, now: now) != nil }
        appeared = appeared.filter { Feedback.progress(since: $0.value, duration: Self.appearDuration, now: now) != nil }
        return !flashes.isEmpty || !appeared.isEmpty
    }

    private func toggleCollapsed(_ id: String) {
        if collapsed.contains(id) { collapsed.remove(id) } else { collapsed.insert(id) }
        reload(groups: groups, sessions: Array(sessions.values))
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
        guard rowOffsets.indices.contains(i) else { return .zero }
        let (y, h) = rowOffsets[i]
        return CGRect(x: 0, y: y, width: bounds.width / Theme.scale, height: h)
    }

    private func rowIndex(at p: CGPoint) -> Int? { rows.indices.first { rowRect($0).contains(p) } }
    /// Sichtbare Sessions in Baumreihenfolge (Bereichsauswahl läuft über Gruppen hinweg).
    private var sessionIds: [String] { rows.compactMap { if case .session(let s, _) = $0 { s.id } else { nil } } }
    private func sessionIndex(_ id: String) -> Int? { sessionIds.firstIndex(of: id) }

    // MARK: Schwebende Toolbar

    private typealias ToolButton = (hit: HitRegion, icon: @MainActor (CGRect, NSColor) -> Void)

    /// Gruppe: Favorit, neue Session (⌘: Terminal), bearbeiten, schließen. Session: umbenennen, schließen. Zeichnen,
    /// Klick und VoiceOver-Aktionen lesen dieselbe Liste; `flags` vom Klick: ⌘ öffnet ein Terminal, ⌥ schließt ohne Rückfrage.
    /// Die Leiste liegt über dem rechten Ende der Kopfzeile, verdeckt Laufzeit und Zähler statt sie zu verschieben.
    private func toolbar(_ i: Int, flags: NSEvent.ModifierFlags = []) -> (rect: CGRect, buttons: [ToolButton]) {
        let force = flags.contains(.option)
        let items: [(label: String, icon: @MainActor (CGRect, NSColor) -> Void, action: @MainActor () -> Void)]
        switch rows[i] {
        case .group(let g):
            let cmd = cmdDown
            items = [
                (g.isFavorite ? String(localized: "Kein Favorit") : String(localized: "Favorit"),
                 { Icons.heart(in: $0.insetBy(dx: 1, dy: 1), color: g.isFavorite ? Theme.group(g.color) : $1, filled: g.isFavorite) },
                 { [weak self] in self?.onToggleFavorite?(g.id) }),
                (String(localized: "Neue Session"),
                 { g.host != nil ? Icons.server(in: $0, color: $1) : cmd ? Icons.computer(in: $0, color: $1) : Icons.plus(in: $0, color: $1) },
                 { [weak self] in flags.contains(.command) ? self?.onNewTerminal?(g.id) : self?.onNewSession?(g.id) }),
                (String(localized: "Bearbeiten"), { Icons.pen(in: $0, color: $1) }, { [weak self] in self?.onEditGroup?(g.id) }),
                (String(localized: "Schließen"), { Icons.x(in: $0, color: $1) }, { [weak self] in self?.onCloseGroup?(g.id, force) }),
            ]
        case .session(let s, _):
            items = [(String(localized: "Umbenennen"), { Icons.pen(in: $0, color: $1) }, { [weak self] in self?.onRenameSession?(s.id) }),
                     (String(localized: "Schließen"), { Icons.x(in: $0, color: $1) }, { [weak self] in self?.onCloseSession?(s.id, force) })]
        }
        let r = rowRect(i), head = min(r.height, 24), w = CGFloat(items.count) * 20 + 4
        let t = CGRect(x: r.maxX - 6 - w, y: r.minY + (head - 22) / 2, width: w, height: 22)
        return (t, items.enumerated().map { k, b in
            (HitRegion(rect: CGRect(x: t.minX + 2 + CGFloat(k) * 20, y: t.minY + 1, width: 20, height: 20), label: b.label, action: b.action), b.icon)
        })
    }

    private func drawToolbar(_ t: CGRect, _ buttons: [ToolButton]) {
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
        for (k, b) in buttons.enumerated() {
            let hot = hoveredButton == k
            if hot { Theme.line.setFill(); NSBezierPath(roundedRect: b.hit.rect, xRadius: 4, yRadius: 4).fill() }
            let color = hot ? (k == buttons.count - 1 ? Theme.error : Theme.fg) : Theme.muted
            b.icon(b.hit.rect.insetBy(dx: 2, dy: 2), color)
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
        if let i = hovered, !dragging, i < rows.count, case let tb = toolbar(i), tb.rect.intersects(dirtyRect) { drawToolbar(tb.rect, tb.buttons) }
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
        let attached = isAttached(s.id)
        let c = Theme.statusColor(s.status, attached: attached)
        if let f = flashes[s.id], let p = Feedback.progress(since: f.start, duration: Self.flashDuration) {
            let amount = f.waiting ? abs(sin(p * 2 * .pi)) : 1 - p
            return NSColor.white.mixed(0.75 * amount, into: c)
        }
        guard s.status == .running, attached else { return c }
        return c.withAlphaComponent(Feedback.pulse())
    }

    private func drawGroup(_ g: Group, in r: CGRect, first: Bool, hover: Bool) {
        let members = g.sessionIds.compactMap { sessions[$0] }
        let dots = members.map { Theme.statusColor($0.status, attached: isAttached($0.id)) }
        let waiting = members.filter { $0.status == .waiting && isAttached($0.id) }.count
        renderer.drawGroup(SidebarGroupItem(group: g, color: Theme.group(g.color), dots: dots, open: !collapsed.contains(g.id),
                                            selected: g.sessionIds.contains { selected.contains($0) }, hover: hover, first: first,
                                            waitingCount: waiting), in: r)
    }

    private func drawSession(_ s: Session, group g: Group, in r: CGRect, hover: Bool) {
        let slide = appeared[s.id].flatMap { Feedback.progress(since: $0, duration: Self.appearDuration) }
        if let slide, let ctx = NSGraphicsContext.current?.cgContext {
            NSGraphicsContext.saveGraphicsState()
            ctx.translateBy(x: (1 - slide) * -24, y: 0)
            ctx.setAlpha(slide)
        }
        defer { if slide != nil { NSGraphicsContext.restoreGraphicsState() } }
        renderer.drawSession(SidebarSessionItem(session: s, color: Theme.group(g.color), dot: dotColor(s),
                                                attached: isAttached(s.id),
                                                selected: selected.contains(s.id), focused: focused == s.id,
                                                keyFocus: window?.firstResponder === self, hover: hover,
                                                message: showMessages ? messages[s.id] : nil, showAge: showAge,
                                                unread: unread.contains(s.id)), in: r)
    }

    // MARK: Accessibility

    /// Eine Zeile je Element, Klick wie mit der Maus, die Hover-Knöpfe als Aktionen. Rahmen folgen der Breite, daher
    /// bei jeder Abfrage neu, die Elemente selbst bleiben je Zeile dieselben.
    private var a11y: [A11yElement] = []
    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .outline }
    override func accessibilityLabel() -> String? { String(localized: "Sessions") }

    /// Zeilen der ausgewählten Sessions, für VO-Pfeile in einer echten Outline.
    override func accessibilitySelectedRows() -> [Any]? {
        let keys = Set(selected.map(Row.key(session:)))
        return a11y.filter { keys.contains($0.key) }
    }

    override func accessibilityChildren() -> [Any]? {
        a11y = rows.enumerated().map { i, row in
            let e = a11y.reuse(row.key), frame = rowRect(i).scaled(Theme.scale)
            let tools = toolbar(i).buttons.map { a11yAction($0.hit.label, $0.hit.action) }
            switch row {
            case .group(let g):
                let n = g.sessionIds.count(where: { sessions[$0] != nil })
                let label = n == 1 ? String(localized: "Gruppe \(g.name), 1 Session") : String(localized: "Gruppe \(g.name), \(n) Sessions")
                e.update(parent: self, role: .row, label: label, frame: frame,
                        press: { [weak self] in self?.onSelect?(g.sessionIds, .replace) },
                        // Reihenfolge im VoiceOver-Menü wie bisher: Neue Session vor Favorit.
                        actions: [a11yAction(collapsed.contains(g.id) ? String(localized: "Ausklappen") : String(localized: "Einklappen")) { [weak self] in self?.toggleCollapsed(g.id) },
                                  tools[1], tools[0], tools[2], tools[3]])
                e.setAccessibilityDisclosureLevel(0)
                e.setAccessibilityExpanded(!collapsed.contains(g.id))
                return e
            case .session(let s, _):
                let status = s.status.spoken(attached: isAttached(s.id))
                let extra = [unread.contains(s.id) ? String(localized: "neu") : nil, selected.contains(s.id) ? String(localized: "ausgewählt") : nil].compactMap { $0 }
                let value = ([status] + extra).joined(separator: ", ")
                e.update(parent: self, role: .row, label: String(localized: "Session \(s.title)"), value: value, frame: frame,
                        press: { [weak self] in self?.anchor = s.id; self?.onSelect?([s.id], .replace) },
                        actions: tools)
                e.setAccessibilityDisclosureLevel(1)
                e.setAccessibilitySelected(selected.contains(s.id))
                return e
            }
        }
        return a11y
    }

    // MARK: Events

    /// Nur per ⌘1, nicht per Klick: sonst zeigt die alte Fokus-Zeile zwischen Drücken und Loslassen kurz den
    /// Tastatur-Rahmen, danach holt sich die Kachel die Tastatur ohnehin zurück (Flackern).
    override var acceptsFirstResponder: Bool { NSApp.currentEvent?.type != .leftMouseDown }
    override func becomeFirstResponder() -> Bool { needsDisplay = true; return true }
    override func resignFirstResponder() -> Bool { needsDisplay = true; return true }

    /// ⌘1 gibt dem Baum die Tastatur: ↑↓ wandert über die sichtbaren Sessions, ←/→ klappt die Gruppe der
    /// fokussierten Session zu/auf, ⏎/Leertaste öffnet sie, ⌫ schließt sie (mit Rückfrage), ⌃⏎/⇧F10 zeigt ihr
    /// Kontextmenü, ⌥⌘↑/↓ tauscht sie mit dem Nachbarn in der Gruppe.
    override func keyDown(with event: NSEvent) {
        let mods = event.modifierFlags.intersection(Hotkey.modMask)
        switch (event.keyCode, mods) {
        case (KeyCode.down, [.option, .command]): moveFocused(step: 1)
        case (KeyCode.up, [.option, .command]): moveFocused(step: -1)
        case (KeyCode.down, []): step(1)
        case (KeyCode.up, []): step(-1)
        case (KeyCode.left, []): collapseFocusedGroup()
        case (KeyCode.right, []): expandFocusedGroup()
        case (KeyCode.returnKey, [.control]), (KeyCode.f10, [.shift]): if let id = focused { showContextMenu(for: id) }
        case (KeyCode.returnKey, []), (KeyCode.space, []): if let id = focused { onSelect?([id], .replace) }
        case (KeyCode.delete, []): if let id = focused { onCloseSession?(id, false) }
        default: super.keyDown(with: event)
        }
    }

    private func step(_ d: Int) {
        guard let id = sessionId(after: focused, step: d) else { return }
        anchor = id
        onSelect?([id], .cursor)
        reveal(id)
    }

    /// Gruppe der fokussierten Session, auch wenn diese durch Einklappen gerade nicht sichtbar ist.
    private func groupOfFocused() -> Group? {
        focused.flatMap { id in groups.first { $0.sessionIds.contains(id) } }
    }

    private func collapseFocusedGroup() {
        guard let g = groupOfFocused(), !collapsed.contains(g.id) else { return }
        toggleCollapsed(g.id)
    }

    private func expandFocusedGroup() {
        guard let g = groupOfFocused(), collapsed.contains(g.id) else { return }
        toggleCollapsed(g.id)
        if let id = focused { reveal(id) }
    }

    /// Nachbar der fokussierten Session innerhalb ihrer eigenen Gruppe (nicht baumweit): ⌥⌘↑/↓ tauscht damit.
    private func moveFocused(step: Int) {
        guard let id = focused, let g = groupOfFocused(), let i = g.sessionIds.firstIndex(of: id),
              g.sessionIds.indices.contains(i + step) else { return }
        onMoveSession?(id, g.sessionIds[i + step])
    }

    private func showContextMenu(for id: String) {
        guard let menu = onContextMenu?(id), let i = sessionRow(id) else { return }
        let r = rowRect(i).scaled(Theme.scale)
        menu.popUp(positioning: nil, at: CGPoint(x: r.minX + 20, y: r.midY), in: self)
    }

    /// Nachbar in Baumreihenfolge über die sichtbaren Sessions, am Rand bleibt es stehen. Ohne Start die erste.
    func sessionId(after id: String?, step: Int) -> String? {
        let ids = sessionIds
        guard !ids.isEmpty else { return nil }
        return ids[id.flatMap { ids.firstIndex(of: $0) }.map { min(max($0 + step, 0), ids.count - 1) } ?? 0]
    }

    func reveal(_ id: String) {
        if let r = sessionRow(id) { scrollToVisible(rowRect(r).scaled(Theme.scale)) }
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
        let k = i.flatMap { toolbar($0).buttons.firstIndex { $0.hit.rect.contains(p) } }
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
        // Toolbar nur, wo sie sichtbar ist: ohne Hover (Fenster nicht aktiv) wählt der Klick die Zeile.
        if hovered == i, case let tb = toolbar(i, flags: event.modifierFlags), tb.rect.contains(p) {
            tb.buttons.map(\.hit).first(at: p)?.action()
            return
        }
        switch rows[i] {
        case .group(let g):
            if p.x < 26 { toggleCollapsed(g.id); return }
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
        let t = target(for: press.row, at: p)
        if let t, t.key != dropTarget?.key { Feedback.snap() }
        dropTarget = t
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
            // Klick auf eine schon ausgewählte Session nimmt sie wieder heraus, wie ⌘-Klick.
            onSelect?([s.id], cmd || selected.contains(s.id) ? .toggle : .replace)
        }
    }
}
