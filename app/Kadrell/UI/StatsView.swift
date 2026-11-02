import SwiftUI

/// F3: Limits je Account und Prompts im Zeitverlauf, darunter die Lebenszeit-Zähler des Profils (siehe `Stats`).
/// Füllt das Fenster. 1 bis 4 oder ←/→ wählen den Zeitraum, Esc, ⏎ oder F3 schließt.
struct StatsView: View {
    let model: StatsModel

    init(model: StatsModel = StatsModel()) { self.model = model }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            SeamLine().padding(.top, 12)
            VStack(alignment: .leading, spacing: 20) {
                if let snapshot = model.snapshot {
                    LimitsSection(snapshot: snapshot)
                    PromptsSection(snapshot: snapshot, reading: model.readingPrompts)
                }
                CountersSection()
            }
            .padding(16)
            SeamLine()
            footer
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var header: some View {
        HStack(spacing: 16) {
            Text("Statistik").font(Theme.ui(18, bold: true)).foregroundStyle(Theme.fgColor)
            HStack(spacing: 4) {
                ForEach(StatsRange.allCases) { rangeButton($0) }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16).padding(.top, 14)
    }

    private func rangeButton(_ range: StatsRange) -> some View {
        Button { model.range = range } label: {
            Text(range.label).font(Theme.ui(12)).foregroundStyle(Theme.fgColor).lineLimit(1)
                .padding(.horizontal, 12).padding(.vertical, 5)
                .instrumentRow(selected: model.range == range)
        }
        .buttonStyle(.plain)
        .focusable(false)
    }

    private var footer: some View {
        HStack {
            Text("1 bis 4 oder ←/→ wählen den Zeitraum")
            Spacer(minLength: 12)
            Text("Esc oder F3 schließen")
        }
        .font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1)
        .padding(.horizontal, 16).padding(.vertical, 10)
    }
}

/// Die Lebenszeit-Zähler als Reihe von Zahlen.
private struct CountersSection: View {
    private struct Counter: Identifiable {
        let id: String
        let value: Int
        let label: LocalizedStringKey
        var date: Date? = nil
    }

    private var counters: [Counter] {
        [
            Counter(id: "messages", value: Stats.count(.messages), label: "Nachrichten geschickt"),
            Counter(id: "sessions", value: Stats.count(.sessions), label: "Sessions geöffnet"),
            Counter(id: "terminals", value: Stats.count(.terminals), label: "Terminals geöffnet"),
            Counter(id: "switches", value: Stats.count(.focusSwitches), label: "Kachelwechsel"),
            Counter(id: "record", value: Stats.record.count, label: "Meiste Sessions gleichzeitig", date: Stats.record.date),
        ]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text("GESAMT").sectionTitle()
                since.font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
            }
            // Eine Reihe, wenn die Breite reicht, sonst zwei Spalten. Kein `LazyVGrid`: Das misst seine Höhe mit einer anderen Breite als der endgültigen.
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .top, spacing: 24) {
                    ForEach(counters) { tile($0) }
                }
                Grid(alignment: .topLeading, horizontalSpacing: 24, verticalSpacing: 10) {
                    ForEach(Array(stride(from: 0, to: counters.count, by: 2)), id: \.self) { i in
                        GridRow {
                            tile(counters[i])
                            if i + 1 < counters.count { tile(counters[i + 1]) }
                        }
                    }
                }
            }
            Text("Gezählt wird, solange Kadrell läuft.").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
        }
    }

    @ViewBuilder private var since: some View {
        if let since = Stats.since {
            HStack(spacing: 4) {
                Text("Seit")
                Text(since, format: .dateTime.day().month(.wide).year())
            }
        } else {
            Text("Noch nichts gezählt.")
        }
    }

    private func tile(_ counter: Counter) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(counter.value, format: .number).font(Theme.ui(16, bold: true)).foregroundStyle(Theme.fgColor)
            Text(counter.label).font(Theme.ui(12)).foregroundStyle(Theme.mutedColor).lineLimit(1)
            if let date = counter.date {
                Text(date, format: .dateTime.day().month(.abbreviated).year())
                    .font(Theme.ui(11)).foregroundStyle(Theme.accentTextColor).lineLimit(1)
            }
        }
    }
}
