import SwiftUI
import AppKit

/// Nativer Ordnerdialog. Liefert den gewählten Pfad oder nil. `images: true` wählt stattdessen eine Bilddatei.
@MainActor
func chooseFolder(start: String, images: Bool = false, completion: @escaping (String?) -> Void) {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = !images
    panel.canChooseFiles = images
    if images { panel.allowedContentTypes = [.image] }
    panel.canCreateDirectories = true
    panel.allowsMultipleSelection = false
    panel.directoryURL = URL(fileURLWithPath: FolderIndex.normalize(start))
    panel.prompt = String(localized: "Wählen")
    panel.level = NSWindow.Level(rawValue: NSWindow.Level.floating.rawValue + 1)   // über dem Dialog-Overlay
    panel.begin { resp in
        completion(resp == .OK ? panel.url.map { $0.path } : nil)
    }
}

/// Fußleiste der Dialoge: Hinweis links, Aktion rechts als Button (⏎ löst dieselbe Aktion aus).
/// Zeichnet sich nicht an Ort und Stelle, sondern meldet sich an `OverlayScroll`: dort steht sie fest unter
/// dem scrollenden Inhalt, der Button bleibt also auch in langen Dialogen sichtbar.
struct DialogFoot: View {
    let hint: String
    let button: String
    let action: () -> Void

    var body: some View {
        Color.clear.frame(height: 0).preference(key: DialogFootKey.self, value: DialogFootData(hint: hint, button: button, action: action))
    }
}

struct DialogFootData: Equatable {
    let hint: String
    let button: String
    let action: () -> Void
    static func == (a: Self, b: Self) -> Bool { a.hint == b.hint && a.button == b.button }
}

struct DialogFootKey: PreferenceKey {
    static var defaultValue: DialogFootData? { nil }
    static func reduce(value: inout DialogFootData?, nextValue: () -> DialogFootData?) { value = nextValue() ?? value }
}

struct DialogFootBar: View {
    let foot: DialogFootData

    var body: some View {
        VStack(spacing: 0) {
            Divider().overlay(Theme.lineColor)
            HStack(spacing: 12) {
                Text(foot.hint).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                Spacer()
                Button(action: foot.action) {
                    Text("\(foot.button)  ⏎").font(Theme.ui(12, bold: true)).foregroundStyle(Theme.bgColor)
                        .padding(.horizontal, 12).padding(.vertical, 6).background(Theme.runningColor)
                }.buttonStyle(.plain).kbdFocusRing()
            }
            .padding(.horizontal, 16).padding(.vertical, 10)
        }
        .background(Theme.panelColor)
    }
}
