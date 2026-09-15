import SwiftUI

/// Zustand von ⌘N. Esc und ⌘⏎ kommen vom OverlayPanel, deshalb liegt `start()` am Modell.
@Observable
@MainActor
final class NewSessionModel {
    struct Entry: Identifiable { let id: String; let label: String; let sub: String; let meta: String; let group: Group? }
    let groups: [Group]
    let counts: [String: Int]
    var step: Int
    var filter = ""
    var selected = 0
    var chosen: Group?
    var cwd: String
    var compSelected = 0
    var focusRequest = 0
    var onStart: ((Group?, String) -> Void)?

    var completions: [String] { folderCompletions(for: cwd) }

    nonisolated static func expand(_ s: String) -> String {
        s.hasPrefix("~") ? NSHomeDirectory() + s.dropFirst() : s
    }

    init(groups: [Group], counts: [String: Int], preselected: Group?) {
        self.groups = groups
        self.counts = counts
        step = (preselected != nil || groups.isEmpty) ? 2 : 1
        chosen = preselected
        cwd = preselected?.cwd ?? Settings.startFolder
    }

    var entries: [Entry] {
        groups.filter { filter.isEmpty || PaletteWindow.fuzzy(filter, $0.name) }
            .map { Entry(id: $0.id, label: $0.name, sub: Theme.shortPath($0.cwd), meta: "\(counts[$0.id] ?? 0) Sessions", group: $0) }
            + [Entry(id: "new", label: "Neue Gruppe …", sub: "Ordner wählen, Ordnername wird Gruppenname", meta: "", group: nil)]
    }

    /// Bestehende Gruppe: Session startet sofort in deren Ordner. „Neue Gruppe“: Ordner abfragen.
    func pick(_ i: Int) {
        guard entries.indices.contains(i) else { return }
        chosen = entries[i].group
        if let g = chosen { onStart?(g, g.cwd); return }
        cwd = Settings.startFolder
        step = 2
        focusRequest += 1
    }

    func start() {
        guard step == 2 else { pick(selected); return }
        var dir = NewSessionModel.expand(cwd.trimmingCharacters(in: .whitespacesAndNewlines))
        while dir.count > 1, dir.hasSuffix("/") { dir.removeLast() }
        guard !dir.isEmpty else { focusRequest += 1; return }
        onStart?(chosen, dir)
    }
}

/// ⌘N: Schritt 1 Gruppe wählen (startet sofort), Schritt 2 nur bei neuer Gruppe: Ordner.
struct NewSessionView: View {
    @Bindable var model: NewSessionModel
    @FocusState private var focus: Field?
    enum Field { case filter }

    init(model: NewSessionModel, onStart: @escaping (Group?, String) -> Void) {
        self.model = model
        model.onStart = onStart
    }

    var body: some View {
        VStack(spacing: 0) {
            if model.step == 1 {
                label("Neue Session · Gruppe wählen")
                TextField("Gruppe filtern …", text: $model.filter)
                    .textFieldStyle(.plain).font(.custom("JetBrainsMonoNF-Regular", size: 15)).padding(14)
                    .focused($focus, equals: .filter)
                    .onChange(of: model.filter) { _, _ in model.selected = 0 }
                    .onSubmit { model.pick(model.selected) }
                    .onKeyPress(.downArrow) { model.selected = min(model.entries.count - 1, model.selected + 1); return .handled }
                    .onKeyPress(.upArrow) { model.selected = max(0, model.selected - 1); return .handled }
                Divider().overlay(Theme.lineColor)
                let entries = model.entries
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(spacing: 0) {
                            ForEach(Array(entries.enumerated()), id: \.element.id) { i, e in
                                HStack(spacing: 10) {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(e.label).foregroundStyle(Theme.fgColor)
                                        Text(e.sub).font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
                                    }
                                    Spacer()
                                    Text(e.meta).font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
                                }
                                .padding(.vertical, 8).padding(.horizontal, 16)
                                .frame(height: 46)
                                .background(i == model.selected ? Theme.surfaceColor : .clear)
                                .overlay(alignment: .leading) { if i == model.selected { Rectangle().fill(Theme.runningColor).frame(width: 3) } }
                                .contentShape(Rectangle())
                                .onTapGesture { model.pick(i) }
                                .id(e.id)
                            }
                        }
                    }
                    .frame(height: min(322, CGFloat(entries.count) * 46))
                    .onChange(of: model.selected) { _, s in if entries.indices.contains(s) { proxy.scrollTo(entries[s].id) } }
                }
                foot("⏎ starten · Esc abbrechen")
            } else {
                label("Neue Session · Ordner")
                FolderInput(path: $model.cwd, selected: $model.compSelected) { }
                DialogFoot(hint: "Tab oder ⏎ vervollständigen · Esc abbrechen", button: "Starten") { model.start() }
            }
        }
        .font(.custom("JetBrainsMonoNF-Regular", size: 12))
        .foregroundStyle(Theme.fgColor)
        .frame(width: 640)
        .background(Theme.panelColor)
        .onAppear { focusStep() }
        .onChange(of: model.focusRequest) { _, _ in DispatchQueue.main.async { focusStep() } }
    }

    private func focusStep() {
        focus = model.step == 1 ? .filter : nil
    }

    private func label(_ s: String) -> some View {
        Text(s.uppercased()).font(.custom("JetBrainsMonoNF-Regular", size: 11)).kerning(0.6).foregroundStyle(Theme.mutedColor)
            .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.top, 12)
    }

    private func foot(_ s: String) -> some View {
        VStack(spacing: 0) {
            Divider().overlay(Theme.lineColor)
            Text(s).font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .trailing).padding(.horizontal, 16).padding(.vertical, 10)
        }
    }
}

extension Theme {
    static let bgColor = Color(nsColor: bg)
    static let panelColor = Color(nsColor: panel)
    static let surfaceColor = Color(nsColor: surface)
    static let lineColor = Color(nsColor: line)
    static let fgColor = Color(nsColor: fg)
    static let mutedColor = Color(nsColor: muted)
    static let runningColor = Color(nsColor: running)
}


/// AppKit-Textfeld für Pfade: Cursor am Ende, Tab vervollständigt, Pfeile wählen, ⏎ startet.
struct PathField: NSViewRepresentable {
    @Binding var text: String
    var onTab: () -> Void
    var onSubmit: () -> Void
    var onMove: (Int) -> Void

    func makeNSView(context: Context) -> NSTextField {
        let f = NSTextField()
        f.isBordered = false
        f.drawsBackground = false
        f.focusRingType = .none
        f.font = Theme.font(13)
        f.textColor = Theme.fg
        f.usesSingleLineMode = true      // lange Pfade scrollen horizontal statt umzubrechen
        f.cell?.wraps = false
        f.cell?.isScrollable = true
        f.lineBreakMode = .byClipping
        f.delegate = context.coordinator
        f.stringValue = text
        DispatchQueue.main.async {
            f.window?.makeFirstResponder(f)
            f.currentEditor()?.selectedRange = NSRange(location: f.stringValue.utf16.count, length: 0)
        }
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
