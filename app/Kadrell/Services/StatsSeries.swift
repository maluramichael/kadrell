import Foundation

/// Zeitraum der Statistik-Seite, zugleich die Breite der Prompt-Balken.
enum StatsRange: Int, CaseIterable, Identifiable, Sendable {
    case day, week, month, year

    var id: Int { rawValue }

    var span: TimeInterval {
        let day: TimeInterval = 24 * 3600
        switch self {
        case .day: return day
        case .week: return 7 * day
        case .month: return 30 * day
        case .year: return 365 * day
        }
    }

    func barStart(_ date: Date, calendar: Calendar) -> Date {
        switch self {
        case .day, .week:
            var c = calendar.dateComponents([.year, .month, .day, .hour], from: date)
            let hours = self == .day ? 1 : 3
            c.hour = (c.hour ?? 0) / hours * hours
            return calendar.date(from: c) ?? date
        case .month:
            return calendar.startOfDay(for: date)
        case .year:
            return calendar.dateInterval(of: .weekOfYear, for: date)?.start ?? date
        }
    }

    func barEnd(_ start: Date, calendar: Calendar) -> Date {
        let step: (Calendar.Component, Int) = switch self {
        case .day: (.hour, 1)
        case .week: (.hour, 3)
        case .month: (.day, 1)
        case .year: (.weekOfYear, 1)
        }
        return calendar.date(byAdding: step.0, value: step.1, to: start) ?? start
    }
}

struct PromptEvent: Equatable, Sendable {
    let time: Double
    let project: String
}

/// Teil eines gestapelten Balkens: `low`…`high` auf der Prompt-Achse. `slot` nil heißt „Andere“.
struct PromptBar: Identifiable, Equatable, Sendable {
    let start: Date
    let end: Date
    let project: String
    let slot: Int?
    let low: Int
    let high: Int

    var id: String { "\(start.timeIntervalSince1970)-\(project)" }
    var count: Int { high - low }

    /// Gezeichneter Teil im sichtbaren Zeitraum. Der Rand schneidet einen Balken oft mitten durch; ungekürzt läge er
    /// über der Achse oder der Liste daneben. Ein Zehntel der Breite bleibt frei, damit Balken nicht zusammenlaufen.
    func visibleSpan(from: Date, to: Date) -> ClosedRange<Date> {
        let left = max(start, from)
        let right = min(end.addingTimeInterval(-end.timeIntervalSince(start) * 0.1), to)
        return left...max(left, right)
    }
}

struct ProjectCount: Identifiable, Equatable, Sendable {
    let name: String
    let count: Int
    let slot: Int?

    var id: String { name }
}

enum PromptStats {
    /// So viele Projekte bekommen eine eigene Farbe, alle weiteren liegen zusammen unter „Andere“.
    static let colorSlots = 7
    /// Projektname von „Andere“; ein Ordner kann nicht leer heißen.
    static let other = ""

    /// Alle Eingaben aus den Transcripts. Ein fortgesetztes Transcript kann ältere Zeilen kopieren, deshalb zählt
    /// derselbe Zeitstempel nur einmal.
    static func events(_ sessions: [HistorySession]) -> [PromptEvent] {
        var seen = Set<Double>()
        var out: [PromptEvent] = []
        for session in sessions {
            let project = projectName(cwd: session.cwd)
            for time in session.promptTimes where seen.insert(time).inserted {
                out.append(PromptEvent(time: time, project: project))
            }
        }
        return out
    }

    /// Ein Worktree zählt zum Projekt, in dem er liegt.
    static func projectName(cwd: String) -> String {
        let base = cwd.components(separatedBy: "/.claude/worktrees/").first ?? cwd
        return (base as NSString).lastPathComponent
    }

    /// Farbe je Projekt, nach den Prompts der letzten 30 Tage. Bewusst unabhängig vom gewählten Zeitraum: Ein Projekt
    /// behält beim Umschalten seine Farbe.
    static func slots(_ events: [PromptEvent], now: Date) -> [String: Int] {
        let recentFrom = now.timeIntervalSince1970 - StatsRange.month.span
        var recent: [String: Int] = [:], total: [String: Int] = [:]
        for event in events {
            total[event.project, default: 0] += 1
            if event.time >= recentFrom { recent[event.project, default: 0] += 1 }
        }
        let ranked = total.keys.sorted {
            let a = (recent[$0] ?? 0, total[$0] ?? 0), b = (recent[$1] ?? 0, total[$1] ?? 0)
            return a != b ? a > b : $0 < $1
        }
        return Dictionary(uniqueKeysWithValues: ranked.prefix(colorSlots).enumerated().map { ($1, $0) })
    }

    static func inRange(_ events: [PromptEvent], _ range: StatsRange, now: Date) -> [PromptEvent] {
        let from = now.timeIntervalSince1970 - range.span, to = now.timeIntervalSince1970
        return events.filter { $0.time >= from && $0.time <= to }
    }

    /// Gestapelte Balken für die schon gefilterten Eingaben: Projekte in Farbreihenfolge, „Andere“ oben.
    static func bars(_ events: [PromptEvent], range: StatsRange, slots: [String: Int], calendar: Calendar = .current) -> [PromptBar] {
        var buckets: [Date: [String: Int]] = [:]
        for event in events {
            let start = range.barStart(Date(timeIntervalSince1970: event.time), calendar: calendar)
            let project = slots[event.project] == nil ? other : event.project
            buckets[start, default: [:]][project, default: 0] += 1
        }
        return buckets.keys.sorted().flatMap { start in
            stack(buckets[start] ?? [:], start: start, end: range.barEnd(start, calendar: calendar), slots: slots)
        }
    }

    private static func stack(_ counts: [String: Int], start: Date, end: Date, slots: [String: Int]) -> [PromptBar] {
        var low = 0
        return counts.keys.sorted { (slots[$0] ?? colorSlots) < (slots[$1] ?? colorSlots) }.map { project in
            let high = low + (counts[project] ?? 0)
            defer { low = high }
            return PromptBar(start: start, end: end, project: project, slot: slots[project], low: low, high: high)
        }
    }

    /// Alle Projekte des Zeitraums, meiste Prompts zuerst.
    static func projects(_ events: [PromptEvent], slots: [String: Int]) -> [ProjectCount] {
        var counts: [String: Int] = [:]
        for event in events { counts[event.project, default: 0] += 1 }
        return counts.map { ProjectCount(name: $0.key, count: $0.value, slot: slots[$0.key]) }
            .sorted { $0.count != $1.count ? $0.count > $1.count : $0.name < $1.name }
    }
}

enum LimitKind: Int, CaseIterable, Identifiable, Sendable {
    case session, weekly, fable

    var id: Int { rawValue }

    func value(_ sample: UsageSample) -> Int? {
        switch self {
        case .session: return sample.session
        case .weekly: return sample.weekly
        case .fable: return sample.fable
        }
    }
}

/// `segment` trennt die Linie an Messlücken: Lief Kadrell nicht, gibt es für die Zeit keine Werte, und eine
/// durchgezogene Linie würde welche vortäuschen.
struct LimitPoint: Identifiable, Equatable, Sendable {
    let date: Date
    let value: Int
    let kind: LimitKind
    let segment: Int

    var id: String { "\(kind.rawValue)-\(date.timeIntervalSince1970)" }
    var series: String { "\(kind.rawValue)-\(segment)" }
}

enum LimitStats {
    /// Mehr Punkte je Linie sieht man auf der Kachel nicht.
    static let maxPoints = 240.0
    /// Ein inaktiver Account wird alle 20 Minuten gemessen, Lücken darunter sind normal.
    static let minGap: TimeInterval = 3600

    static func points(_ samples: [UsageSample], range: StatsRange, now: Date, kinds: [LimitKind]) -> [LimitPoint] {
        let from = now.timeIntervalSince1970 - range.span
        let visible = samples.filter { $0.t >= from }.sorted { $0.t < $1.t }
        return kinds.flatMap { line(visible, kind: $0, width: range.span / maxPoints) }
    }

    /// Je Bucket der Höchststand: So bleibt die Spitze vor einem Reset auch in der Jahresansicht sichtbar.
    private static func line(_ samples: [UsageSample], kind: LimitKind, width: TimeInterval) -> [LimitPoint] {
        var peaks: [(t: Double, value: Int)] = []
        var bucket = Int.min
        for sample in samples {
            guard let value = kind.value(sample) else { continue }
            let b = Int(sample.t / width)
            if b != bucket {
                peaks.append((sample.t, value))
                bucket = b
            } else if let last = peaks.last, value >= last.value {
                peaks[peaks.count - 1] = (sample.t, value)
            }
        }
        return segmented(peaks, kind: kind, gap: max(minGap, 3 * width))
    }

    private static func segmented(_ peaks: [(t: Double, value: Int)], kind: LimitKind, gap: TimeInterval) -> [LimitPoint] {
        var segment = 0
        var previous: Double?
        return peaks.map { peak in
            if let previous, peak.t - previous > gap { segment += 1 }
            previous = peak.t
            return LimitPoint(date: Date(timeIntervalSince1970: peak.t), value: peak.value, kind: kind, segment: segment)
        }
    }
}

/// Eine Kachel der Limits. Titel und Detail entstehen auf dem Main-Actor, weil sie vom Sprach-Bundle abhängen.
struct LimitSource: Sendable {
    let id: String
    let title: String
    let detail: String
    let kinds: [LimitKind]
    /// Letzter Stand aus dem Account-Cache: So hat die Kachel gleich einen Punkt, auch vor der ersten neuen Messung.
    let current: UsageSample?
}

struct AccountLimits: Identifiable, Equatable, Sendable {
    let id: String
    let title: String
    let detail: String
    let points: [LimitPoint]
}

/// Was die Statistik-Seite zeigt, fertig gerechnet. Entsteht außerhalb des Main-Actors, ein Jahr Verlauf dauert spürbar.
struct StatsSnapshot: Equatable, Sendable {
    var range: StatsRange
    var from: Date
    var now: Date
    var limits: [AccountLimits]
    var bars: [PromptBar]
    var projects: [ProjectCount]
    var promptTotal: Int

    static func make(range: StatsRange, now: Date, samples: [UsageSample], sources: [LimitSource], events: [PromptEvent]) -> StatsSnapshot {
        let visible = PromptStats.inRange(events, range, now: now)
        let slots = PromptStats.slots(events, now: now)
        let grouped = Dictionary(grouping: samples, by: \.account)
        let limits = sources.map { source -> AccountLimits in
            let own = merged(grouped[source.id] ?? [], current: source.current)
            let points = LimitStats.points(own, range: range, now: now, kinds: source.kinds)
            return AccountLimits(id: source.id, title: source.title, detail: source.detail, points: points)
        }
        return StatsSnapshot(range: range, from: now.addingTimeInterval(-range.span), now: now, limits: limits,
                             bars: PromptStats.bars(visible, range: range, slots: slots),
                             projects: PromptStats.projects(visible, slots: slots), promptTotal: visible.count)
    }

    /// Der Cache-Stand zählt nur, wenn er neuer ist als die letzte Zeile, sonst stünde dieselbe Messung doppelt da.
    private static func merged(_ samples: [UsageSample], current: UsageSample?) -> [UsageSample] {
        guard let current, !current.isEmpty, current.t > (samples.map(\.t).max() ?? 0) else { return samples }
        return samples + [current]
    }
}
