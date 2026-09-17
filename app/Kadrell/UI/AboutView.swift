import SwiftUI

/// F1 und „Über Kadrell“: Autor, Link, Tastenkürzel. Esc oder ⏎ schließt.
struct AboutView: View {
    private var sections: [(String, [(String, String)])] {
        [
            (String(localized: "Sessions öffnen"), [
                (String(localized: "Klick"), String(localized: "nur diese Session zeigen · auf Gruppe: alle ihre Sessions")), (String(localized: "⌘ Klick"), String(localized: "Session dazu oder weg")),
                (String(localized: "⇧ Klick"), String(localized: "Bereich seit dem letzten Klick dazu")), ("⌘A", String(localized: "ganze Gruppen der Auswahl, nochmal: zurück")), ("⌘⇧A", String(localized: "alle Sessions, nochmal: zurück")),
                ("⌘N", String(localized: "Neue Session: Projekt suchen, ~/d/p abkürzen, ⌘O Finder, Ordner hineinziehen")), ("⌘⏎", String(localized: "Neue Session im Ordner der fokussierten")), ("⌘T", String(localized: "Terminal ohne Claude im Ordner der fokussierten")),
                (String(localized: "+ an Gruppe"), String(localized: "neue Session in der Gruppe · mit ⌘: Terminal ohne Claude")),
            ] + rows([.openEditor, .renameSession]) + [(String(localized: "Stift"), String(localized: "an Kachel und Baum-Zeile: umbenennen"))]),
            (String(localized: "Navigation"), rows([.focusLeft, .focusRight, .focusUp, .focusDown, .nextSession, .prevSession, .lastSession, .previewNext, .previewPrev, .nextWaiting]
                                + HotkeyAction.allCases.filter { $0.tileIndex != nil } + [.focusSidebar, .focusWorkspace])
                + [("⌘P", String(localized: "Suche, mit > Kommandos")), ("⌘F", String(localized: "Im Terminal suchen, ⌘G weiter")), ("⌘⇧F", String(localized: "In allen Terminals suchen"))]),
            (String(localized: "Kacheln verwalten"), rows([.swapLeft, .swapRight, .swapUp, .swapDown, .resizeLeft, .resizeRight, .resizeUp, .resizeDown, .splitRight, .splitDown, .zoom, .nextLayout, .syncInput, .closeFocused]) + [
                (String(localized: "Trennlinie ziehen"), String(localized: "Größen des Layouts ändern, Doppelklick verteilt gleich · Terminals füllen die Felder der Reihe nach")),
                (String(localized: "‹ SP › in der Leiste"), String(localized: "Spalten im Grid, unter 1 wieder automatisch")),
                (String(localized: "TEILT in der Leiste"), String(localized: "Frei: Teilung an der Fokus-Kachel, Klick schaltet → ↓ AUTO (längere Seite)")),
                (String(localized: "Seitlich wischen"), String(localized: "Scrollen: Spalten verschieben (Maus: ⇧ + Rad), der Fokus rückt von selbst ins Bild")),
                (String(localized: "Ziehen"), String(localized: "Session oder Gruppe umsortieren, Baum und Kacheln gleich")),
                ("⌘W", String(localized: "Fokus-Session beenden und entfernen, mit Rückfrage")), (String(localized: "⌥ + Klick auf X"), String(localized: "Schließen ohne Rückfrage")), ("⌘B", String(localized: "Baum ein/aus")), ("⌘⇧B", String(localized: "Alle Gruppen auf- oder zuklappen")),
            ] + rows([.cycleSort])),
            (String(localized: "Fenster und App"), [
                (String(localized: "Rundes X"), String(localized: "Fenster nur verstecken, Sessions laufen weiter")), (String(localized: "Menüleisten-Icon / Dock"), String(localized: "Fenster zurückholen")),
                ("⌘⇧T", String(localized: "Neues Fenster: eigene Auswahl und eigenes Layout, dieselben Sessions")), ("⌘⇧W", String(localized: "Fenster schließen, Sessions laufen weiter")),
                ("⌘M", String(localized: "Im Dock ablegen")), (String(localized: "⌘ Mausrad"), String(localized: "Terminal-Schrift aller Sessions größer/kleiner")),
                ("⌘,", String(localized: "Einstellungen: Projektordner, Darstellung, Tastenkürzel")), ("F1", String(localized: "diese Hilfe")),
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
