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
    var compSelected = 0
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

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("EINSTELLUNGEN").font(.custom("JetBrainsMonoNF-Regular", size: 11)).kerning(0.6).foregroundStyle(Theme.mutedColor)
                .padding(.horizontal, 16).padding(.top, 12)
            Text("Startordner für ⌘N").font(.custom("JetBrainsMonoNF-Regular", size: 12)).foregroundStyle(Theme.fgColor)
                .padding(.horizontal, 16).padding(.top, 14)
            FolderInput(path: $model.startFolder, selected: $model.compSelected) { model.save() }
            Divider().overlay(Theme.lineColor)
            Text("Tab vervollständigen · ⏎ speichern · Esc abbrechen").font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .trailing).padding(.horizontal, 16).padding(.vertical, 10)
        }
        .frame(width: 640, alignment: .leading)
        .background(Theme.panelColor)
    }
}
