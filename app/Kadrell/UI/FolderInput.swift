import SwiftUI
import AppKit

// Einheitliche Pfadeingabe: tippen, Tab vervollständigt, Pfeile wählen, ⏎ bestätigt, Button öffnet den Ordnerdialog.

/// Unterordner, die zum getippten Pfad passen (nur Verzeichnisse, keine versteckten).
func folderCompletions(for typed: String) -> [String] {
    let path = NewSessionModel.expand(typed)
    let dir: String, prefix: String
    if path.hasSuffix("/") { dir = path; prefix = "" } else {
        let u = URL(fileURLWithPath: path)
        dir = u.deletingLastPathComponent().path; prefix = u.lastPathComponent
    }
    guard let names = try? FileManager.default.contentsOfDirectory(atPath: dir) else { return [] }
    return names.filter { !$0.hasPrefix(".") && $0.lowercased().hasPrefix(prefix.lowercased()) }
        .filter { var d: ObjCBool = false; return FileManager.default.fileExists(atPath: dir + "/" + $0, isDirectory: &d) && d.boolValue }
        .sorted().prefix(8).map { (dir.hasSuffix("/") ? dir : dir + "/") + $0 + "/" }
}

/// Nativer Ordnerdialog. Liefert den gewählten Pfad mit Slash oder nil.
@MainActor
func chooseFolder(start: String, completion: @escaping (String?) -> Void) {
    let panel = NSOpenPanel()
    panel.canChooseDirectories = true
    panel.canChooseFiles = false
    panel.canCreateDirectories = true
    panel.allowsMultipleSelection = false
    panel.directoryURL = URL(fileURLWithPath: NewSessionModel.expand(start))
    panel.prompt = "Wählen"
    panel.begin { resp in
        completion(resp == .OK ? panel.url.map { $0.path + "/" } : nil)
    }
}

/// Pfadfeld mit Vorschlagsliste und Ordner-Button, für ⌘N und Einstellungen.
struct FolderInput: View {
    @Binding var path: String
    @Binding var selected: Int
    var onSubmit: () -> Void
    var completions: [String] { folderCompletions(for: path) }

    var body: some View {
        HStack(spacing: 10) {
            PathField(text: $path, onTab: complete, onSubmit: complete,   // ⏎ wie Tab; starten nur per ⌘⏎ oder Button
                      onMove: { d in selected = max(0, min(max(completions.count - 1, 0), selected + d)) })
                .frame(height: 20)
            Button("Ordner wählen …") {
                chooseFolder(start: path) { if let p = $0 { path = p } }
            }
            .buttonStyle(.plain).font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
            .padding(.horizontal, 10).padding(.vertical, 4).overlay(Rectangle().stroke(Theme.lineColor, lineWidth: 1))
        }
        .padding(14)
        .onChange(of: path) { _, _ in selected = 0 }
        let comps = completions
        if !comps.isEmpty {
            Divider().overlay(Theme.lineColor)
            VStack(spacing: 0) {
                ForEach(Array(comps.enumerated()), id: \.element) { i, c in
                    Text(Theme.shortPath(c)).font(.custom("JetBrainsMonoNF-Regular", size: 12))
                        .foregroundStyle(i == selected ? Theme.fgColor : Theme.mutedColor)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 6).padding(.horizontal, 16)
                        .background(i == selected ? Theme.surfaceColor : .clear)
                        .overlay(alignment: .leading) { if i == selected { Rectangle().fill(Theme.runningColor).frame(width: 3) } }
                        .contentShape(Rectangle())
                        .onTapGesture { selected = i; complete() }
                }
            }
        }
    }

    private func complete() {
        let c = completions
        guard !c.isEmpty else { return }
        path = c[min(selected, c.count - 1)]
        selected = 0
    }
}


/// Fußleiste der Dialoge: Hinweis links, Aktion rechts als Button (⌘⏎ löst dieselbe Aktion aus).
struct DialogFoot: View {
    let hint: String
    let button: String
    let action: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            Divider().overlay(Theme.lineColor)
            HStack(spacing: 12) {
                Text(hint).font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
                Spacer()
                Button(action: action) {
                    Text("\(button)  ⌘⏎").font(.custom("JetBrainsMonoNF-Bold", size: 12)).foregroundStyle(Theme.bgColor)
                        .padding(.horizontal, 12).padding(.vertical, 6).background(Theme.runningColor)
                }.buttonStyle(.plain)
            }
            .padding(.horizontal, 16).padding(.vertical, 10)
        }
    }
}
