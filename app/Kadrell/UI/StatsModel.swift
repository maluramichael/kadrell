import AppKit
import Observation

/// Daten der Statistik-Seite (F3). Liest nur, solange das Panel offen ist, und zieht dann alle 15 Sekunden nach:
/// Neue Prompts stehen sofort im Transcript, neue Limits kommen alle paar Minuten.
@Observable
@MainActor
final class StatsModel {
    private static let refreshInterval: Duration = .seconds(15)

    private(set) var snapshot: StatsSnapshot?
    /// Die Transcripts wurden seit dem Öffnen noch nicht gelesen. Beim allerersten Mal dauert das einige Sekunden.
    private(set) var readingPrompts = true
    var range: StatsRange = .week {
        didSet { if range != oldValue { rebuild() } }
    }

    @ObservationIgnored private let sources: @MainActor () -> [LimitSource]
    @ObservationIgnored private let samples: @MainActor () async -> [UsageSample]
    @ObservationIgnored private let sessions: @MainActor () async -> [HistorySession]
    @ObservationIgnored private var currentSamples: [UsageSample] = []
    @ObservationIgnored private var events: [PromptEvent] = []
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var monitor: Any?

    init(sources: @escaping @MainActor () -> [LimitSource] = { [] },
         samples: @escaping @MainActor () async -> [UsageSample] = { [] },
         sessions: @escaping @MainActor () async -> [HistorySession] = { [] }) {
        self.sources = sources
        self.samples = samples
        self.sessions = sessions
    }

    func setOpen(_ open: Bool) {
        guard open != (task != nil) else { return }
        open ? start() : stop()
    }

    private func start() {
        readingPrompts = true
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            self?.handle(event) ?? event
        }
        task = Task { [weak self] in
            while !Task.isCancelled {
                await self?.reload()
                try? await Task.sleep(for: Self.refreshInterval)
            }
        }
    }

    private func stop() {
        task?.cancel()
        task = nil
        if let monitor { NSEvent.removeMonitor(monitor) }
        monitor = nil
    }

    /// Erst die Limits zeigen: Das erste Lesen der Transcripts kann dauern, und solange bleibt die Seite nicht leer.
    func reload() async {
        currentSamples = await samples()
        await rebuildNow()
        let read = await sessions()
        events = await Task.detached(priority: .utility) { PromptStats.events(read) }.value
        readingPrompts = false
        await rebuildNow()
    }

    private func rebuild() {
        Task { [weak self] in await self?.rebuildNow() }
    }

    /// Rechnet außerhalb des Main-Actors. Hat sich unterdessen der Zeitraum geändert, gilt nur noch der neuere Lauf.
    private func rebuildNow() async {
        generation += 1
        let mine = generation, range = range, samples = currentSamples, events = events, sources = sources()
        let next = await Task.detached(priority: .userInitiated) {
            StatsSnapshot.make(range: range, now: Date(), samples: samples, sources: sources, events: events)
        }.value
        if mine == generation { snapshot = next }
    }

    /// 1 bis 4 wählen den Zeitraum, ←/→ blättern. Nur im Overlay: Das Terminal dahinter behält seine Tasten.
    private func handle(_ event: NSEvent) -> NSEvent? {
        guard event.window is OverlayPanel, event.modifierFlags.intersection(Hotkey.modMask).isEmpty,
              let next = Self.range(after: range, characters: event.charactersIgnoringModifiers, special: event.specialKey) else { return event }
        range = next
        return nil
    }

    nonisolated static func range(after current: StatsRange, characters: String?, special: NSEvent.SpecialKey?) -> StatsRange? {
        switch special {
        case .leftArrow?: return StatsRange(rawValue: current.rawValue - 1) ?? current
        case .rightArrow?: return StatsRange(rawValue: current.rawValue + 1) ?? current
        default: break
        }
        guard let digit = characters.flatMap(Int.init) else { return nil }
        return StatsRange(rawValue: digit - 1)
    }
}
