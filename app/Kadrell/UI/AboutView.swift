import SwiftUI

/// F1 und „Über Kadrell“: Autor, Link, Tastenkürzel. Esc oder ⏎ schließt.
struct AboutView: View {
    private var sections: [(String, [(String, String)])] {
        [
            (String(localized: "Sessions öffnen", bundle: Bundle.app), [
                (String(localized: "Klick", bundle: Bundle.app), String(localized: "nur diese Session zeigen · auf Gruppe: alle ihre Sessions", bundle: Bundle.app)), (String(localized: "⌘ Klick", bundle: Bundle.app), String(localized: "Session dazu oder weg", bundle: Bundle.app)),
                (String(localized: "⇧ Klick", bundle: Bundle.app), String(localized: "Bereich seit dem letzten Klick dazu", bundle: Bundle.app)), ("⌘A", String(localized: "ganze Gruppen der Auswahl, nochmal: zurück", bundle: Bundle.app)), ("⌘⇧A", String(localized: "alle Sessions, nochmal: zurück", bundle: Bundle.app)),
                ("⌘N", String(localized: "Neue Session: Projekt suchen, ~/d/p abkürzen, ⌘O Finder, Ordner hineinziehen", bundle: Bundle.app)), ("⌘⏎", String(localized: "Neue Session im Ordner der fokussierten", bundle: Bundle.app)), ("⌘T", String(localized: "Terminal ohne Claude im Ordner der fokussierten", bundle: Bundle.app)),
                (String(localized: "+ an Gruppe", bundle: Bundle.app), String(localized: "neue Session in der Gruppe · mit ⌘: Terminal ohne Claude", bundle: Bundle.app)),
            ] + rows([.openEditor, .renameSession]) + [(String(localized: "Stift", bundle: Bundle.app), String(localized: "an Kachel und Baum-Zeile: umbenennen", bundle: Bundle.app))]),
            (String(localized: "Navigation", bundle: Bundle.app), rows([.focusLeft, .focusRight, .focusUp, .focusDown, .nextSession, .prevSession, .lastSession, .previewNext, .previewPrev, .nextWaiting]
                                + HotkeyAction.allCases.filter { $0.tileIndex != nil } + [.focusSidebar, .focusWorkspace])
                + [("⌘P", String(localized: "Suche, mit > Kommandos", bundle: Bundle.app)), ("⌘F", String(localized: "Im Terminal suchen, ⌘G weiter", bundle: Bundle.app)), ("⌘⇧F", String(localized: "In allen Terminals suchen", bundle: Bundle.app))]),
            (String(localized: "Kacheln verwalten", bundle: Bundle.app), rows([.swapLeft, .swapRight, .swapUp, .swapDown, .resizeLeft, .resizeRight, .resizeUp, .resizeDown, .splitRight, .splitDown, .zoom, .nextLayout, .syncInput, .closeFocused]) + [
                (String(localized: "Trennlinie ziehen", bundle: Bundle.app), String(localized: "Größen des Layouts ändern, Doppelklick verteilt gleich · Terminals füllen die Felder der Reihe nach", bundle: Bundle.app)),
                (String(localized: "‹ SP › in der Leiste", bundle: Bundle.app), String(localized: "Spalten im Grid, unter 1 wieder automatisch", bundle: Bundle.app)),
                (String(localized: "TEILT in der Leiste", bundle: Bundle.app), String(localized: "Frei: Teilung an der Fokus-Kachel, Klick schaltet → ↓ AUTO (längere Seite)", bundle: Bundle.app)),
                (String(localized: "Seitlich wischen", bundle: Bundle.app), String(localized: "Scrollen: Spalten verschieben (Maus: ⇧ + Rad), der Fokus rückt von selbst ins Bild", bundle: Bundle.app)),
                (String(localized: "Ziehen", bundle: Bundle.app), String(localized: "Session oder Gruppe umsortieren, Baum und Kacheln gleich", bundle: Bundle.app)),
                ("⌘W", String(localized: "Fokus-Session beenden und entfernen, mit Rückfrage", bundle: Bundle.app)), (String(localized: "⌥ + Klick auf X", bundle: Bundle.app), String(localized: "Schließen ohne Rückfrage", bundle: Bundle.app)), (String(localized: "Mittelklick im Baum", bundle: Bundle.app), String(localized: "Session schließen · auf Gruppe: die ganze Gruppe · mit ⌥ ohne Rückfrage", bundle: Bundle.app)), ("⌘B", String(localized: "Baum ein/aus", bundle: Bundle.app)), ("⌘⇧B", String(localized: "Alle Gruppen auf- oder zuklappen", bundle: Bundle.app)),
            ] + rows([.cycleSort])),
            (String(localized: "Fenster und App", bundle: Bundle.app), [
                (String(localized: "Rundes X", bundle: Bundle.app), String(localized: "Fenster nur verstecken, Sessions laufen weiter", bundle: Bundle.app)), (String(localized: "Menüleisten-Icon / Dock", bundle: Bundle.app), String(localized: "Fenster zurückholen", bundle: Bundle.app)),
                ("⌘⇧T", String(localized: "Neues Fenster: eigene Auswahl und eigenes Layout, dieselben Sessions", bundle: Bundle.app)), ("⌘⇧W", String(localized: "Fenster schließen, Sessions laufen weiter", bundle: Bundle.app)),
                ("⌘M", String(localized: "Im Dock ablegen", bundle: Bundle.app)), (String(localized: "⌘ Mausrad", bundle: Bundle.app), String(localized: "Terminal-Schrift aller Sessions größer/kleiner", bundle: Bundle.app)),
                ("⌘,", String(localized: "Einstellungen: Projektordner, Darstellung, Tastenkürzel", bundle: Bundle.app)), ("F1", String(localized: "diese Hilfe", bundle: Bundle.app)),
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
