import AppKit

/// Linke Seite: Baum Gruppe › Sessions, handgezeichnet wie die Leiste. Liegt in einem NSScrollView und
/// setzt seine Höhe selbst.
@MainActor
final class SidebarView: NSView {
    static let groupRow: CGFloat = 26
    static let sessionRow: CGFloat = 24

    private(set) var groups: [Group] = []
    private(set) var sessions: [String: Session] = [:]
    var selected: Set<String> = []
    var focused: String?
    var attach: AttachManager?
    /// Eingeschaltet: jede Session-Zeile bekommt eine zweite Zeile mit `messages[id]`.
    var showMessages = false
    var messages: [String: String] = [:]
    private var collapsed: Set<String> = []
    private var rows: [Row] = []
    private var hovered: Int?
    /// Letzte einzeln angeklickte Session: Startpunkt für ⇧-Bereiche.
    private var anchor: String?
    private var pulseTask: Task<Void, Never>?
    /// Gedrückte Zeile: Klick wird erst beim Loslassen ausgewertet, Ziehen sortiert um. Zeilen als Wert,
    /// nicht als Index: ein Poll kann den Baum zwischen Drücken und Loslassen neu aufbauen.
    private var pressed: (point: CGPoint, row: Row, flags: NSEvent.ModifierFlags)?
    private var dragging = false
    private var dropTarget: Row?

    enum SelectMode { case replace, toggle, add }
    /// Klick = nur diese, ⌘-Klick = dazu oder weg, ⇧-Klick = Bereich seit dem letzten Klick dazu.
    var onSelect: (([String], SelectMode) -> Void)?
    var onNewSession: ((String) -> Void)?
    var onEditGroup: ((String) -> Void)?
    /// Zweiter Parameter: ⌘ gehalten, dann ohne Rückfrage.
    var onCloseGroup: ((String, Bool) -> Void)?
    var onCloseSession: ((String, Bool) -> Void)?
    /// Ziehen: (gezogen, Ziel), Session innerhalb ihrer Gruppe bzw. Gruppe vor/hinter eine andere.
    var onMoveSession: ((String, String) -> Void)?
    var onMoveGroup: ((String, String) -> Void)?

    private enum Row {
        case group(Group), session(Session, Group)
        var key: String { switch self { case .group(let g): "g:" + g.id; case .session(let s, _): "s:" + s.id } }
    }
    private func index(of r: Row) -> Int? { rows.firstIndex { $0.key == r.key } }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        pulseTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(80))
                guard let self, self.sessions.values.contains(where: { $0.status == .running || $0.isPending }) else { continue }
                self.needsDisplay = true
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
        self.groups = groups
        self.sessions = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        rows = []
        for g in groups {
            rows.append(.group(g))
            guard !collapsed.contains(g.id) else { continue }
            for s in g.sessionIds.compactMap({ self.sessions[$0] }) { rows.append(.session(s, g)) }
        }
        let h = rows.reduce(CGFloat(12)) { $0 + rowHeight($1) } + CGFloat(groups.count) * 6
        let want = max(h, superview?.bounds.height ?? 0)
        if frame.height != want { setFrameSize(NSSize(width: frame.width, height: want)) }
        needsDisplay = true
    }

    private func rowHeight(_ r: Row) -> CGFloat {
        if case .group = r { SidebarView.groupRow } else { SidebarView.sessionRow + (showMessages ? 14 : 0) }
    }
    /// Obere Zeile einer Session: Punkt, Titel, Laufzeit und Icon sitzen hier, auch mit Nachrichtenzeile darunter.
    private func headRect(_ r: CGRect) -> CGRect { CGRect(x: r.minX, y: r.minY, width: r.width, height: SidebarView.sessionRow) }

    private func rowRect(_ i: Int) -> CGRect {
        var y: CGFloat = 6
        for (j, r) in rows.enumerated() {
            if case .group = r, j > 0 { y += 6 }
            let h = rowHeight(r)
            if j == i { return CGRect(x: 0, y: y, width: bounds.width, height: h) }
            y += h
        }
        return .zero
    }

    private func rowIndex(at p: CGPoint) -> Int? { rows.indices.first { rowRect($0).contains(p) } }
    /// Sichtbare Sessions in Baumreihenfolge (Bereichsauswahl läuft über Gruppen hinweg).
    private var sessionIds: [String] { rows.compactMap { if case .session(let s, _) = $0 { s.id } else { nil } } }
    private func sessionIndex(_ id: String) -> Int? { sessionIds.firstIndex(of: id) }
    /// Trefferflächen rechts in der Zeile, von rechts nach links: nur bei Hover sichtbar.
    private func iconRects(_ r: CGRect, count: Int) -> [CGRect] {
        (0..<count).map { CGRect(x: r.maxX - 8 - CGFloat($0 + 1) * 20, y: r.midY - 8, width: 16, height: 16) }
    }

    // MARK: Zeichnen

    override func draw(_ dirtyRect: NSRect) {
        Theme.panel.setFill()
        dirtyRect.fill()
        for (i, row) in rows.enumerated() {
            let r = rowRect(i)
            guard r.intersects(dirtyRect) else { continue }
            switch row {
            case .group(let g): drawGroup(g, in: r, hover: hovered == i)
            case .session(let s, let g): drawSession(s, group: g, in: r, hover: hovered == i)
            }
        }
        if let y = dropLineY() { Theme.fg.setFill(); CGRect(x: 0, y: y - 1, width: bounds.width, height: 2).fill() }
    }

    /// Einfügelinie: über dem Ziel beim Ziehen nach oben, darunter (bei Gruppen unter deren letzter Zeile) nach unten.
    private func dropLineY() -> CGFloat? {
        guard dragging, let tr = dropTarget, let t = index(of: tr), let sr = pressed?.row, let src = index(of: sr) else { return nil }
        guard t > src else { return rowRect(t).minY }
        guard case .group = rows[t] else { return rowRect(t).maxY }
        let end = rows.indices.dropFirst(t + 1).first { if case .group = rows[$0] { true } else { false } } ?? rows.count
        return rowRect(end - 1).maxY
    }

    private func drawGroup(_ g: Group, in r: CGRect, hover: Bool) {
        let color = NSColor(hexString: g.color)
        let any = g.sessionIds.contains { selected.contains($0) }
        if hover { Theme.surface.setFill(); r.fill() }
        if any { color.setFill(); CGRect(x: 0, y: r.minY, width: 3, height: r.height).fill() }
        Icons.chevron(in: CGRect(x: 6, y: r.midY - 8, width: 16, height: 16), open: !collapsed.contains(g.id), color: Theme.muted)
        let name = NSAttributedString(string: "▪ " + g.name, attributes: Theme.attrs(12, color, bold: true))
        let right = r.maxX - 8 - 3 * 20   // Platz für die Icons immer reservieren, sonst springt der Text beim Hover
        let nameW = min(name.size().width, max(0, right - 26))
        name.draw(with: CGRect(x: 26, y: r.midY - 8, width: nameW, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        let path = NSAttributedString(string: Theme.shortPath(g.cwd), attributes: Theme.attrs(10.5, Theme.muted))
        let px = 26 + nameW + 6
        if right - px > 20 { path.draw(with: CGRect(x: px, y: r.midY - 7, width: right - px, height: 14), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine]) }
        if hover {
            let rects = iconRects(r, count: 3)
            Icons.x(in: rects[0], color: Theme.muted)
            Icons.pen(in: rects[1], color: Theme.muted)
            Icons.plus(in: rects[2], color: Theme.muted)
        }
    }

    private func drawSession(_ s: Session, group g: Group, in row: CGRect, hover: Bool) {
        let color = NSColor(hexString: g.color)
        let sel = selected.contains(s.id), foc = focused == s.id
        if foc { color.mixed(0.14, into: Theme.surface).setFill(); row.fill() }
        else if sel || hover { Theme.surface.setFill(); row.fill() }
        if sel { color.setFill(); CGRect(x: 0, y: row.minY, width: 3, height: row.height).fill() }
        let r = headRect(row)
        if showMessages, let m = messages[s.id] {
            // Ende der Antwort: dort steht meist, worauf die Session wartet. Vorn abgeschnitten.
            let style = NSMutableParagraphStyle()
            style.lineBreakMode = .byTruncatingHead
            var attrs = Theme.attrs(10.5, Theme.muted)
            attrs[.paragraphStyle] = style
            NSAttributedString(string: String(m.suffix(300)), attributes: attrs)
                .draw(in: CGRect(x: 50, y: r.maxY - 4, width: max(0, row.maxX - 8 - 50), height: 14))
        }
        let attached = attach?.isAttached(s.id) ?? false
        let c = attached || !s.canAttach ? Theme.color(for: s.status) : Theme.detached
        let t = CACurrentMediaTime().truncatingRemainder(dividingBy: 1.2) / 1.2
        let pulse = 0.3 + 0.7 * (0.5 + 0.5 * cos(2 * .pi * t))
        if s.isPending {
            Icons.spinner(in: CGRect(x: 26, y: r.midY - 5, width: 10, height: 10), color: Theme.sub)
        } else {
            (s.status == .running && s.canAttach ? c.withAlphaComponent(pulse) : c).setFill()
            NSBezierPath(ovalIn: CGRect(x: 27, y: r.midY - 4, width: 8, height: 8)).fill()
        }
        if hover { Icons.x(in: iconRects(r, count: 1)[0], color: Theme.muted) }
        var right = r.maxX - 8 - 20   // Icon-Platz immer reserviert
        let age = NSAttributedString(string: s.elapsed(), attributes: Theme.attrs(10.5, Theme.muted))
        right -= age.size().width
        age.draw(at: CGPoint(x: right, y: r.midY - 7))
        let title = NSAttributedString(string: s.title, attributes: Theme.attrs(12, sel || hover ? Theme.fg : Theme.sub))
        title.draw(with: CGRect(x: 42, y: r.midY - 8, width: max(0, right - 8 - 42), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    // MARK: Events

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for t in trackingAreas { removeTrackingArea(t) }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    override func mouseMoved(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        guard p.x < bounds.width - ThinSplitView.grabWidth / 2 else { return }   // Griffzone des Trenners
        let i = rowIndex(at: p)
        (i == nil ? NSCursor.arrow : NSCursor.pointingHand).set()
        guard i != hovered else { return }
        hovered = i
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        hovered = nil
        needsDisplay = true
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        pressed = nil
        guard let i = rowIndex(at: p) else { return }
        let r = rowRect(i)
        let force = event.modifierFlags.contains(.command)
        switch rows[i] {
        case .group(let g):
            let icons = iconRects(r, count: 3)
            if icons[0].insetBy(dx: -3, dy: -3).contains(p) { onCloseGroup?(g.id, force); return }
            if icons[1].insetBy(dx: -3, dy: -3).contains(p) { onEditGroup?(g.id); return }
            if icons[2].insetBy(dx: -3, dy: -3).contains(p) { onNewSession?(g.id); return }
            if p.x < 26 {
                if collapsed.contains(g.id) { collapsed.remove(g.id) } else { collapsed.insert(g.id) }
                reload(groups: groups, sessions: Array(sessions.values))
                return
            }
        case .session(let s, _):
            if iconRects(headRect(r), count: 1)[0].insetBy(dx: -3, dy: -3).contains(p) { onCloseSession?(s.id, force); return }
        }
        pressed = (p, rows[i], event.modifierFlags)
    }

    override func mouseDragged(with event: NSEvent) {
        guard let press = pressed else { return }
        let p = convert(event.locationInWindow, from: nil)
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
