import SwiftUI

/// Zustand von ⌘N: ein Suchfeld über Gruppen, benutzte Ordner und Git-Repos. Esc und ⏎ kommen vom OverlayPanel,
/// deshalb liegt `start()` am Modell.
@Observable
@MainActor
final class NewSessionModel {
    struct Candidate: Identifiable {
        let path: String
        let group: Group?
        /// Herkunft rechts in der Zeile, wenn es keine Gruppe ist: „Finder“, „Zwischenablage“, „Git“.
        let tag: String
        var id: String { path }
    }

    let groups: [Group]
    let counts: [String: Int]
    var query = "" { didSet { if query != oldValue { update() } } }
    private(set) var candidates: [Candidate] = []
    var selected = 0
    var onStart: ((Group?, String) -> Void)?
    private var context: [(path: String, tag: String)] = []
    private let index: FolderIndex

    /// `askFinder: false` in Tests: sonst fragt macOS nach der Erlaubnis, den Finder zu steuern.
    init(groups: [Group], counts: [String: Int], index: FolderIndex = .shared, askFinder: Bool = true) {
        self.groups = groups
        self.counts = counts
        self.index = index
        if let c = FolderIndex.clipboardFolder() { context.append((c, String(localized: "Zwischenablage"))) }
        update()
        guard askFinder else { return }
        Task { [weak self] in
            guard let f = await FolderIndex.finderFolder(), let self else { return }
            self.context.removeAll { $0.path == f }
            self.context.insert((f, "Finder"), at: 0)
            // Nicht unter den Fingern umsortieren: nur, solange noch nichts getippt oder gewählt ist.
            if self.query.isEmpty, self.selected == 0 { self.update() }
        }
    }

    /// Mit „/“ oder „~“: Pfad, jedes Stück darf abgekürzt sein (`~/d/p/kad`). Sonst Wortfetzen gegen den Index.
    func update() {
        let q = query.trimmingCharacters(in: .whitespaces)
        let byCwd = Dictionary(groups.map { ($0.cwd, $0) }, uniquingKeysWith: { a, _ in a })
        if q.hasPrefix("/") || q.hasPrefix("~") || q.contains("/") {
            candidates = FolderIndex.expandAbbreviated(q).map { p in
                let p = FolderIndex.normalize(p)
                return Candidate(path: p, group: byCwd[p], tag: "")
            }
        } else {
            var tags: [String: String] = [:]
            var order: [String] = []
            func add(_ p: String, _ tag: String) { if tags[p] == nil { tags[p] = tag; order.append(p) } }
            for c in context { add(c.path, c.tag) }
            for g in groups { add(g.cwd, "") }
            for p in index.uses.keys where FolderIndex.isDirectory(p) { add(p, "") }
            for p in index.repos { add(p, "Git") }
            let contextPaths = Set(context.map(\.path))
            let scored = order.compactMap { p -> (path: String, ctx: Bool, rank: Int, score: Double)? in
                let g = byCwd[p]
                // Gruppenname zählt wie ein Ordnername: „kad“ findet die Gruppe Kadrell auch in claude-agent-overview.
                let byName = g.flatMap { FolderIndex.rank(q, path: p + "/" + $0.name) }
                guard let r = [FolderIndex.rank(q, path: p), byName].compactMap({ $0 }).min() else { return nil }
                let groupBonus = g.map { 0.5 + Double(counts[$0.id] ?? 0) * 0.1 } ?? 0
                return (p, contextPaths.contains(p), r, index.frecency(p) + groupBonus)
            }
            candidates = scored.sorted {
                if $0.ctx != $1.ctx { return $0.ctx }
                if $0.rank != $1.rank { return $0.rank < $1.rank }
                if $0.score != $1.score { return $0.score > $1.score }
                return $0.path.lowercased() < $1.path.lowercased()
            }
            .prefix(60).map { Candidate(path: $0.path, group: byCwd[$0.path], tag: tags[$0.path] ?? "") }
        }
        selected = 0
    }

    /// Neuer Stand aus dem Index (Repo-Scan fertig), ohne eine schon getroffene Auswahl umzuwerfen.
    func refreshIfUntouched() { if selected == 0 { update() } }

    func move(_ d: Int) { selected = max(0, min(max(candidates.count - 1, 0), selected + d)) }

    /// Tab: markierten Pfad ins Feld übernehmen, ein „/“ danach zeigt seine Unterordner.
    func tab() {
        guard candidates.indices.contains(selected) else { return }
        query = Theme.shortPath(candidates[selected].path)
    }

    func pick(_ i: Int) {
        guard candidates.indices.contains(i) else { return }
        onStart?(candidates[i].group, candidates[i].path)
    }

    /// ⏎: markierten Treffer starten, ohne Treffer einen ausgeschrieben getippten Ordner.
    func start() {
        if !candidates.isEmpty { pick(selected); return }
        let dir = FolderIndex.normalize(query)
        if FolderIndex.isDirectory(dir) { onStart?(nil, dir) }
    }

    /// ⌘O: nativer Ordnerdialog, für alles, was die Suche nicht findet.
    func browse() {
        let start = candidates.indices.contains(selected) ? candidates[selected].path : Settings.startFolder
        chooseFolder(start: start) { [weak self] p in if let p { self?.onStart?(nil, p) } }
    }

    /// Ordner aus dem Finder hineingezogen: sofort dort starten.
    func drop(_ urls: [URL]) -> Bool {
        guard let dir = urls.lazy.compactMap({ FolderIndex.folder(for: $0.path) }).first else { return false }
        onStart?(nil, dir)
        return true
    }
}

/// ⌘N: tippen filtert Gruppen, benutzte Ordner und Git-Repos, ⏎ startet.
struct NewSessionView: View {
    @Bindable var model: NewSessionModel
    @State private var dropping = false

    init(model: NewSessionModel, onStart: @escaping (Group?, String) -> Void) {
        self.model = model
        model.onStart = onStart
    }

    var body: some View {
        VStack(spacing: 0) {
            Text("NEUE SESSION").font(Theme.ui(11)).kerning(0.6).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.top, 12)
            PathField(text: $model.query, placeholder: String(localized: "Projekt suchen oder Pfad tippen (~/d/p/kad)"),
                      onTab: model.tab, onSubmit: model.start, onMove: model.move)
                .frame(height: 22 * Theme.scale).padding(14)
            Divider().overlay(Theme.lineColor)
            list
            DialogFoot(hint: String(localized: "⏎ starten · Tab übernehmen · ⌘O Finder · Ordner hineinziehen"), button: String(localized: "Starten")) { model.start() }
        }
        .font(Theme.ui(12))
        .foregroundStyle(Theme.fgColor)
        .frame(width: 640 * Theme.scale)
        .background(Theme.panelColor)
        .overlay { if dropping { Rectangle().stroke(Theme.runningColor, lineWidth: 2) } }
        .dropDestination(for: URL.self) { urls, _ in model.drop(urls) } isTargeted: { dropping = $0 }
        .background { Button("", action: model.browse).keyboardShortcut("o", modifiers: .command).opacity(0) }
    }

    private var list: some View {
        let items = model.candidates
        return ScrollViewReader { proxy in
            ScrollView {
                VStack(spacing: 0) {
                    if items.isEmpty {
                        Text("Kein Ordner gefunden. ⌘O wählt im Finder.").foregroundStyle(Theme.mutedColor)
                            .frame(maxWidth: .infinity, alignment: .leading).padding(16)
                    }
                    ForEach(Array(items.enumerated()), id: \.element.id) { i, c in row(c, on: i == model.selected)
                        .onTapGesture { model.pick(i) }
                    }
                }
            }
            .frame(height: min(368, CGFloat(max(items.count, 1)) * 46) * Theme.scale)
            .onChange(of: model.selected) { _, s in if items.indices.contains(s) { proxy.scrollTo(items[s].id) } }
        }
    }

    private func row(_ c: NewSessionModel.Candidate, on: Bool) -> some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text(c.group?.name ?? (c.path as NSString).lastPathComponent).foregroundStyle(Theme.fgColor).lineLimit(1)
                Text(Theme.shortPath(c.path)).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1).truncationMode(.head)
            }
            Spacer()
            Text(c.group.map { String(localized: "\(model.counts[$0.id] ?? 0) Sessions") } ?? c.tag).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
        }
        .padding(.vertical, 8).padding(.horizontal, 16)
        .frame(height: 46 * Theme.scale)
        .background(on ? Theme.surfaceColor : .clear)
        .overlay(alignment: .leading) { if on { Rectangle().fill(Theme.runningColor).frame(width: 3) } }
        .contentShape(Rectangle())
        .id(c.id)
    }
}

extension Theme {
    /// Schrift der SwiftUI-Dialoge, mit der UI-Größe skaliert.
    static func ui(_ size: CGFloat, bold: Bool = false) -> Font {
        .custom(bold ? "JetBrainsMonoNF-Bold" : "JetBrainsMonoNF-Regular", size: size * scale)
    }

    static var bgColor: Color { Color(nsColor: bg) }
    static var panelColor: Color { Color(nsColor: panel) }
    static var surfaceColor: Color { Color(nsColor: surface) }
    static var lineColor: Color { Color(nsColor: line) }
    static var fgColor: Color { Color(nsColor: fg) }
    static var mutedColor: Color { Color(nsColor: muted) }
    static var runningColor: Color { Color(nsColor: running) }
}


/// AppKit-Textfeld für Pfade: Cursor am Ende, Tab vervollständigt, Pfeile wählen, ⏎ startet.
struct PathField: NSViewRepresentable {
    @Binding var text: String
    var placeholder = ""
    var autofocus = true
    var onTab: () -> Void
    var onSubmit: () -> Void
    var onMove: (Int) -> Void

    func makeNSView(context: Context) -> NSTextField {
        let f = NSTextField()
        f.isBordered = false
        f.drawsBackground = false
        f.focusRingType = .none
        f.font = Theme.font(13 * Theme.scale)
        f.textColor = Theme.fg
        f.usesSingleLineMode = true      // lange Pfade scrollen horizontal statt umzubrechen
        f.cell?.wraps = false
        f.cell?.isScrollable = true
        f.lineBreakMode = .byClipping
        f.delegate = context.coordinator
        f.stringValue = text
        f.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: [.font: Theme.font(13 * Theme.scale), .foregroundColor: Theme.muted])
        if autofocus { DispatchQueue.main.async {
            f.window?.makeFirstResponder(f)
            f.currentEditor()?.selectedRange = NSRange(location: f.stringValue.utf16.count, length: 0)
        } }
        return f
    }

    func updateNSView(_ f: NSTextField, context: Context) {
        guard f.stringValue != text else { return }
        f.stringValue = text
        f.currentEditor()?.selectedRange = NSRange(location: text.utf16.count, length: 0)
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: PathField
        init(_ p: PathField) { parent = p }
        func controlTextDidChange(_ n: Notification) {
            if let f = n.object as? NSTextField { parent.text = f.stringValue }
        }
        func control(_ control: NSControl, textView: NSTextView, doCommandBy sel: Selector) -> Bool {
            switch sel {
            case #selector(NSResponder.insertTab(_:)): parent.onTab(); return true
            case #selector(NSResponder.insertNewline(_:)): parent.onSubmit(); return true
            case #selector(NSResponder.moveDown(_:)): parent.onMove(1); return true
            case #selector(NSResponder.moveUp(_:)): parent.onMove(-1); return true
            default: return false
            }
        }
    }
}
