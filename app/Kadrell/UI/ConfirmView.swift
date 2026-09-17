import SwiftUI

/// Rückfrage im App-Design (statt NSAlert). Nicht-destruktive bestätigt blankes ⏎, destruktive nur ⌘⏎. Esc bricht ab.
struct ConfirmView: View {
    let title: String
    let info: String
    let button: String
    let destructive: Bool
    /// Reine Mitteilung ohne echte Alternative (Fehlermeldung, Erfolg): kein „Abbrechen“ daneben, das dasselbe täte wie der Button.
    var infoOnly: Bool = false
    /// Gesetzt: Häkchen „Nicht mehr fragen“. Schreibt sofort (⏎ läuft am Button vorbei), Abbrechen stellt zurück.
    var ask: Settings.Ask? = nil
    let onConfirm: () -> Void
    let onCancel: () -> Void
    @State private var dontAsk = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title).font(Theme.ui(14, bold: true)).foregroundStyle(Theme.fgColor)
                .padding(.horizontal, 16).padding(.top, 16)
            if !info.isEmpty {
                Text(info).font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
                    .padding(.horizontal, 16).padding(.top, 8)
            }
            HStack(spacing: 8) {
                if ask != nil {
                    Toggle(isOn: Binding(get: { dontAsk }, set: { dontAsk = $0; ask?.enabled = !$0 })) {
                        Text("Nicht mehr fragen").font(Theme.ui(12)).foregroundStyle(dontAsk ? Theme.fgColor : Theme.mutedColor)
                    }
                    .toggleStyle(.checkbox)
                    .help("Wieder einschalten: Einstellungen (⌘,) › Rückfragen")
                }
                Spacer()
                if !infoOnly {
                    Button(action: onCancel) {
                        Text("Abbrechen").font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
                            .padding(.horizontal, 12).padding(.vertical, 6).overlay(Rectangle().stroke(Theme.lineColor, lineWidth: 1))
                    }.buttonStyle(.plain).kbdFocusRing()
                }
                Button(action: onConfirm) {
                    Text(destructive ? "\(button)  ⌘⏎" : button).font(Theme.ui(12, bold: true)).foregroundStyle(Theme.bgColor)
                        .padding(.horizontal, 12).padding(.vertical, 6).background(destructive ? Theme.errorColor : Theme.runningColor)
                }.buttonStyle(.plain).kbdFocusRing()
            }
            .padding(16)
        }
        .frame(width: 520 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
    }
}
