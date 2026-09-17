import AppKit

/// ⌘P Omni-Leiste: Fuzzy-Suche über Sessions, Gruppen, Pfade, letzte Zeilen; `>` schaltet in den Kommandomodus,
/// `/` durchsucht den Verlauf aller laufenden Terminals (⌘⇧F).
@MainActor
final class PaletteWindow: ChildPanel, NSTextFieldDelegate, NSTableViewDataSource, NSTableViewDelegate {
    struct Item {
        /// Vorgelesen: Titel, Status, Gruppe, Zusatzzeile.
        var spoken: String { [label, status?.spoken, group, sub].compactMap { $0 }.joined(separator: ", ") }
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
        /// Verlauf je laufendem Terminal: `getBufferAsData` plus UTF-8-Dekodierung kostet über alle Terminals
        /// hinweg spürbar, deshalb erst geholt, wenn `/`-Suche tatsächlich läuft, nicht schon beim Öffnen.
        var buffers: () -> [(Session, group: Group?, lines: [String])] = { [] }
        /// Session, Suchbegriff, wievielter Treffer in ihrem Verlauf (ab 0).
        var onFindInSession: (String, String, Int) -> Void = { _, _, _ in }
        /// `@`: ssh-Hosts, zuletzt benutzte vorn. `@host:` listet dessen tmux-Sessions, geholt über `remoteSessions`.
        /// Liest `~/.ssh/config`, deshalb erst bei tatsächlichem `@`-Modus statt bei jedem Öffnen.
        var hosts: () -> [String] = { [] }
        var onConnect: (String, String?) -> Void = { _, _ in }
        var remoteSessions: (String, @escaping ([String]?) -> Void) -> Void = { _, done in done(nil) }
    }

    var source = Source()
    var onHighlight: ((Set<String>?) -> Void)?
    private let field = NSTextField()
    private let table = NSTableView()
    private let scroll = NSScrollView()
    private let foot = NSTextField(labelWithString: String(localized: "↑↓ wählen · ⏎ öffnen · Esc schließen"))
    private var items: [Item] = []
    private var selected = 0
    /// tmux-Sessions je Host, einmal pro Öffnen geholt; nil = Abfrage läuft, leeres Ergebnis = kein tmux-Server.
    private var remoteCache: [String: [String]?] = [:]
    /// `source.buffers()`/`source.hosts()`: teuer, deshalb erst bei tatsächlichem Bedarf und dann nur einmal pro Öffnen geholt.
    private var buffersCache: [(Session, group: Group?, lines: [String])]?
    private var hostsCache: [String]?
    private var searchTask: Task<Void, Never>?

    init() {
        let s = Theme.scale, W = (640 * s).rounded(), H = (420 * s).rounded()
        super.init(size: NSSize(width: W, height: H))
        isOpaque = false
        backgroundColor = .clear
        let root = NSView(frame: contentRect(forFrameRect: frame))
        root.wantsLayer = true
        root.layer?.backgroundColor = Theme.panel.cgColor
        root.layer?.borderColor = Theme.line.cgColor
        root.layer?.borderWidth = 1
        contentView = root

        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = Theme.font(15 * s)
        field.textColor = Theme.fg
        field.placeholderAttributedString = NSAttributedString(string: String(localized: "Session, Gruppe, Pfad suchen …  ( > Kommandos, / in allen Terminals, @ Remote )"), attributes: [.font: Theme.font(15 * s), .foregroundColor: Theme.muted])
        field.delegate = self
        field.frame = NSRect(x: 16 * s, y: H - 44 * s, width: W - 32 * s, height: 24 * s)
        field.autoresizingMask = [.width, .minYMargin]
        root.addSubview(field)
        let sep = NSView(frame: NSRect(x: 0, y: H - 56 * s, width: W, height: 1))
        sep.wantsLayer = true; sep.layer?.backgroundColor = Theme.line.cgColor
        sep.autoresizingMask = [.width, .minYMargin]
        root.addSubview(sep)

        let col = NSTableColumn(identifier: .init("c"))
        col.width = W - 40
        table.addTableColumn(col)
        table.headerView = nil
        table.backgroundColor = .clear
        table.rowHeight = (44 * s).rounded()
        table.intercellSpacing = .zero
        table.selectionHighlightStyle = .none
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.action = #selector(rowClicked)
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.frame = NSRect(x: 0, y: 34 * s, width: W, height: H - 90 * s)
        scroll.autoresizingMask = [.width, .height]
        root.addSubview(scroll)

        foot.font = Theme.font(11 * s)
        foot.textColor = Theme.muted
        foot.alignment = .right
        foot.frame = NSRect(x: 16 * s, y: 10 * s, width: W - 32 * s, height: 16 * s)
        foot.autoresizingMask = [.width, .maxYMargin]
        root.addSubview(foot)
        let sep2 = NSView(frame: NSRect(x: 0, y: 34 * s, width: W, height: 1))
        sep2.wantsLayer = true; sep2.layer?.backgroundColor = Theme.line.cgColor
        sep2.autoresizingMask = [.width, .maxYMargin]
        root.addSubview(sep2)
    }

    var isCommandMode: Bool { field.stringValue.hasPrefix(">") }

    func open(over parent: NSWindow, prefix: String = "") {
        let pf = parent.frame
        setFrameOrigin(NSPoint(x: pf.midX - frame.width / 2, y: pf.maxY - 0.12 * pf.height - frame.height))
        field.stringValue = prefix
        remoteCache = [:]
        buffersCache = nil
        hostsCache = nil
        attach(to: parent)
        makeFirstResponder(field)
        field.currentEditor()?.moveToEndOfLine(nil)
        refreshList()
    }

    override func dismiss() { dismiss(runHighlightReset: true) }
    func dismiss(runHighlightReset: Bool) {
        super.dismiss()
        if runHighlightReset { onHighlight?(nil) }
    }

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
                .map { Item(label: $0.0, sub: String(localized: "Kommando"), group: nil, status: nil, sessionKey: nil, run: $0.1) }
            onHighlight?(nil)
        } else if q.hasPrefix("/") {
            items = terminalMatches(String(q.dropFirst()))
            onHighlight?(Set(items.compactMap(\.sessionKey)))
        } else if q.hasPrefix("@") {
            items = remoteItems(String(q.dropFirst()))
            onHighlight?(nil)
        } else {
            var list: [Item] = []
            if !q.isEmpty {
                list += source.groups.filter { PaletteWindow.fuzzy(q, $0.name) }.map { g in
                    Item(label: g.name, sub: String(localized: "Gruppe · \(g.cwd)"), group: nil, status: nil, sessionKey: nil, run: { [source] in source.onFitGroup(g.id) })
                }
            }
            var matches = source.sessions.filter { s, _, lines in
                q.isEmpty || PaletteWindow.fuzzy(q, s.title + " " + s.cwd + " " + lines.suffix(3).joined(separator: " "))
            }
            // Ohne Suchbegriff: wer auf dich wartet, steht zuerst, sonst bleibt die Reihenfolge der Gruppen.
            if q.isEmpty { matches.sort { ($0.0.status == .waiting ? 0 : 1) < ($1.0.status == .waiting ? 0 : 1) } }
            list += matches.map { s, g, lines in
                Item(label: s.title, sub: String((lines.last ?? Theme.shortPath(s.cwd)).prefix(70)), group: g?.name, status: s.status,
                     sessionKey: s.id, run: { [source] in source.onFocusSession(s.id) })
            }
            items = list
            onHighlight?(q.isEmpty ? nil : Set(list.compactMap(\.sessionKey)))
        }
        table.reloadData()
        if !items.isEmpty { table.scrollRowToVisible(0) }
    }

    /// `@text` filtert die Hosts, ⏎ hängt sich an deren laufende tmux. `@host:text` zeigt die tmux-Sessions des
    /// Hosts (asynchron, bis dahin „lädt …“) und oben „Neu: text“, ⏎ legt sie an oder hängt sich an.
    private func remoteItems(_ q: String) -> [Item] {
        if hostsCache == nil { hostsCache = source.hosts() }
        let hosts = hostsCache ?? []
        guard let colon = q.firstIndex(of: ":") else {
            return hosts.filter { q.isEmpty || PaletteWindow.fuzzy(q, $0) }.map { h in
                Item(label: h, sub: String(localized: "ssh · ⏎ tmux attach · „\(h):“ wählt die Session"), group: nil, status: nil, sessionKey: nil,
                     run: { [source] in source.onConnect(h, nil) })
            }
        }
        let host = String(q[..<colon]), text = q[q.index(after: colon)...].trimmingCharacters(in: .whitespaces)
        var list: [Item] = []
        if !text.isEmpty {
            list.append(Item(label: String(localized: "Neu: \(text)"), sub: String(localized: "tmux new -As auf \(host)"), group: nil, status: nil, sessionKey: nil,
                             run: { [source] in source.onConnect(host, text) }))
        }
        if remoteCache[host] == nil {
            remoteCache[host] = .some(nil)
            source.remoteSessions(host) { [weak self] found in
                Task { @MainActor in
                    guard let self else { return }
                    self.remoteCache[host] = .some(found ?? [])
                    self.remoteError[host] = found == nil
                    if self.isVisible { self.refreshList() }
                }
            }
        }
        switch remoteCache[host] {
        case .some(.some(let names)):
            if names.isEmpty {
                let msg = remoteError[host] == true ? String(localized: "\(host) nicht erreichbar") : String(localized: "kein tmux-Server auf \(host)")
                list.append(Item(label: msg, sub: String(localized: "„Neu: …“ versucht es trotzdem"), group: nil, status: nil, sessionKey: nil, run: {}))
            }
            list += names.filter { text.isEmpty || PaletteWindow.fuzzy(text, $0) }.map { n in
                Item(label: n, sub: String(localized: "tmux-Session auf \(host)"), group: nil, status: nil, sessionKey: nil,
                     run: { [source] in source.onConnect(host, n) })
            }
        default:
            list.append(Item(label: String(localized: "lädt …"), sub: String(localized: "tmux ls auf \(host)"), group: nil, status: nil, sessionKey: nil, run: {}))
        }
        return list
    }
    private var remoteError: [String: Bool] = [:]

    /// Treffer zeilenweise, ohne Groß-/Kleinschreibung wie die Suchleiste im Terminal. Die Nummer des Treffers
    /// in der Session springt dort per `findNext` an dieselbe Stelle.
    private func terminalMatches(_ term: String) -> [Item] {
        guard term.count >= 2 else { return [] }
        if buffersCache == nil { buffersCache = source.buffers() }
        var list: [Item] = []
        for (s, g, lines) in buffersCache ?? [] {
            var n = 0
            for line in lines {
                var r = line.startIndex..<line.endIndex
                var first: Int?
                while let hit = line.range(of: term, options: .caseInsensitive, range: r) {
                    if first == nil { first = n }
                    n += 1
                    r = hit.upperBound..<line.endIndex
                }
                guard let index = first else { continue }
                list.append(Item(label: line.trimmingCharacters(in: .whitespaces), sub: s.title, group: g?.name, status: s.status,
                                 sessionKey: s.id, run: { [source] in source.onFindInSession(s.id, term, index) }))
                // ponytail: feste Obergrenze, sonst wird die Tabelle bei „e“-artigen Begriffen zäh.
                if list.count >= 300 { return list }
            }
        }
        return list
    }

    /// Leicht verzögert statt pro Tastendruck: die Buffer-Suche (`/`) läuft mit `String.range(of:)` über den
    /// ganzen Verlauf aller Terminals, das soll nicht bei jedem Anschlag neu anlaufen.
    func controlTextDidChange(_ obj: Notification) {
        searchTask?.cancel()
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(50))
            guard !Task.isCancelled, let self else { return }
            self.refreshList()
        }
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
        switch sel {
        case #selector(NSResponder.moveDown(_:)): move(1); return true
        case #selector(NSResponder.moveUp(_:)): move(-1); return true
        case #selector(NSResponder.insertNewline(_:)):
            // Ein wartender, noch nicht gelaufener Suchdurchlauf darf ⏎ nicht auf veralteten Treffern ausführen.
            if searchTask != nil { searchTask?.cancel(); searchTask = nil; refreshList() }
            activate(selected)
            return true
        case #selector(NSResponder.cancelOperation(_:)): dismiss(); return true
        default: return false
        }
    }

    private func move(_ d: Int) {
        guard !items.isEmpty else { return }
        selected = max(0, min(items.count - 1, selected + d))
        table.reloadData()
        table.scrollRowToVisible(selected)
        // Die Auswahl ist nur gezeichnet, die Tastatur bleibt im Suchfeld: VoiceOver sagt die Zeile daher an.
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested,
                             userInfo: [.announcement: items[selected].spoken, .priority: NSAccessibilityPriorityLevel.high.rawValue])
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
    override func draw(_ dirtyRect: NSRect) { Theme.scaled(bounds) { drawRow($0) } }
    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .staticText }
    override func accessibilityLabel() -> String? { item.map { $0.spoken } ?? String(localized: "Keine Treffer") }

    private func drawRow(_ b: CGRect) {
        if selected {
            Theme.surface.setFill(); b.fill()
            Theme.running.setFill(); CGRect(x: 0, y: 0, width: 3, height: b.height).fill()
        }
        guard let item else {
            NSAttributedString(string: String(localized: "Keine Treffer"), attributes: Theme.attrs(12, Theme.muted)).draw(at: CGPoint(x: 16, y: 14))
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
