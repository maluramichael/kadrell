import SwiftUI

/// Auswahl vor dem Start einer neuen Session: der zuletzt gewählte Agent oben und markiert, ⏎ startet ihn.
/// Tippen filtert unscharf über die Namen, ↑↓ wählt. Esc und ⏎ kommen vom OverlayPanel, deshalb liegt `start()` am Modell.
@Observable
@MainActor
final class AgentPickModel {
    struct Option: Identifiable, Equatable {
        let kind: AgentKind
        /// Rechts in der Zeile: Startflags bei Claude, das lokale Modell bei Pi.
        let detail: String
        var id: String { kind.rawValue }
    }

    /// Ordner, in dem die Session startet, als Zeile unter dem Titel.
    let folder: String
    private let options: [Option]
    var query = "" { didSet { if query != oldValue { update() } } }
    private(set) var list: [Option]
    var selected = 0
    @ObservationIgnored var onPick: ((AgentKind) -> Void)?

    init(folder: String, options: [Option], last: AgentKind) {
        self.folder = folder
        self.options = Self.order(options, last: last)
        list = self.options
    }

    /// Zuletzt gewählter Agent zuerst, die übrigen in ihrer festen Reihenfolge.
    static func order(_ options: [Option], last: AgentKind) -> [Option] {
        options.filter { $0.kind == last } + options.filter { $0.kind != last }
    }

    /// Nur über den Namen: die Startflags („bypassPermissions“) träfen sonst fast jede Buchstabenfolge.
    private func update() {
        let q = query.trimmingCharacters(in: .whitespaces)
        list = q.isEmpty ? options : options.filter { PaletteWindow.fuzzy(q, $0.kind.displayName) }
        selected = 0
    }

    func move(_ d: Int) { selected = max(0, min(max(list.count - 1, 0), selected + d)) }

    func pick(_ i: Int) {
        guard list.indices.contains(i) else { return }
        selected = i
        start()
    }

    func start() {
        guard list.indices.contains(selected) else { NSSound.beep(); return }
        onPick?(list[selected].kind)
    }
}

/// Kleines Overlay „Starten mit“ zwischen Ordnerwahl und Start, für ⌘N, ⌘⏎, Kontextmenüs, Baum und Schnellstart.
struct AgentPickView: View {
    @Bindable var model: AgentPickModel

    var body: some View {
        VStack(spacing: 0) {
            Text("STARTEN MIT").dialogTitle()
            Text(Theme.shortPath(model.folder)).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1).truncationMode(.head)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.top, 4)
            PathField(text: $model.query, placeholder: String(localized: "Tippen filtert", bundle: Bundle.app),
                      onTab: {}, onSubmit: model.start, onMove: model.move)
                .padding(14)
            SeamLine()
            VStack(spacing: 0) {
                if model.list.isEmpty {
                    Text("Nichts gefunden.").foregroundStyle(Theme.mutedColor)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(16)
                }
                ForEach(Array(model.list.enumerated()), id: \.element.id) { i, o in
                    row(o, on: i == model.selected).onTapGesture { model.pick(i) }
                }
            }
            DialogFoot(hint: String(localized: "⏎ starten · ↑↓ wählt · Esc abbrechen", bundle: Bundle.app),
                       button: String(localized: "Starten", bundle: Bundle.app)) { model.start() }
        }
        .font(Theme.ui(12)).foregroundStyle(Theme.fgColor).overlayWidth(440)
    }

    private static let rowHeight: CGFloat = 36

    private func row(_ o: AgentPickModel.Option, on: Bool) -> some View {
        HStack(spacing: 10) {
            Text(o.kind.displayName).foregroundStyle(Theme.fgColor).lineLimit(1)
            Spacer()
            Text(o.detail).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1).truncationMode(.tail)
        }
        .padding(.horizontal, 16)
        .frame(height: Self.rowHeight * Theme.scale)
        .instrumentRow(selected: on)
    }
}
