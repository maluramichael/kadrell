import AppKit

/// Rechte Sidebar: ein Tab je Extension mit Panel, darunter ihr Baum. Gezeichnet wie der Baum links, in unskalierten
/// Punkten (siehe `Theme.scaled`). Tastatur per ⌘3: ↑↓ Eintrag, ←→ Tab, ⏎ erste Aktion, ⌥⏎ alle Aktionen, Esc zurück.
/// Zeichnen steht in `ExtensionPanelView+Drawing.swift`.
@MainActor
final class ExtensionPanelView: ClipWidthView {
    var panels: [(name: String, tree: PanelTree)] = [] {
        didSet {
            if !panels.contains(where: { $0.name == selectedTab }) { selectedTab = panels.first?.name }
            layoutRows()
        }
    }
    var selectedTab: String? {
        didSet {
            guard selectedTab != oldValue else { return }
            cursor = nil
            layoutRows()
        }
    }
    var onAction: (_ name: String, _ id: String) -> Void = { _, _ in }
    var onLeave: () -> Void = {}

    static let tabHeight: CGFloat = 30

    /// Eine sichtbare Zeile des gewählten Tabs. `key` (Tab + Pfad) erkennt Abschnitte über `panel.set` hinweg wieder.
    struct Line {
        let node: PanelNode, key: String, depth: Int, y: CGFloat, height: CGFloat
        var selectable: Bool { if case .text = node { false } else { true } }
    }
    private(set) var lines: [Line] = []
    /// Abschnitte, die von Hand anders auf- oder zugeklappt sind, als die Extension sie liefert.
    private var toggled: Set<String> = []
    private(set) var cursor: Int?
    private(set) var hovered: Int?
    /// Gedrückte Zeile: ausgelöst wird erst beim Loslassen über derselben Zeile.
    private var pressed: Int?

    override var isFlipped: Bool { true }

    static func same(_ a: [(name: String, tree: PanelTree)], _ b: [(name: String, tree: PanelTree)]) -> Bool {
        a.count == b.count && zip(a, b).allSatisfy { $0.name == $1.name && $0.tree == $1.tree }
    }

    static func label(_ p: (name: String, tree: PanelTree)) -> String { p.tree.title.isEmpty ? p.name : p.tree.title }

    func isOpen(_ l: Line) -> Bool {
        guard case .section(_, let collapsed, _) = l.node else { return false }
        return collapsed == toggled.contains(l.key)
    }

    /// Zeilen neu aufbauen und die eigene Höhe setzen. Auch nach einem Wechsel der UI-Größe.
    func layoutRows() {
        lines = []
        var y = Self.tabHeight + 4
        if let tab = selectedTab, let tree = panels.first(where: { $0.name == tab })?.tree { flatten(tree.nodes, depth: 0, path: tab, y: &y) }
        if let c = cursor, !(lines.indices.contains(c) && lines[c].selectable) { cursor = nil }
        if let h = hovered, !lines.indices.contains(h) { hovered = nil }
        let want = max(((y + 6) * Theme.scale).rounded(.up), superview?.bounds.height ?? 0)
        if frame.height != want { setFrameSize(NSSize(width: frame.width, height: want)) }
        needsDisplay = true
    }

    private func flatten(_ nodes: [PanelNode], depth: Int, path: String, y: inout CGFloat) {
        for (i, node) in nodes.enumerated() {
            let line = Line(node: node, key: path + "/\(i)", depth: depth, y: y, height: Self.height(node))
            lines.append(line)
            y += line.height
            if case .section(_, _, let children) = node, isOpen(line) { flatten(children, depth: depth + 1, path: line.key, y: &y) }
        }
    }

    private static func height(_ node: PanelNode) -> CGFloat {
        switch node {
        case .section: 26
        case .item: 24
        case .text: 20
        case .button: 32
        }
    }

    func rect(_ i: Int) -> CGRect { CGRect(x: 0, y: lines[i].y, width: bounds.width / Theme.scale, height: lines[i].height) }

    func tabRects() -> [CGRect] {
        var x: CGFloat = 4
        return panels.map { p in
            let w = NSAttributedString(string: Self.label(p), attributes: Theme.attrs(11, Theme.fg, bold: true)).size().width + 16
            defer { x += w }
            return CGRect(x: x, y: 0, width: w, height: Self.tabHeight)
        }
    }

    // MARK: Tastatur

    /// Nur per ⌘3, nicht per Klick: wie beim Baum, sonst blitzt der Tastatur-Rahmen bei jedem Klick auf.
    override var acceptsFirstResponder: Bool { NSApp.currentEvent?.type != .leftMouseDown }
    override func becomeFirstResponder() -> Bool { needsDisplay = true; return true }
    override func resignFirstResponder() -> Bool { needsDisplay = true; return true }
    var hasKeyboard: Bool { window?.firstResponder === self }

    override func keyDown(with event: NSEvent) {
        let mods = event.modifierFlags.intersection(Hotkey.modMask)
        switch (event.keyCode, mods) {
        case (KeyCode.down, []): step(1)
        case (KeyCode.up, []): step(-1)
        case (KeyCode.left, []): switchTab(-1)
        case (KeyCode.right, []): switchTab(1)
        case (KeyCode.returnKey, []), (KeyCode.keypadEnter, []), (KeyCode.space, []): if let c = cursor { activate(c) }
        case (KeyCode.returnKey, [.option]): if let c = cursor { showActions(c) }
        case (KeyCode.escape, []): onLeave()
        default: super.keyDown(with: event)
        }
    }

    /// Über wählbare Zeilen (Abschnitte, Einträge, Knöpfe), am Rand bleibt es stehen. Ohne Auswahl: erste bzw. letzte.
    private func step(_ d: Int) {
        let rows = lines.indices.filter { lines[$0].selectable }
        guard !rows.isEmpty else { return }
        let pos = cursor.flatMap { rows.firstIndex(of: $0) }.map { min(max($0 + d, 0), rows.count - 1) } ?? (d > 0 ? 0 : rows.count - 1)
        cursor = rows[pos]
        scrollToVisible(rect(rows[pos]).scaled(Theme.scale))
        needsDisplay = true
        NSAccessibility.post(element: self, notification: .valueChanged)
    }

    private func switchTab(_ d: Int) {
        guard let i = panels.firstIndex(where: { $0.name == selectedTab }) else { return }
        selectedTab = panels[min(max(i + d, 0), panels.count - 1)].name
    }

    /// Abschnitt klappt, Eintrag löst seine erste Aktion aus, Knopf seine.
    private func activate(_ i: Int) {
        switch lines[i].node {
        case .section:
            if !toggled.insert(lines[i].key).inserted { toggled.remove(lines[i].key) }
            layoutRows()
        case .item(_, _, _, let actions): if let a = actions.first { fire(a.id) }
        case .button(_, let action): fire(action)
        case .text: break
        }
    }

    private func fire(_ id: String) {
        if let tab = selectedTab { onAction(tab, id) }
    }

    /// Tab und Aktion werden beim Bauen festgehalten: ein panel.set bei offenem Menü darf sie nicht umlenken.
    private func actionMenu(_ i: Int) -> NSMenu? {
        guard case .item(_, _, _, let actions) = lines[i].node, !actions.isEmpty, let tab = selectedTab else { return nil }
        let menu = NSMenu()
        for a in actions {
            let m = NSMenuItem(title: a.label, action: #selector(menuAction(_:)), keyEquivalent: "")
            m.target = self
            m.representedObject = [tab, a.id]
            menu.addItem(m)
        }
        return menu
    }

    @objc private func menuAction(_ sender: NSMenuItem) {
        if let target = sender.representedObject as? [String], target.count == 2 { onAction(target[0], target[1]) }
    }

    private func showActions(_ i: Int) {
        let r = rect(i).scaled(Theme.scale)
        actionMenu(i)?.popUp(positioning: nil, at: CGPoint(x: r.minX + 20, y: r.maxY), in: self)
    }

    // MARK: Maus

    private func local(_ event: NSEvent) -> CGPoint {
        let p = convert(event.locationInWindow, from: nil)
        return CGPoint(x: p.x / Theme.scale, y: p.y / Theme.scale)
    }

    private func lineIndex(at p: CGPoint) -> Int? { lines.indices.first { rect($0).contains(p) } }

    override func mouseDown(with event: NSEvent) {
        let p = local(event)
        pressed = nil
        if p.y < Self.tabHeight {
            if let k = tabRects().firstIndex(where: { $0.contains(p) }) { selectedTab = panels[k].name }
            return
        }
        guard let i = lineIndex(at: p), lines[i].selectable else { return }
        pressed = i
        cursor = i
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard let i = pressed else { return }
        pressed = nil
        if lineIndex(at: local(event)) == i { activate(i) }
    }

    /// Rechtsklick: alle Aktionen des Eintrags unter dem Zeiger.
    override func menu(for event: NSEvent) -> NSMenu? { lineIndex(at: local(event)).flatMap(actionMenu) }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for t in trackingAreas { removeTrackingArea(t) }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    override func mouseMoved(with event: NSEvent) {
        let i = lineIndex(at: local(event)).flatMap { lines[$0].selectable ? $0 : nil }
        (i == nil ? NSCursor.arrow : NSCursor.pointingHand).set()
        guard i != hovered else { return }
        hovered = i
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        hovered = nil
        needsDisplay = true
    }

    // MARK: Accessibility

    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .list }
    override func accessibilityLabel() -> String? {
        let tab = panels.first { $0.name == selectedTab }.map(Self.label) ?? ""
        return String(localized: "Extensions: \(tab)", bundle: Bundle.app)
    }
    override func accessibilityValue() -> Any? { cursor.map { Self.spoken(lines[$0].node) } }

    private static func spoken(_ node: PanelNode) -> String {
        switch node {
        case .section(let title, _, _): title
        case .item(let text, let detail, _, _): [text, detail].compactMap { $0 }.joined(separator: ", ")
        case .text(let text, _): text
        case .button(let label, _): label
        }
    }
}
