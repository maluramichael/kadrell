import SwiftUI

/// F4: alle gefundenen Extensions mit Zustand, an/aus, Neu laden, Ordner, Log und Einstellungen.
/// Tasten laufen über `ExtensionsModel.handleKey` (aus dem Tasten-Monitor der App), Esc und F4 schließen.
struct ExtensionsView: View {
    static let docs = URL(string: "https://kadrell.malura.de/extensions")!

    @ObservedObject var model: ExtensionsModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().overlay(Theme.lineColor).padding(.top, 12)
            if model.items.isEmpty {
                Text("Noch keine Extensions. Ordner hineinlegen oder per git clone holen, sie erscheinen sofort.")
                    .font(Theme.ui(12)).foregroundStyle(Theme.mutedColor).padding(16)
            }
            ForEach(Array(model.items.enumerated()), id: \.element.name) { i, ext in
                if i > 0 { Divider().overlay(Theme.lineColor) }
                ExtensionRow(model: model, ext: ext, selected: i == model.selected)
                    .contentShape(Rectangle())
                    .onTapGesture { model.click(i) }
            }
            if let ext = model.current {
                Divider().overlay(Theme.lineColor)
                ExtensionDetail(model: model, ext: ext).id(ext.name)
            }
            Divider().overlay(Theme.lineColor)
            discover
            Divider().overlay(Theme.lineColor)
            Text("↑↓ wählen · Leertaste an/aus · R neu laden · L Log · Esc oder F4 schließen").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .trailing).padding(.horizontal, 16).padding(.vertical, 10)
        }
        .frame(width: 760 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
        .task { await model.load() }
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline, spacing: 14) {
            Text(verbatim: "EXTENSIONS").kerning(0.6).foregroundStyle(Theme.mutedColor)
            Spacer()
            Button { model.openCatalog() } label: { Text("Extensions-Ordner öffnen").foregroundStyle(Theme.fgColor) }
                .buttonStyle(.plain).kbdFocusRing()
            Link("Doku", destination: Self.docs).foregroundStyle(Theme.runningColor)
        }
        .font(Theme.ui(11)).padding(.horizontal, 16).padding(.top, 12)
    }

    /// Marktplatz: ein Suchfeld sucht GitHub-Repos mit Topic `kadrell-extension`, jeder Treffer lässt sich installieren.
    private var discover: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("ENTDECKEN").kerning(0.6).foregroundStyle(Theme.mutedColor)
                Spacer()
                if model.searching { ProgressView().controlSize(.small) }
            }.font(Theme.ui(11))
            HStack(spacing: 10) {
                TextField("Auf GitHub suchen (Topic kadrell-extension)", text: $model.query)
                    .textFieldStyle(.plain).foregroundStyle(Theme.fgColor)
                    .onSubmit { Task { await model.searchStore() } }
                Button { Task { await model.searchStore() } } label: { Text("Suchen").foregroundStyle(Theme.runningColor) }
                    .buttonStyle(.plain).kbdFocusRing()
            }
            if let e = model.storeError {
                Text(e).foregroundStyle(Theme.errorColor).fixedSize(horizontal: false, vertical: true)
            }
            ForEach(model.results) { StoreRow(model: model, remote: $0) }
        }
        .font(Theme.ui(12)).padding(.horizontal, 16).padding(.vertical, 10)
    }
}

/// Ein Treffer der Marktplatz-Suche: Repo, Sterne, Beschreibung und ein Knopf zum Installieren oder Aktualisieren.
private struct StoreRow: View {
    @ObservedObject var model: ExtensionsModel
    let remote: RemoteExtension

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 8) {
                    Link(remote.fullName, destination: remote.htmlURL).font(Theme.ui(12, bold: true)).foregroundStyle(Theme.runningColor)
                    Text(verbatim: "★ \(remote.stars)").foregroundStyle(Theme.mutedColor)
                }
                if !remote.description.isEmpty {
                    Text(verbatim: remote.description).foregroundStyle(Theme.mutedColor).fixedSize(horizontal: false, vertical: true)
                }
            }
            Spacer(minLength: 12)
            if model.installing.contains(remote.id) {
                ProgressView().controlSize(.mini).scaleEffect(0.7)
            } else {
                Button { Task { await model.install(remote) } } label: {
                    Text(model.isInstalled(remote) ? "Aktualisieren" : "Installieren").foregroundStyle(Theme.fgColor)
                }.buttonStyle(.plain).kbdFocusRing()
            }
        }
        .font(Theme.ui(12)).frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Eine Zeile der Liste: Zustandspunkt, Name, Version, Zustand oder Fehlergrund, Schalter, Beschreibung, Rechte.
private struct ExtensionRow: View {
    @ObservedObject var model: ExtensionsModel
    let ext: FoundExtension
    let selected: Bool

    private var state: ExtensionState { model.manager.state(ext.name) }

    private var dot: Color {
        let c: ThemeColor = switch state {
        case .running: .ok
        case .starting, .reloading: .warn
        case .failed: .err
        case .off: ext.problem == nil ? .muted : .err
        }
        return Color(nsColor: c.nsColor)
    }

    /// Ein Problem im Ordner (kaputtes Manifest, fremde Rechte) steht vor dem Zustand: die Extension läuft dann gar nicht.
    private var status: (text: String, failed: Bool) {
        if let problem = ext.problem { return (problem, true) }
        if case .failed = state { return (state.label, true) }
        return (state.label, false)
    }

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Circle().fill(dot).frame(width: 8 * Theme.scale, height: 8 * Theme.scale)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(ext.name).font(Theme.ui(12, bold: true)).foregroundStyle(Theme.fgColor)
                    Text(ext.manifest?.version ?? "").foregroundStyle(Theme.mutedColor)
                    Spacer(minLength: 12)
                    Toggle("", isOn: Binding(get: { model.isEnabled(ext.name) }, set: { _ in model.toggle(ext) }))
                        .labelsHidden().toggleStyle(.switch).controlSize(.mini).tint(Theme.runningColor)
                        .disabled(!model.canToggle(ext))
                }
                Text(status.text).foregroundStyle(status.failed ? Theme.errorColor : Theme.mutedColor).fixedSize(horizontal: false, vertical: true)
                if let d = ext.manifest?.description, !d.isEmpty { Text(d).foregroundStyle(Theme.fgColor).fixedSize(horizontal: false, vertical: true) }
                if let p = ext.manifest?.permissions, !p.isEmpty {
                    Text("Rechte: \(p.joined(separator: ", "))").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                }
            }
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 16).padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(selected ? Theme.surfaceColor : .clear)
        .overlay(alignment: .leading) { if selected { Theme.runningColor.frame(width: 2) } }
    }
}
