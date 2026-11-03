import Charts
import SwiftUI

/// Kategoriale Farben der Charts, mit `validate_palette.js` in beiden Modi geprüft: Hell und Dunkel sind eigene Stufen
/// derselben Farbtöne. Die Reihenfolge ist fest, ab dem achten Projekt gilt „Andere“ in Grau.
enum StatsPalette {
    private static let light: [UInt32] = [0x2a78d6, 0xeb6834, 0x1baf7a, 0xeda100, 0xe87ba4, 0x008300, 0x6250d6]
    private static let dark: [UInt32] = [0x3987e5, 0xd95926, 0x199e70, 0xc98500, 0xd55181, 0x008300, 0x9085e9]

    static func project(slot: Int?) -> Color {
        guard let slot else { return Theme.mutedColor }
        let colors = Theme.current.dark ? dark : light
        return Color(nsColor: NSColor(hex: colors[slot % colors.count]))
    }

    /// Die ersten drei Farben bestehen auch paarweise über alle Kombinationen, das brauchen sich kreuzende Linien.
    static func limit(_ kind: LimitKind) -> Color { project(slot: kind.rawValue) }
}

extension StatsRange {
    var label: LocalizedStringKey {
        switch self {
        case .day: return "24 Std."
        case .week: return "7 Tage"
        case .month: return "30 Tage"
        case .year: return "1 Jahr"
        }
    }

    fileprivate var axisValues: AxisMarkValues {
        switch self {
        case .day: return .stride(by: .hour, count: 4)
        case .week: return .stride(by: .day)
        case .month: return .stride(by: .day, count: 5)
        case .year: return .stride(by: .month, count: 2)
        }
    }

    fileprivate var axisFormat: Date.FormatStyle {
        switch self {
        case .day: return .dateTime.hour()
        case .week: return .dateTime.weekday(.abbreviated)
        case .month: return .dateTime.day().month(.abbreviated)
        case .year: return .dateTime.month(.abbreviated)
        }
    }

    /// Zeitpunkt im Tooltip eines Balkens: so genau, wie der Balken breit ist.
    fileprivate var barFormat: Date.FormatStyle {
        switch self {
        case .day, .week: return .dateTime.weekday(.abbreviated).day().month(.abbreviated).hour()
        case .month, .year: return .dateTime.day().month(.abbreviated).year()
        }
    }
}

extension ProjectSort {
    var label: LocalizedStringKey {
        switch self {
        case .count: return "Anzahl"
        case .name: return "Name"
        case .recent: return "Zuletzt"
        }
    }
}

extension LimitKind {
    var label: LocalizedStringKey {
        switch self {
        case .session: return "5 Stunden"
        case .weekly: return "7 Tage"
        case .fable: return "Fable"
        }
    }
}

extension AccountLimits {
    /// Einzelne Messungen ohne Nachbarn: Eine Linie zeichnet dafür nichts, sie brauchen einen Punkt.
    fileprivate var lonelyPoints: [LimitPoint] {
        Dictionary(grouping: points, by: \.series).values.compactMap { $0.count == 1 ? $0[0] : nil }
    }

    /// Je Limit der Messwert am nächsten bei `date`. Liegt keiner in der Nähe, fehlt das Limit im Tooltip.
    fileprivate func nearest(_ date: Date, tolerance: TimeInterval) -> [LimitPoint] {
        LimitKind.allCases.compactMap { kind in
            points.filter { $0.kind == kind }
                .min { abs($0.date.timeIntervalSince(date)) < abs($1.date.timeIntervalSince(date)) }
                .flatMap { abs($0.date.timeIntervalSince(date)) <= tolerance ? $0 : nil }
        }
    }
}

/// Setzt `date` auf den Zeitpunkt unter dem Mauszeiger. Hover statt `chartXSelection`: Das reagiert auf dem Mac nur aufs Ziehen.
private struct ChartHover: ViewModifier {
    @Binding var date: Date?

    func body(content: Content) -> some View {
        content.chartOverlay { proxy in
            GeometryReader { geometry in
                Rectangle().fill(.clear).contentShape(Rectangle()).onContinuousHover { phase in
                    guard case .active(let location) = phase, let plot = proxy.plotFrame else { date = nil; return }
                    date = proxy.value(atX: location.x - geometry[plot].minX, as: Date.self)
                }
            }
        }
    }
}

/// Linie an jedem Tagesbeginn (Jahr: Monatsbeginn), unter den Daten. Kräftiger als das Raster, damit man sieht,
/// wo ein Tag anfängt.
private struct DaySeparators: ChartContent {
    let days: [DayMark]
    let range: StatsRange

    var body: some ChartContent {
        ForEach(days) { day in
            RuleMark(x: .value("Zeit", day.date))
                .foregroundStyle(Theme.mutedColor.opacity(opacity(day)))
                .lineStyle(StrokeStyle(lineWidth: 1))
        }
    }

    /// In 30 Tagen stehen die Linien dicht, dort tritt nur der Wochenanfang mit Datum hervor.
    private func opacity(_ day: DayMark) -> Double { range == .month && !day.labeled ? 0.25 : 0.55 }
}

/// Datum oben rechts neben den beschrifteten Tageslinien. Eigene Ebene statt Annotation: Die verschöbe die Plotfläche.
private struct DayLabels: ViewModifier {
    let days: [DayMark]
    var background: Color = .clear
    /// Platz, den ein Datum rechts der Linie braucht.
    private static var labelRoom: CGFloat { 110 * Theme.scale }

    func body(content: Content) -> some View {
        content.chartOverlay(alignment: .topLeading) { proxy in
            GeometryReader { geometry in
                if let plot = proxy.plotFrame {
                    let frame = geometry[plot]
                    ZStack(alignment: .topLeading) {
                        ForEach(days.filter(\.labeled)) { day in
                            if let x = proxy.position(forX: day.date) {
                                Text(day.date, format: .dateTime.weekday(.abbreviated).day().month(.abbreviated))
                                    .font(Theme.ui(10, bold: true)).foregroundStyle(Theme.mutedColor).lineLimit(1).fixedSize()
                                    .padding(.horizontal, 3).background(background)
                                    // Breite null an der Linie: Das Datum hängt rechts daran, am rechten Rand links,
                                    // sonst ragte es aus dem Chart. Jedes Label für sich, die ZStack richtet nichts aus.
                                    .frame(width: 0, alignment: x > frame.width - Self.labelRoom ? .trailing : .leading)
                                    .offset(x: frame.minX + x, y: frame.minY + 1)
                            }
                        }
                    }
                }
            }
            .allowsHitTesting(false)
        }
    }
}

private struct ChartTooltip<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 2) { content }
            .font(Theme.ui(11)).foregroundStyle(Theme.fgColor)
            .padding(.horizontal, 8).padding(.vertical, 5)
            .background(Theme.surfaceColor, in: RoundedRectangle(cornerRadius: 4))
    }
}

/// Farbfleck neben einem Wert: Der Text bleibt in Textfarbe, die Farbe trägt nur die Zuordnung.
private struct Swatch: View {
    let color: Color

    var body: some View {
        RoundedRectangle(cornerRadius: 2).fill(color).frame(width: 8 * Theme.scale, height: 8 * Theme.scale).accessibilityHidden(true)
    }
}

private extension View {
    /// Rechts bleibt Platz, sonst schneidet der Rand die Beschriftung kurz vor „jetzt“ ab.
    func statsAxes(_ snapshot: StatsSnapshot) -> some View {
        chartXScale(domain: snapshot.from...snapshot.now, range: .plotDimension(endPadding: 24 * Theme.scale))
            .chartXAxis {
                AxisMarks(values: snapshot.range.axisValues) { value in
                    AxisGridLine().foregroundStyle(Theme.mutedColor.opacity(0.2))
                    AxisValueLabel {
                        if let date = value.as(Date.self) {
                            Text(date, format: snapshot.range.axisFormat).font(Theme.ui(10)).foregroundStyle(Theme.mutedColor)
                        }
                    }
                }
            }
    }
}

// MARK: Limits

struct LimitsSection: View {
    let snapshot: StatsSnapshot
    @Environment(\.overlayMaxWidth) private var maxWidth

    /// So viele Spalten, wie breit genug sind, aber nie mehr als Accounts: Sonst bleibt rechts eine leere Spalte stehen.
    private var columns: [GridItem] {
        let fit = maxWidth.isFinite ? Int(maxWidth / (380 * Theme.scale)) : snapshot.limits.count
        return Array(repeating: GridItem(.flexible(), spacing: 12, alignment: .top), count: max(1, min(fit, snapshot.limits.count)))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 14) {
                Text("LIMITS").sectionTitle()
                Text("Verbraucht in Prozent").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                ForEach(LimitKind.allCases) { kind in
                    HStack(spacing: 4) {
                        Swatch(color: StatsPalette.limit(kind))
                        Text(kind.label).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                    }
                }
            }
            LazyVGrid(columns: columns, alignment: .leading, spacing: 12) {
                ForEach(snapshot.limits) { LimitCard(account: $0, snapshot: snapshot) }
            }
        }
    }
}

private struct LimitCard: View {
    let account: AccountLimits
    let snapshot: StatsSnapshot
    @State private var hover: Date?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(account.title).font(Theme.ui(12, bold: true)).foregroundStyle(Theme.fgColor).lineLimit(1)
                Spacer(minLength: 8)
                Text(account.detail).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1)
            }
            if account.isEmpty {
                Text("Noch keine Messwerte in diesem Zeitraum.").font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
                    .frame(maxWidth: .infinity, minHeight: chartHeight)
            } else {
                chart
            }
        }
        .padding(10)
        .instrumentGroup()
    }

    private var chartHeight: CGFloat { 110 * Theme.scale }

    private var chart: some View {
        Chart {
            DaySeparators(days: snapshot.days, range: snapshot.range)
            // Nicht gemessen: gestrichelt, dünner und blasser, damit die Messwerte vorn bleiben.
            ForEach(account.bridges) { point in
                LineMark(x: .value("Zeit", point.date), y: .value("Prozent", point.value), series: .value("Linie", point.series))
                    .foregroundStyle(StatsPalette.limit(point.kind).opacity(0.6))
                    .lineStyle(StrokeStyle(lineWidth: 1.5, dash: [4, 3]))
            }
            ForEach(account.points) { point in
                LineMark(x: .value("Zeit", point.date), y: .value("Prozent", point.value), series: .value("Linie", point.series))
                    .foregroundStyle(StatsPalette.limit(point.kind))
                    .lineStyle(StrokeStyle(lineWidth: 2))
            }
            ForEach(account.lonelyPoints) { point in
                PointMark(x: .value("Zeit", point.date), y: .value("Prozent", point.value))
                    .foregroundStyle(StatsPalette.limit(point.kind))
            }
            if let hover {
                RuleMark(x: .value("Zeit", hover)).foregroundStyle(Theme.mutedColor)
                    .annotation(position: .top, overflowResolution: .init(x: .fit(to: .chart), y: .fit(to: .chart))) { tooltip(hover) }
            }
        }
        .chartYScale(domain: 0...100)
        .chartYAxis {
            AxisMarks(position: .leading, values: [0, 50, 100]) { value in
                AxisGridLine().foregroundStyle(Theme.mutedColor.opacity(0.2))
                AxisValueLabel {
                    Text("\(value.as(Int.self) ?? 0) %").font(Theme.ui(10)).foregroundStyle(Theme.mutedColor)
                }
            }
        }
        .statsAxes(snapshot)
        .modifier(DayLabels(days: snapshot.days, background: Theme.bgColor))
        .modifier(ChartHover(date: $hover))
        .frame(height: chartHeight)
    }

    private func tooltip(_ date: Date) -> some View {
        ChartTooltip {
            Text(date, format: .dateTime.weekday(.abbreviated).day().month(.abbreviated).hour().minute())
            ForEach(account.nearest(date, tolerance: max(LimitStats.minGap, snapshot.range.span / LimitStats.maxPoints))) { point in
                HStack(spacing: 4) {
                    Swatch(color: StatsPalette.limit(point.kind))
                    Text(point.kind.label)
                    Text("\(point.value) %")
                }
            }
        }
    }
}

// MARK: Prompts

struct PromptsSection: View {
    let snapshot: StatsSnapshot
    let reading: Bool
    let model: StatsModel
    @Environment(\.overlayMaxWidth) private var maxWidth
    @State private var hover: Date?

    /// Reicht die Breite nicht für Chart und Liste nebeneinander, steht die Liste darunter.
    private static let sideBySideWidth: CGFloat = 720

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline, spacing: 14) {
                Text("PROMPTS").sectionTitle()
                Text(snapshot.promptTotal, format: .number).font(Theme.ui(14, bold: true)).foregroundStyle(Theme.fgColor)
                Text("Aus den Transcripts, auch von Sessions außerhalb von Kadrell.").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1)
            }
            if snapshot.bars.isEmpty {
                placeholder
            } else if maxWidth >= Self.sideBySideWidth * Theme.scale {
                HStack(alignment: .top, spacing: 16) {
                    chart
                    projectList.frame(width: 240 * Theme.scale)
                }
            } else {
                chart
                projectList
            }
        }
    }

    private var chartHeight: CGFloat { 190 * Theme.scale }

    private var placeholder: some View {
        SwiftUI.Group {
            if reading { Text("Transcripts werden gelesen …") } else { Text("Keine Prompts in diesem Zeitraum.") }
        }
        .font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
        .frame(maxWidth: .infinity, minHeight: chartHeight)
    }

    private var chart: some View {
        Chart {
            DaySeparators(days: snapshot.days, range: snapshot.range)
            ForEach(snapshot.bars) { bar in
                let span = bar.visibleSpan(from: snapshot.from, to: snapshot.now)
                RectangleMark(xStart: .value("Zeit", span.lowerBound), xEnd: .value("Zeit", span.upperBound),
                              yStart: .value("Prompts", bar.low), yEnd: .value("Prompts", bar.high))
                    .foregroundStyle(StatsPalette.project(slot: bar.slot))
            }
            if let hover, !bars(at: hover).isEmpty {
                RuleMark(x: .value("Zeit", hover)).foregroundStyle(Theme.mutedColor)
                    .annotation(position: .top, overflowResolution: .init(x: .fit(to: .chart), y: .fit(to: .chart))) { tooltip(hover) }
            }
        }
        .chartYAxis {
            AxisMarks(position: .leading) { value in
                AxisGridLine().foregroundStyle(Theme.mutedColor.opacity(0.2))
                AxisValueLabel {
                    Text("\(value.as(Int.self) ?? 0)").font(Theme.ui(10)).foregroundStyle(Theme.mutedColor)
                }
            }
        }
        .statsAxes(snapshot)
        .modifier(DayLabels(days: snapshot.days, background: Theme.raisedColor.opacity(0.85)))
        .modifier(ChartHover(date: $hover))
        .frame(height: chartHeight)
    }

    private func bars(at date: Date) -> [PromptBar] {
        snapshot.bars.filter { $0.start <= date && date < $0.end }
    }

    private func tooltip(_ date: Date) -> some View {
        let parts = bars(at: date)
        return ChartTooltip {
            if let first = parts.first {
                Text(first.start, format: snapshot.range.barFormat)
                Text("\(parts.reduce(0) { $0 + $1.count }) Prompts").font(Theme.ui(11, bold: true))
            }
            ForEach(parts.reversed()) { part in
                HStack(spacing: 4) {
                    Swatch(color: StatsPalette.project(slot: part.slot))
                    if part.project == PromptStats.other { Text("Andere") } else { Text(verbatim: part.project) }
                    Text(part.count, format: .number)
                }
            }
        }
    }

    /// Macht die Farben auch als Zahlen lesbar, die Tabelle zum Chart. Alle Projekte des Zeitraums, scrollbar; ↑/↓
    /// bewegt den Cursor, Leertaste oder Klick wählt Projekte für den Chart, s sortiert.
    private var projectList: some View {
        VStack(alignment: .leading, spacing: 4) {
            listHeader
            ScrollViewReader { reader in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(snapshot.projects) { projectRow($0) }
                    }
                }
                .frame(maxHeight: chartHeight)
                .onChange(of: model.cursor) { _, cursor in
                    if let cursor { reader.scrollTo(cursor) }
                }
            }
        }
    }

    private var listHeader: some View {
        HStack(spacing: 4) {
            Text("Sortiert:")
            Text(snapshot.sort.label).foregroundStyle(Theme.fgColor)
            Spacer(minLength: 8)
            if !snapshot.filter.isEmpty { Text("\(snapshot.filter.count) gewählt").foregroundStyle(Theme.accentTextColor) }
        }
        .font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1)
        .padding(.horizontal, 12)
    }

    private func projectRow(_ project: ProjectCount) -> some View {
        let color = project.included ? Theme.fgColor : Theme.mutedColor
        return HStack(spacing: 6) {
            Swatch(color: project.included ? StatsPalette.project(slot: project.slot) : Theme.mutedColor.opacity(0.4))
            Text(verbatim: project.name).font(Theme.ui(12)).foregroundStyle(color).lineLimit(1).truncationMode(.middle)
            Spacer(minLength: 8)
            Text(project.count, format: .number).font(Theme.ui(12, bold: true)).foregroundStyle(color)
        }
        // Die Zeilenfläche rückt 6 pt ein, der Inhalt braucht innen nochmal so viel Luft.
        .padding(.horizontal, 12).padding(.vertical, 4)
        .instrumentRow(selected: model.cursor == project.name)
        .id(project.name)
        .onTapGesture {
            model.cursor = project.name
            model.toggle(project.name)
        }
    }
}
