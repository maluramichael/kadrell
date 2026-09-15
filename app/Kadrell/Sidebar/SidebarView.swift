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
    private var collapsed: Set<String> = []
    private var rows: [Row] = []
    private var hovered: Int?
    /// Letzte einzeln angeklickte Session: Startpunkt für ⇧-Bereiche.
    private var anchor: String?
    private var pulseTask: Task<Void, Never>?

    enum SelectMode { case replace, toggle, add }
    /// Klick = nur diese, ⌘-Klick = dazu oder weg, ⇧-Klick = Bereich seit dem letzten Klick dazu.
    var onSelect: (([String], SelectMode) -> Void)?
    var onActivateSession: ((Session) -> Void)?
    var onNewSession: ((String) -> Void)?
    var onEditGroup: ((String) -> Void)?
    /// Zweiter Parameter: ⌘ gehalten, dann ohne Rückfrage.
    var onCloseGroup: ((String, Bool) -> Void)?
    var onCloseSession: ((String, Bool) -> Void)?

    private enum Row { case group(Group), session(Session, Group) }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        pulseTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(80))
                guard let self, self.sessions.values.contains(where: { $0.status == .running }) else { continue }
                self.needsDisplay = true
            }
        }
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

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

    private func rowHeight(_ r: Row) -> CGFloat { if case .group = r { SidebarView.groupRow } else { SidebarView.sessionRow } }

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
    }

    private func drawGroup(_ g: Group, in r: CGRect, hover: Bool) {
        let color = NSColor(hexString: g.color)
        let any = g.sessionIds.contains { selected.contains($0) }
        if hover { Theme.surface.setFill(); r.fill() }
        if any { color.setFill(); CGRect(x: 0, y: r.minY, width: 3, height: r.height).fill() }
        Icons.chevron(in: CGRect(x: 6, y: r.midY - 8, width: 16, height: 16), open: !collapsed.contains(g.id), color: Theme.muted)
        let name = NSAttributedString(string: "▪ " + g.name, attributes: Theme.attrs(12, color, bold: true))
        let icons = hover ? 3 : 0
        let right = r.maxX - 8 - CGFloat(icons) * 20
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

    private func drawSession(_ s: Session, group g: Group, in r: CGRect, hover: Bool) {
        let color = NSColor(hexString: g.color)
        let sel = selected.contains(s.id), foc = focused == s.id
        if foc { color.mixed(0.14, into: Theme.surface).setFill(); r.fill() }
        else if sel || hover { Theme.surface.setFill(); r.fill() }
        if sel { color.setFill(); CGRect(x: 0, y: r.minY, width: 3, height: r.height).fill() }
        let attached = attach?.isAttached(s.id) ?? false
        let c = attached || !s.canAttach ? Theme.color(for: s.status) : Theme.detached
        let t = CACurrentMediaTime().truncatingRemainder(dividingBy: 1.2) / 1.2
        let pulse = 0.3 + 0.7 * (0.5 + 0.5 * cos(2 * .pi * t))
        (s.status == .running && s.canAttach ? c.withAlphaComponent(pulse) : c).setFill()
        NSBezierPath(ovalIn: CGRect(x: 27, y: r.midY - 4, width: 8, height: 8)).fill()
        var right = r.maxX - 8
        if hover {
            Icons.x(in: iconRects(r, count: 1)[0], color: Theme.muted)
            right -= 20
        } else {
            let age = NSAttributedString(string: s.elapsed(), attributes: Theme.attrs(10.5, Theme.muted))
            right -= age.size().width
            age.draw(at: CGPoint(x: right, y: r.midY - 7))
        }
        let title = NSAttributedString(string: s.name, attributes: Theme.attrs(12, sel || hover ? Theme.fg : Theme.sub))
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
        guard let i = rowIndex(at: p) else { return }
        let r = rowRect(i)
        let shift = event.modifierFlags.contains(.shift), cmd = event.modifierFlags.contains(.command)
        let force = cmd
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
            onSelect?(g.sessionIds, shift || cmd ? .add : .replace)
        case .session(let s, _):
            if iconRects(r, count: 1)[0].insetBy(dx: -3, dy: -3).contains(p) { onCloseSession?(s.id, force); return }
            if shift, let a = anchor, let ai = sessionIndex(a), let bi = sessionIndex(s.id) {
                onSelect?(sessionIds[min(ai, bi)...max(ai, bi)].map { $0 }, .add)
                return
            }
            anchor = s.id
            if !cmd, s.isDone || s.isStale { onActivateSession?(s); return }
            onSelect?([s.id], cmd ? .toggle : .replace)
        }
    }
}
