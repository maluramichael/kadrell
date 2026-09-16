import SwiftUI

/// F1 und „Über Kadrell“: Autor, Link, Tastenkürzel. Esc oder ⏎ schließt.
struct AboutView: View {
    private var sections: [(String, [(String, String)])] {
        [
            ("Sessions öffnen", [
                ("Klick", "nur diese Session zeigen · auf Gruppe: alle ihre Sessions"), ("⌘ Klick", "Session dazu oder weg"),
                ("⇧ Klick", "Bereich seit dem letzten Klick dazu"), ("⌘A", "ganze Gruppen der Auswahl, nochmal: zurück"), ("⌘⇧A", "alle Sessions, nochmal: zurück"),
                ("⌘N", "Neue Session"), ("⌘⏎", "Neue Session im Ordner der fokussierten"),
            ] + rows([.openEditor, .renameSession]) + [("Stift", "an Kachel und Baum-Zeile: umbenennen")]),
            ("Navigation", rows([.focusLeft, .focusRight, .focusUp, .focusDown, .nextSession, .prevSession, .lastSession, .previewNext, .previewPrev]
                                + HotkeyAction.allCases.filter { $0.tileIndex != nil } + [.focusSidebar, .focusWorkspace])
                + [("⌘P", "Suche, mit > Kommandos"), ("⌘F", "Im Terminal suchen, ⌘G weiter"), ("⌘⇧F", "In allen Terminals suchen")]),
            ("Kacheln verwalten", rows([.swapLeft, .swapRight, .swapUp, .swapDown, .zoom, .nextLayout, .syncInput, .closeFocused]) + [
                ("Ziehen", "Session oder Gruppe umsortieren, Baum und Kacheln gleich"),
                ("⌘W", "Fokus-Session beenden und entfernen, mit Rückfrage"), ("⌘ + Klick auf X", "Schließen ohne Rückfrage"), ("⌘B", "Baum ein/aus"), ("⌘⇧B", "Alle Gruppen auf- oder zuklappen"),
            ] + rows([.cycleSort])),
            ("Fenster und App", [
                ("⌘M", "Im Dock ablegen"), ("⌘ Mausrad", "Terminal-Schrift aller Sessions größer/kleiner"),
                ("⌘,", "Einstellungen: Startordner, Darstellung, Tastenkürzel"), ("F1", "diese Hilfe"),
            ]),
        ]
    }

    /// Belegbare Kürzel, eine Zeile je Hilfetext; lange Reihen (Kachel 1–9) nur erstes … letztes.
    private func rows(_ actions: [HotkeyAction]) -> [(String, String)] {
        let current = Hotkeys.current
        var out: [(String, String)] = []
        for a in actions where !out.contains(where: { $0.1 == a.helpText }) {
            let ks = actions.filter { $0.helpText == a.helpText }.compactMap { current[$0]?.display }
            guard let first = ks.first, let last = ks.last else { continue }
            out.append((ks.count > 4 ? "\(first) … \(last)" : ks.joined(separator: " "), a.helpText))
        }
        return out
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("Kadrell").font(Theme.ui(18, bold: true)).foregroundStyle(Theme.fgColor)
                Text("v\(Settings.version)").font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
            }
            .padding(.horizontal, 16).padding(.top, 16)
            Text("Claude-Code-Sessions als Terminals: Baum links, rechts als Grid oder Stack. Beim Beenden enden sie, beim nächsten Start geht es mit --resume weiter.").font(Theme.ui(12))
                .foregroundStyle(Theme.mutedColor).padding(.horizontal, 16).padding(.top, 6)
            HStack(spacing: 6) {
                Text("von Michael Malura ·").foregroundStyle(Theme.mutedColor)
                Link("malura.de", destination: URL(string: "https://malura.de")!).foregroundStyle(Theme.runningColor)
            }
            .font(Theme.ui(12)).padding(.horizontal, 16).padding(.top, 4)
            Divider().overlay(Theme.lineColor).padding(.top, 14)
            VStack(alignment: .leading, spacing: 5) {
                ForEach(sections, id: \.0) { title, keys in
                    Text(title.uppercased()).kerning(0.6).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.runningColor)
                        .padding(.leading, 172 * Theme.scale).padding(.top, title == sections.first?.0 ? 0 : 12).padding(.bottom, 2)
                    ForEach(keys, id: \.1) { k, t in
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Text(k).font(Theme.ui(12, bold: true)).foregroundStyle(Theme.fgColor).frame(width: 160 * Theme.scale, alignment: .trailing)
                            Text(t).font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
                        }
                    }
                }
            }
            .padding(16)
            Divider().overlay(Theme.lineColor)
            Text("Esc oder F1 schließen").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .trailing).padding(.horizontal, 16).padding(.vertical, 10)
        }
        .frame(width: 640 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
    }
}
