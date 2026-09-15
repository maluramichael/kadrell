import SwiftUI

enum Settings {
    static let startFolderKey = "startFolder"
    static let columnsKey = "columnsPerGroup"
    /// Kachel-Spalten je Gruppe (i3-artiges Raster), 1 bis 6, Default 2.
    static var columns: Int {
        get { let v = UserDefaults.standard.integer(forKey: columnsKey); return v == 0 ? 2 : max(1, min(6, v)) }
        set { UserDefaults.standard.set(max(1, min(6, newValue)), forKey: columnsKey) }
    }
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
    var columns = Settings.columns
    var compSelected = 0
    var onDone: (() -> Void)?

    func save() {
        let p = startFolder.trimmingCharacters(in: .whitespacesAndNewlines)
        if !p.isEmpty { Settings.startFolder = p }
        Settings.columns = columns
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
            HStack(spacing: 12) {
                Text("Spalten pro Gruppe").font(.custom("JetBrainsMonoNF-Regular", size: 12)).foregroundStyle(Theme.fgColor)
                ForEach(1...6, id: \.self) { n in
                    Text("\(n)").font(.custom("JetBrainsMonoNF-Bold", size: 12))
                        .foregroundStyle(n == model.columns ? Theme.bgColor : Theme.mutedColor)
                        .frame(width: 26, height: 22)
                        .background(n == model.columns ? Theme.runningColor : .clear)
                        .overlay(Rectangle().stroke(Theme.lineColor, lineWidth: 1))
                        .contentShape(Rectangle())
                        .onTapGesture { model.columns = n }
                }
            }
            .padding(14)
            Divider().overlay(Theme.lineColor)
            Text("Tab vervollständigen · ⏎ speichern · Esc abbrechen").font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .trailing).padding(.horizontal, 16).padding(.vertical, 10)
        }
        .frame(width: 640, alignment: .leading)
        .background(Theme.panelColor)
    }
}
