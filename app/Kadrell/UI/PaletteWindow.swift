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
        var indent: Int = 0
        var attached = true
        var shellRunning = false
        let run: () -> Void
    }
    struct Source {
        var sessions: [(Session, group: Group?, lines: [String])] = []
        var groups: [Group] = []
        var commands: [(String, () -> Void)] = []
        var onFocusSession: (String) -> Void = { _ in }
        /// Ob die Session einen laufenden Prozess hat; getrennte sind grau und warten nicht auf dich.
        var isAttached: (String) -> Bool = { _ in true }
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
    /// Nach dem Schließen (Esc, ohne Auswahl): dem Hauptfenster die Tastatur zurückgeben, sonst hängt der Fokus.
    var onClose: (() -> Void)?
    private let field = NSTextField()
    private(set) var items: [Item] = []
    private let table = NSTableView()
    private let scroll = NSScrollView()
    private let foot = NSTextField(labelWithString: String(localized: "↑↓ wählen · ⏎ öffnen · Esc schließen", bundle: Bundle.app))
    private var selected = 0
    /// tmux-Sessions je Host, einmal pro Öffnen geholt; nil = Abfrage läuft, leeres Ergebnis = kein tmux-Server.
    private var remoteCache: [String: [String]?] = [:]
    /// `source.buffers()`/`source.hosts()`: teuer, deshalb erst bei tatsächlichem Bedarf und dann nur einmal pro Öffnen geholt.
    private var buffersCache: [(Session, group: Group?, lines: [String])]?
    /// Kleingeschriebene Puffer (einmal pro Öffnen) und, solange der Begriff nur wächst, die Sessions der letzten Treffer.
    private var lowerCache: [LoweredBuffer]?
    private var lastTerm = ""
    private var lastHitIds: Set<String>?
    private var scanTask: Task<Void, Never>?
    /// Zwischen Anschlag und Ergebnis zeigt die Liste noch den alten Begriff, ⏎ darf dann nichts ausführen.
    private var scanPending = false
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

        field.cell = CenteredTextFieldCell(textCell: "")   // sonst klebt der Text an der Oberkante des Feldes
        field.isEditable = true
        field.isSelectable = true
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = Theme.font(15 * s)
        field.textColor = Theme.fg
        field.placeholderAttributedString = NSAttributedString(string: String(localized: "Session, Gruppe, Pfad suchen …  ( > Kommandos, / in allen Terminals, @ Remote )", bundle: Bundle.app), attributes: [.font: Theme.font(15 * s), .foregroundColor: Theme.muted])
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
        let maxW = min(pf.width, (parent.screen ?? NSScreen.main)?.visibleFrame.width ?? pf.width) - 48
        setContentSize(NSSize(width: min((640 * Theme.scale).rounded(), maxW), height: contentRect(forFrameRect: frame).height))
        setFrameOrigin(NSPoint(x: pf.midX - frame.width / 2, y: pf.maxY - 0.12 * pf.height - frame.height))
        field.stringValue = prefix
        remoteCache = [:]
        buffersCache = nil
        lowerCache = nil
        lastTerm = ""
        lastHitIds = nil
        hostsCache = nil
        attach(to: parent)
        makeFirstResponder(field)
        field.currentEditor()?.moveToEndOfLine(nil)
        refreshList()
    }

    override func dismiss() { dismiss(runHighlightReset: true) }
    func dismiss(runHighlightReset: Bool) {
        scanTask?.cancel()
        scanPending = false
        searchTask?.cancel()
        super.dismiss()
        if runHighlightReset { onHighlight?(nil) }
        onClose?()
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
        if !q.hasPrefix("/") { scanTask?.cancel(); scanPending = false }
        if q.hasPrefix(">") {
            let t = q.dropFirst().trimmingCharacters(in: .whitespaces)
            items = source.commands.filter { t.isEmpty || PaletteWindow.fuzzy(t, $0.0) }
                .map { Item(label: $0.0, sub: String(localized: "Kommando", bundle: Bundle.app), group: nil, status: nil, sessionKey: nil, run: $0.1) }
            onHighlight?(nil)
        } else if q.hasPrefix("/") {
            startTerminalSearch(String(q.dropFirst()))
            return
        } else if q.hasPrefix("@") {
            items = remoteItems(String(q.dropFirst()))
            onHighlight?(nil)
        } else {
            let list = q.isEmpty ? flatSessionItems() : groupedItems(q)
            items = list
            onHighlight?(q.isEmpty ? nil : Set(list.compactMap(\.sessionKey)))
        }
        table.reloadData()
        if !items.isEmpty { table.scrollRowToVisible(0) }
    }

    /// ⌘P ohne Suchbegriff: flache Liste, wer auf dich wartet zuerst.
    private func flatSessionItems() -> [Item] {
        source.sessions
            .sorted { rank($0.0) < rank($1.0) }
            .map { sessionItem($0, indent: 0, showGroup: true) }
    }

    private func rank(_ s: Session) -> Int { s.status == .waiting && source.isAttached(s.id) ? 0 : 1 }

    /// Mit Suchbegriff: nach Gruppen gebündelt wie in der Seitenleiste. Passt der Gruppenname, stehen alle ihre
    /// Sessions eingerückt darunter; sonst nur die selbst passenden. Gruppenlose Treffer kommen unten.
    private func groupedItems(_ q: String) -> [Item] {
        var list: [Item] = []
        var shown = Set<String>()
        for g in source.groups where PaletteWindow.fuzzy(q, g.name) {
            shown.insert(g.id)
            list.append(groupItem(g))
            list += sessionsOf(g.id).map { sessionItem($0, indent: 1, showGroup: false) }
        }
        for g in source.groups where !shown.contains(g.id) {
            let hits = sessionsOf(g.id).filter { sessionMatches(q, $0) }
            guard !hits.isEmpty else { continue }
            list.append(groupItem(g))
            list += hits.map { sessionItem($0, indent: 1, showGroup: false) }
        }
        list += source.sessions.filter { $0.group == nil && sessionMatches(q, $0) }
            .map { sessionItem($0, indent: 0, showGroup: true) }
        return list
    }

    private func sessionsOf(_ gid: String) -> [(Session, group: Group?, lines: [String])] {
        source.sessions.filter { $0.group?.id == gid }
    }

    // Terminal-Puffer bleibt der `/`-Suche vorbehalten: sonst matchen fremde Sessions über zufälligen Puffer-Inhalt.
    private func sessionMatches(_ q: String, _ e: (Session, group: Group?, lines: [String])) -> Bool {
        PaletteWindow.fuzzy(q, e.0.title + " " + e.0.cwd)
    }

    private func sessionItem(_ e: (Session, group: Group?, lines: [String]), indent: Int, showGroup: Bool) -> Item {
        Item(label: e.0.title, sub: String((e.lines.last ?? Theme.shortPath(e.0.cwd)).prefix(70)),
             group: showGroup ? e.group?.name : nil, status: e.0.status, sessionKey: e.0.id, indent: indent,
             attached: source.isAttached(e.0.id), shellRunning: e.0.hasRunningShell,
             run: { [source] in source.onFocusSession(e.0.id) })
    }

    private func groupItem(_ g: Group) -> Item {
        Item(label: "▾ " + g.name, sub: Theme.shortPath(g.cwd), group: nil, status: nil, sessionKey: nil, indent: 0,
             run: { [source] in source.onFitGroup(g.id) })
    }

    /// `@text` filtert die Hosts, ⏎ hängt sich an deren laufende tmux. `@host:text` zeigt die tmux-Sessions des
    /// Hosts (asynchron, bis dahin „lädt …“) und oben „Neu: text“, ⏎ legt sie an oder hängt sich an.
    private func remoteItems(_ q: String) -> [Item] {
        if hostsCache == nil { hostsCache = source.hosts() }
        let hosts = hostsCache ?? []
        guard let colon = q.firstIndex(of: ":") else {
            return hostItems(q, hosts)
        }
        let host = String(q[..<colon]), text = q[q.index(after: colon)...].trimmingCharacters(in: .whitespaces)
        var list: [Item] = []
        if !text.isEmpty {
            list.append(Item(label: String(localized: "Neu: \(text)", bundle: Bundle.app), sub: String(localized: "tmux new -As auf \(host)", bundle: Bundle.app), group: nil, status: nil, sessionKey: nil,
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
                let msg = remoteError[host] == true ? String(localized: "\(host) nicht erreichbar", bundle: Bundle.app) : String(localized: "kein tmux-Server auf \(host)", bundle: Bundle.app)
                list.append(Item(label: msg, sub: String(localized: "„Neu: …“ versucht es trotzdem", bundle: Bundle.app), group: nil, status: nil, sessionKey: nil, run: {}))
            }
            list += names.filter { text.isEmpty || PaletteWindow.fuzzy(text, $0) }.map { n in
                Item(label: n, sub: String(localized: "tmux-Session auf \(host)", bundle: Bundle.app), group: nil, status: nil, sessionKey: nil,
                     run: { [source] in source.onConnect(host, n) })
            }
        default:
            list.append(Item(label: String(localized: "lädt …", bundle: Bundle.app), sub: String(localized: "tmux ls auf \(host)", bundle: Bundle.app), group: nil, status: nil, sessionKey: nil, run: {}))
        }
        return list
    }
    /// Host-Treffer; ohne Treffer bietet ein getippter Host (`user@host`) das direkte Verbinden an.
    private func hostItems(_ q: String, _ hosts: [String]) -> [Item] {
        var list = hosts.filter { q.isEmpty || PaletteWindow.fuzzy(q, $0) }.map { h in
            Item(label: h, sub: String(localized: "ssh · ⏎ tmux attach · „\(h):“ wählt die Session", bundle: Bundle.app), group: nil, status: nil, sessionKey: nil,
                 run: { [source] in source.onConnect(h, nil) })
        }
        if list.isEmpty, !q.isEmpty, !q.contains(" "), !q.hasPrefix("-") {
            list.append(Item(label: String(localized: "Verbinden mit \(q) (ssh)", bundle: Bundle.app), sub: String(localized: "ssh · ⏎ tmux attach", bundle: Bundle.app), group: nil, status: nil, sessionKey: nil,
                             run: { [source] in source.onConnect(q, nil) }))
        }
        if list.isEmpty, hosts.isEmpty {
            list.append(Item(label: String(localized: "Hosts aus ~/.ssh/config · user@host direkt tippen", bundle: Bundle.app), sub: "", group: nil, status: nil, sessionKey: nil, run: {}))
        }
        return list
    }
    private var remoteError: [String: Bool] = [:]

    struct LoweredBuffer: Sendable {
        let session: Session
        let group: Group?
        let lines: [String]
        let lower: [String]
    }
    struct Hit: Sendable {
        let session: Session
        let group: Group?
        let label: String
        let index: Int
    }

    /// Treffer zeilenweise, ohne Groß-/Kleinschreibung. Die Nummer des Treffers in der Session springt im Terminal
    /// per `findNext` an dieselbe Stelle. Der Scan läuft abseits des Main-Threads und bricht beim nächsten Anschlag ab.
    private func startTerminalSearch(_ term: String) {
        scanTask?.cancel()
        scanPending = false
        guard term.count >= 2 else {
            items = []; lastHitIds = nil; lastTerm = ""
            onHighlight?([])
            table.reloadData()
            return
        }
        if buffersCache == nil { buffersCache = source.buffers() }
        scanPending = true
        let raw = buffersCache ?? [], cache = lowerCache
        let only = lastHitIds.flatMap { !lastTerm.isEmpty && term.lowercased().contains(lastTerm) ? $0 : nil }
        scanTask = Task.detached { [weak self] in
            let bufs = cache ?? raw.map { LoweredBuffer(session: $0.0, group: $0.group, lines: $0.lines, lower: $0.lines.map { $0.lowercased() }) }
            let found = PaletteWindow.scan(bufs.filter { only?.contains($0.session.id) ?? true }, term: term.lowercased())
            guard !Task.isCancelled else { return }
            await self?.applyScan(term: term, bufs: bufs, found: found)
        }
    }

    nonisolated static func scan(_ bufs: [LoweredBuffer], term: String) -> (hits: [Hit], truncated: Bool) {
        var hits: [Hit] = []
        for b in bufs {
            var n = 0
            for (i, low) in b.lower.enumerated() {
                if i % 512 == 0, Task.isCancelled { return (hits, true) }
                guard low.contains(term) else { continue }
                hits.append(Hit(session: b.session, group: b.group, label: b.lines[i].trimmingCharacters(in: .whitespaces), index: n))
                var r = low.startIndex..<low.endIndex
                while let h = low.range(of: term, range: r) { n += 1; r = h.upperBound..<low.endIndex }
                // ponytail: feste Obergrenze, sonst wird die Tabelle bei „e“-artigen Begriffen zäh.
                if hits.count >= 300 { return (hits, true) }
            }
        }
        return (hits, false)
    }

    private func applyScan(term: String, bufs: [LoweredBuffer], found: (hits: [Hit], truncated: Bool)) {
        guard isVisible, field.stringValue.hasPrefix("/"), String(field.stringValue.dropFirst()) == term else { return }
        scanPending = false
        lowerCache = bufs
        lastTerm = term.lowercased()
        lastHitIds = found.truncated ? nil : Set(found.hits.map(\.session.id))
        items = found.hits.map { h in
            Item(label: h.label, sub: h.session.title, group: h.group?.name, status: h.session.status, sessionKey: h.session.id,
                 attached: source.isAttached(h.session.id), shellRunning: h.session.hasRunningShell,
                 run: { [source] in source.onFindInSession(h.session.id, term, h.index) })
        }
        selected = 0
        onHighlight?(Set(items.compactMap(\.sessionKey)))
        table.reloadData()
        if !items.isEmpty { table.scrollRowToVisible(0) }
    }

    /// Leicht verzögert statt pro Tastendruck: die Buffer-Suche (`/`) läuft mit `String.range(of:)` über den
    /// ganzen Verlauf aller Terminals, das soll nicht bei jedem Anschlag neu anlaufen.
    func controlTextDidChange(_ obj: Notification) {
        searchTask?.cancel()
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(50))
            guard !Task.isCancelled, let self else { return }
            self.searchTask = nil
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
            if scanPending { return true }
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
    override func accessibilityLabel() -> String? { item.map { $0.spoken } ?? String(localized: "Keine Treffer", bundle: Bundle.app) }

    private func drawRow(_ b: CGRect) {
        if selected {
            Theme.surface.setFill(); b.fill()
            Theme.running.setFill(); CGRect(x: 0, y: 0, width: 3, height: b.height).fill()
        }
        guard let item else {
            NSAttributedString(string: String(localized: "Keine Treffer", bundle: Bundle.app), attributes: Theme.attrs(12, Theme.muted)).draw(at: CGPoint(x: 16, y: 14))
            return
        }
        var x: CGFloat = 16 + CGFloat(item.indent) * 16
        if let st = item.status {
            Icons.statusDot(in: CGRect(x: x, y: 13, width: 8, height: 8), status: st, attached: item.attached,
                            color: Theme.statusColor(st, attached: item.attached, shellRunning: item.shellRunning))
            x += 18
        }
        let g = item.group.map { NSAttributedString(string: $0, attributes: Theme.attrs(11, Theme.muted)) }
        let gw = g?.size().width ?? 0
        NSAttributedString(string: item.label, attributes: Theme.attrs(12.5, Theme.fg)).draw(with: CGRect(x: x, y: 7, width: b.width - x - gw - 32, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        NSAttributedString(string: item.sub, attributes: Theme.attrs(11, Theme.muted)).draw(with: CGRect(x: x, y: 24, width: b.width - x - gw - 32, height: 14), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        g?.draw(at: CGPoint(x: b.width - 16 - gw, y: 8))
    }
}
