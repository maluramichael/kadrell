import SwiftUI

/// F3: die Lebenszeit-Zähler des Profils (siehe `Stats`). Esc, ⏎ oder F3 schließt.
struct StatsView: View {
    private struct Row: Identifiable {
        let id: String
        let value: Int
        let label: LocalizedStringKey
        var date: Date? = nil
    }

    private var rows: [Row] {
        [
            Row(id: "messages", value: Stats.count(.messages), label: "Nachrichten geschickt"),
            Row(id: "sessions", value: Stats.count(.sessions), label: "Sessions geöffnet"),
            Row(id: "terminals", value: Stats.count(.terminals), label: "Terminals geöffnet"),
            Row(id: "switches", value: Stats.count(.focusSwitches), label: "Kachelwechsel"),
            Row(id: "record", value: Stats.record.count, label: "Meiste Sessions gleichzeitig", date: Stats.record.date),
        ]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().overlay(Theme.lineColor).padding(.top, 14)
            VStack(alignment: .leading, spacing: 7) {
                ForEach(rows) { row($0) }
            }
            .padding(16)
            Divider().overlay(Theme.lineColor)
            Text("Gezählt wird, solange Kadrell läuft.").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.top, 10)
            Text("Esc oder F3 schließen").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                .frame(maxWidth: .infinity, alignment: .trailing).padding(.horizontal, 16).padding(.vertical, 10)
        }
        .frame(width: 460 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Statistik").font(Theme.ui(18, bold: true)).foregroundStyle(Theme.fgColor)
            if let since = Stats.since {
                HStack(spacing: 8) {
                    Text("Seit")
                    Text(since, format: .dateTime.day().month(.wide).year())
                }
                .font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
            } else {
                Text("Noch nichts gezählt.").font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
            }
        }
        .padding(.horizontal, 16).padding(.top, 16)
    }

    private func row(_ r: Row) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(r.value, format: .number).font(Theme.ui(14, bold: true)).foregroundStyle(Theme.fgColor)
                .frame(width: 80 * Theme.scale, alignment: .trailing)
            Text(r.label).font(Theme.ui(12)).foregroundStyle(Theme.mutedColor).lineLimit(1)
            if let d = r.date {
                Spacer(minLength: 10)
                Text(d, format: .dateTime.day().month(.abbreviated).year())
                    .font(Theme.ui(11)).foregroundStyle(Theme.runningColor).lineLimit(1)
            }
        }
    }
}
