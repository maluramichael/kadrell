import AppKit

/// ⌘P Omni-Leiste: Fuzzy-Suche über Sessions, Gruppen, Pfade, letzte Zeilen; `>` schaltet in den Kommandomodus.
@MainActor
final class PaletteWindow: NSPanel, NSTextFieldDelegate, NSTableViewDataSource, NSTableViewDelegate {
    struct Item {
        let label: String
        let sub: String
        let group: String?
        let status: SessionStatus?
        let sessionKey: String?
        let run: () -> Void
    }
    struct Source {
        var sessions: [(Session, group: Group?, lines: [String])] = []
        var groups: [Group] = []
        var commands: [(String, () -> Void)] = []
        var onFocusSession: (String) -> Void = { _ in }
        var onFitGroup: (String) -> Void = { _ in }
    }

    var source = Source()
    var onHighlight: ((Set<String>?) -> Void)?
    private let field = NSTextField()
    private let table = NSTableView()
    private let scroll = NSScrollView()
    private let foot = NSTextField(labelWithString: "↑↓ wählen · ⏎ öffnen · Esc schließen")
    private var items: [Item] = []
    private var selected = 0

    init() {
        super.init(contentRect: NSRect(x: 0, y: 0, width: 640, height: 420), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        appearance = NSAppearance(named: .darkAqua)
        let root = NSView(frame: contentRect(forFrameRect: frame))
        root.wantsLayer = true
        root.layer?.backgroundColor = Theme.panel.cgColor
        root.layer?.borderColor = Theme.line.cgColor
        root.layer?.borderWidth = 1
        contentView = root

        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = Theme.font(15)
        field.textColor = Theme.fg
        field.placeholderAttributedString = NSAttributedString(string: "Session, Gruppe, Pfad suchen …  ( > für Kommandos )", attributes: [.font: Theme.font(15), .foregroundColor: Theme.muted])
        field.delegate = self
        field.frame = NSRect(x: 16, y: 420 - 44, width: 640 - 32, height: 24)
        field.autoresizingMask = [.width, .minYMargin]
        root.addSubview(field)
        let sep = NSView(frame: NSRect(x: 0, y: 420 - 56, width: 640, height: 1))
        sep.wantsLayer = true; sep.layer?.backgroundColor = Theme.line.cgColor
        sep.autoresizingMask = [.width, .minYMargin]
        root.addSubview(sep)

        let col = NSTableColumn(identifier: .init("c"))
        col.width = 600
        table.addTableColumn(col)
        table.headerView = nil
        table.backgroundColor = .clear
        table.rowHeight = 44
        table.intercellSpacing = .zero
        table.selectionHighlightStyle = .none
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(rowClicked)
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.frame = NSRect(x: 0, y: 34, width: 640, height: 420 - 56 - 34)
        scroll.autoresizingMask = [.width, .height]
        root.addSubview(scroll)

        foot.font = Theme.font(11)
        foot.textColor = Theme.muted
        foot.alignment = .right
        foot.frame = NSRect(x: 16, y: 10, width: 640 - 32, height: 16)
        foot.autoresizingMask = [.width, .maxYMargin]
        root.addSubview(foot)
        let sep2 = NSView(frame: NSRect(x: 0, y: 34, width: 640, height: 1))
        sep2.wantsLayer = true; sep2.layer?.backgroundColor = Theme.line.cgColor
        sep2.autoresizingMask = [.width, .maxYMargin]
        root.addSubview(sep2)
    }

    override var canBecomeKey: Bool { true }

    var isCommandMode: Bool { field.stringValue.hasPrefix(">") }

    func open(over parent: NSWindow, prefix: String = "") {
        let pf = parent.frame
        setFrameOrigin(NSPoint(x: pf.midX - 320, y: pf.maxY - 0.12 * pf.height - 420))
        field.stringValue = prefix
        host = parent
        parent.addChildWindow(self, ordered: .above)
        makeKeyAndOrderFront(nil)
        Backdrop.sync(parent)
        makeFirstResponder(field)
        field.currentEditor()?.moveToEndOfLine(nil)
        refreshList()
    }

    func dismiss(runHighlightReset: Bool = true) {
        host?.removeChildWindow(self)
        orderOut(nil)
        if runHighlightReset { onHighlight?(nil) }
    }

    /// Wie bei OverlayPanel: `parent` ist nach orderOut/close nil, der Blur hängt am gemerkten Hauptfenster.
    private weak var host: NSWindow?
    override func orderOut(_ sender: Any?) { super.orderOut(sender); Backdrop.sync(host) }
    override func close() { super.close(); Backdrop.sync(host) }

    func switchToCommandMode() {
        if !isCommandMode { field.stringValue = ">"; field.currentEditor()?.moveToEndOfLine(nil); refreshList() }
    }

    nonisolated static func fuzzy(_ q: String, _ s: String) -> Bool {
        let q = Array(q.lowercased()), s = s.lowercased()
        var i = 0
        for c in s where i < q.count && c == q[i] { i += 1 }
        return i == q.count
    }

    private func refreshList() {
        let q = field.stringValue
        selected = 0
        if q.hasPrefix(">") {
            let t = q.dropFirst().trimmingCharacters(in: .whitespaces)
            items = source.commands.filter { t.isEmpty || PaletteWindow.fuzzy(t, $0.0) }
                .map { Item(label: $0.0, sub: "Kommando", group: nil, status: nil, sessionKey: nil, run: $0.1) }
            onHighlight?(nil)
        } else {
            var list: [Item] = []
            if !q.isEmpty {
                list += source.groups.filter { PaletteWindow.fuzzy(q, $0.name) }.map { g in
                    Item(label: g.name, sub: "Gruppe · " + g.cwd, group: nil, status: nil, sessionKey: nil, run: { [source] in source.onFitGroup(g.id) })
                }
            }
            list += source.sessions.filter { s, _, lines in
                q.isEmpty || PaletteWindow.fuzzy(q, s.title + " " + s.cwd + " " + lines.suffix(3).joined(separator: " "))
            }.map { s, g, lines in
                Item(label: s.title, sub: String((lines.last ?? Theme.shortPath(s.cwd)).prefix(70)), group: g?.name, status: s.status,
                     sessionKey: s.id, run: { [source] in source.onFocusSession(s.id) })
            }
            items = list
            onHighlight?(q.isEmpty ? nil : Set(list.compactMap(\.sessionKey)))
        }
        table.reloadData()
        if !items.isEmpty { table.scrollRowToVisible(0) }
    }

    func controlTextDidChange(_ obj: Notification) { refreshList() }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        switch sel {
        case #selector(NSResponder.moveDown(_:)): move(1); return true
        case #selector(NSResponder.moveUp(_:)): move(-1); return true
        case #selector(NSResponder.insertNewline(_:)): activate(selected); return true
        case #selector(NSResponder.cancelOperation(_:)): dismiss(); return true
        default: return false
        }
    }

    private func move(_ d: Int) {
        guard !items.isEmpty else { return }
        selected = max(0, min(items.count - 1, selected + d))
        table.reloadData()
        table.scrollRowToVisible(selected)
    }

    private func activate(_ i: Int) {
        guard items.indices.contains(i) else { return }
        let run = items[i].run
        dismiss()
        run()
    }

    @objc private func rowClicked() { activate(table.clickedRow) }

    func numberOfRows(in tableView: NSTableView) -> Int { max(items.count, 1) }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let v = PaletteRow()
        if items.isEmpty { v.item = nil } else { v.item = items[row]; v.selected = row == selected }
        return v
    }
}

@MainActor
final class PaletteRow: NSView {
    var item: PaletteWindow.Item?
    var selected = false
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        let b = bounds
        if selected {
            Theme.surface.setFill(); b.fill()
            Theme.running.setFill(); CGRect(x: 0, y: 0, width: 3, height: b.height).fill()
        }
        guard let item else {
            NSAttributedString(string: "Keine Treffer", attributes: Theme.attrs(12, Theme.muted)).draw(at: CGPoint(x: 16, y: 14))
            return
        }
        var x: CGFloat = 16
        if let st = item.status {
            Theme.color(for: st).setFill()
            NSBezierPath(ovalIn: CGRect(x: x, y: 13, width: 8, height: 8)).fill()
            x += 18
        }
        let g = item.group.map { NSAttributedString(string: $0, attributes: Theme.attrs(11, Theme.muted)) }
        let gw = g?.size().width ?? 0
        NSAttributedString(string: item.label, attributes: Theme.attrs(12.5, Theme.fg)).draw(with: CGRect(x: x, y: 7, width: b.width - x - gw - 32, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        NSAttributedString(string: item.sub, attributes: Theme.attrs(11, Theme.muted)).draw(with: CGRect(x: x, y: 24, width: b.width - x - gw - 32, height: 14), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        g?.draw(at: CGPoint(x: b.width - 16 - gw, y: 8))
    }
}
