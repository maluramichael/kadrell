import SwiftUI

enum Settings {
    static let startFolderKey = "startFolder"
    /// Startordner für ⌘N, Default Home. Immer mit abschließendem Slash.
    static var startFolder: String {
        get {
            var p = UserDefaults.standard.string(forKey: startFolderKey) ?? NSHomeDirectory()
            if p.hasPrefix("~") { p = NSHomeDirectory() + p.dropFirst() }
            return p.hasSuffix("/") ? p : p + "/"
        }
        set { UserDefaults.standard.set(newValue, forKey: startFolderKey) }
    }
}

@Observable
@MainActor
final class SettingsModel {
    var startFolder = Settings.startFolder
    var onDone: (() -> Void)?

    func save() {
        let p = startFolder.trimmingCharacters(in: .whitespacesAndNewlines)
        if !p.isEmpty { Settings.startFolder = p }
        onDone?()
    }
}

/// ⌘, Einstellungen. ⏎ speichert, Esc bricht ab.
struct SettingsView: View {
    @Bindable var model: SettingsModel
    @FocusState private var focused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("EINSTELLUNGEN").font(.custom("JetBrainsMonoNF-Regular", size: 11)).kerning(0.6).foregroundStyle(Theme.mutedColor)
                .padding(.horizontal, 16).padding(.top, 12)
            Text("Startordner für ⌘N").font(.custom("JetBrainsMonoNF-Regular", size: 12)).foregroundStyle(Theme.fgColor)
                .padding(.horizontal, 16).padding(.top, 14)
            TextField("~/", text: $model.startFolder).textFieldStyle(.plain).font(.custom("JetBrainsMonoNF-Regular", size: 13))
                .padding(.horizontal, 16).padding(.vertical, 10).focused($focused).onSubmit { model.save() }
            Divider().overlay(Theme.lineColor)
            Text("⏎ speichern · Esc abbrechen").font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .trailing).padding(.horizontal, 16).padding(.vertical, 10)
        }
        .frame(width: 560, alignment: .leading)
        .background(Theme.panelColor)
        .onAppear { focused = true }
    }
}
