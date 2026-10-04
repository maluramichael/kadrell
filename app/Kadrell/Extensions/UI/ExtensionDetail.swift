import SwiftUI

/// Unter der Liste, für die gewählte Extension: Knöpfe, Einstellungsformular aus `manifest.settings` und Log.
struct ExtensionDetail: View {
    @ObservedObject var model: ExtensionsModel
    let ext: FoundExtension

    private var settings: [ExtensionManifest.Setting] { ext.manifest?.settings ?? [] }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 16) {
                Text(ext.name.uppercased()).kerning(0.6).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.runningColor)
                Spacer()
                button("Neu laden  R") { model.reload(ext) }.disabled(!model.canToggle(ext) || !model.isEnabled(ext.name))
                button("Ordner öffnen") { model.openFolder(ext) }
                button(model.showLog ? "Log ausblenden  L" : "Log  L") { model.showLog.toggle() }
            }
            .padding(.horizontal, 16).padding(.top, 12).padding(.bottom, 8)
            if !settings.isEmpty { form }
            if model.showLog { log }
        }
        .font(Theme.ui(12))
        .padding(.bottom, 12)
    }

    private func button(_ title: LocalizedStringKey, action: @escaping () -> Void) -> some View {
        Button(action: action) { Text(title).foregroundStyle(Theme.fgColor) }.buttonStyle(.plain).kbdFocusRing()
    }

    private var form: some View {
        VStack(spacing: 0) {
            ForEach(Array(settings.enumerated()), id: \.element.key) { i, s in
                if i > 0 { Divider().overlay(Theme.lineColor) }
                HStack(spacing: 12) {
                    Text(s.label).foregroundStyle(Theme.fgColor).lineLimit(1)
                    Spacer(minLength: 12)
                    control(s).frame(width: 300 * Theme.scale, alignment: .leading)
                }
                .padding(.horizontal, 10).padding(.vertical, 5)
            }
        }
        .overlay(Rectangle().stroke(Theme.lineColor, lineWidth: 1))
        .padding(.horizontal, 16)
    }

    @ViewBuilder
    private func control(_ s: ExtensionManifest.Setting) -> some View {
        let save: (JSONValue) -> Void = { v in Task { await model.save(s.key, v, in: ext) } }
        switch s.type {
        case "bool":
            Toggle("", isOn: Binding(get: { model.stored[s.key]?.bool ?? false }, set: { save(.bool($0)) }))
                .labelsHidden().toggleStyle(.switch).controlSize(.mini).tint(Theme.runningColor)
        case "secret":
            HStack(spacing: 8) {
                (model.secretsSet.contains(s.key) ? Text("gesetzt") : Text("nicht gesetzt")).foregroundStyle(Theme.mutedColor).lineLimit(1)
                SettingField(initial: "", secure: true) { save(.string($0)) }
            }
        default:
            SettingField(initial: model.stored[s.key]?.string ?? "", secure: false) { save(.string($0)) }
        }
    }

    private var log: some View {
        let lines = model.manager.log(ext.name)
        return ScrollView(.vertical) {
            Text(lines.isEmpty ? String(localized: "Noch keine Logzeilen.", bundle: Bundle.app) : lines.joined(separator: "\n"))
                .font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading).padding(8)
        }
        .defaultScrollAnchor(.bottom)
        .frame(height: 180 * Theme.scale)
        .background(Theme.bgColor)
        .padding(.horizontal, 16).padding(.top, 10)
    }
}

/// Textfeld, das erst beim Verlassen oder mit ⏎ speichert: jede gespeicherte Änderung startet die Extension neu,
/// pro Tastendruck wäre das ein Neustart je Zeichen. Ein Geheimnis zeigt nie seinen Wert, das Feld ersetzt es nur.
private struct SettingField: View {
    let initial: String
    let secure: Bool
    let commit: (String) -> Void
    @State private var draft: String?
    @FocusState private var focused: Bool

    var body: some View {
        let text = Binding(get: { draft ?? initial }, set: { draft = $0 })
        SwiftUI.Group {
            if secure { SecureField("neuer Wert", text: text) } else { TextField("", text: text) }
        }
        .textFieldStyle(.plain).foregroundStyle(Theme.fgColor).focused($focused)
        .padding(.horizontal, 8).padding(.vertical, 3).background(Theme.bgColor)
        .onSubmit(apply)
        .onChange(of: focused) { if !focused { apply() } }
        .onDisappear(perform: apply)
    }

    /// Nur Geändertes; ein leeres Geheimnisfeld heißt „nicht angefasst“, nicht „löschen“.
    private func apply() {
        guard let d = draft, d != initial, !(secure && d.isEmpty) else { return }
        draft = nil
        commit(d)
    }
}
