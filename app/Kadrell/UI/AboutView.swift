import SwiftUI

/// F1 und „Über Kadrell“: Autor, Link, Tastenkürzel. Esc oder ⏎ schließt.
struct AboutView: View {
    private let keys: [(String, String)] = [
        ("Klick", "nur diese Session zeigen · auf Gruppe: alle ihre Sessions"), ("⌘ Klick", "Session dazu oder weg"),
        ("⇧ Klick", "Bereich seit dem letzten Klick dazu"), ("⌘A", "alle Sessions öffnen"),
        ("⌘1 / ⌘2", "Grid / Stack"), ("⌘⌥ ← ↑ → ↓", "Fokus bewegen · im Stack auf- und zuklappen"),
        ("⌘⇧ ⏎", "Fokus-Kachel allein (zen)"), ("⌘Esc", "Fokus-Kachel schließen (Esc selbst geht an Claude)"),
        ("⌘N", "Neue Session"), ("⌘⏎", "Neue Session in der Gruppe der Fokus-Kachel"),("⌘P", "Suche, mit > Kommandos"), ("⌘B", "Baum ein/aus"), ("⌘W", "Fokussierte Session stoppen"),
        ("⌘ + Klick auf X", "Schließen ohne Rückfrage"), ("⌘,", "Einstellungen: Startordner"), ("F1", "diese Hilfe"),
    ]
    private var version: String {
        (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "0"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Text("Kadrell").font(.custom("JetBrainsMonoNF-Bold", size: 18)).foregroundStyle(Theme.fgColor)
                Text("v\(version)").font(.custom("JetBrainsMonoNF-Regular", size: 12)).foregroundStyle(Theme.mutedColor)
            }
            .padding(.horizontal, 16).padding(.top, 16)
            Text("Alle Claude-Code-Hintergrund-Sessions: Baum links, Terminals rechts als Grid oder Stack.").font(.custom("JetBrainsMonoNF-Regular", size: 12))
                .foregroundStyle(Theme.mutedColor).padding(.horizontal, 16).padding(.top, 6)
            HStack(spacing: 6) {
                Text("von Michael Malura ·").foregroundStyle(Theme.mutedColor)
                Link("malura.de", destination: URL(string: "https://malura.de")!).foregroundStyle(Theme.runningColor)
            }
            .font(.custom("JetBrainsMonoNF-Regular", size: 12)).padding(.horizontal, 16).padding(.top, 4)
            Divider().overlay(Theme.lineColor).padding(.top, 14)
            VStack(alignment: .leading, spacing: 5) {
                ForEach(keys, id: \.0) { k, t in
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        Text(k).font(.custom("JetBrainsMonoNF-Bold", size: 12)).foregroundStyle(Theme.fgColor).frame(width: 130, alignment: .trailing)
                        Text(t).font(.custom("JetBrainsMonoNF-Regular", size: 12)).foregroundStyle(Theme.mutedColor)
                    }
                }
            }
            .padding(16)
            Divider().overlay(Theme.lineColor)
            Text("Esc oder F1 schließen").font(.custom("JetBrainsMonoNF-Regular", size: 11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .trailing).padding(.horizontal, 16).padding(.vertical, 10)
        }
        .frame(width: 560, alignment: .leading)
        .background(Theme.panelColor)
    }
}
