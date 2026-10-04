import Foundation

enum ExtensionState: Equatable { case off, starting, running, failed(String), reloading }

enum SupervisorInput {
    case enable, disable, reload, spawned, ready
    case exited(expected: Bool, reason: String)
    case pingTimeout, readyTimeout, flood(String), restartDue
}

enum SupervisorAction: Equatable { case spawn, terminate, scheduleRestart(TimeInterval), clearUI }

/// Reine Zustandsmaschine einer Extension: bekommt Ereignisse, liefert Aktionen, startet und misst selbst nichts.
/// Ein Absturz zählt nur in `starting` und `running`; in `failed`, `off` und `reloading` ist der Prozess schon
/// abgeschrieben, ein nachgereichtes Ende oder Timeout darf nicht doppelt zählen.
struct ExtensionSupervisor {
    static let backoff: [TimeInterval] = [1, 5, 30]
    static let crashWindow: TimeInterval = 120
    static let maxCrashesInWindow = 3
    static let stableRun: TimeInterval = 120

    private(set) var state: ExtensionState = .off
    private var crashes: [Date] = []
    private var consecutive = 0
    private var readyAt: Date?
    private var restartPending = false

    mutating func handle(_ input: SupervisorInput, now: Date) -> [SupervisorAction] {
        switch input {
        case .enable: return enable()
        case .disable: return disable()
        case .reload: return reload()
        case .spawned: return []
        case .ready: return ready(now)
        case .exited(let expected, let reason): return exited(expected: expected, reason: reason, now: now)
        case .pingTimeout:
            return crash(String(localized: "Antwortet nicht mehr (kein Pong binnen 5 s)", bundle: Bundle.app), kill: true, now: now)
        case .readyTimeout:
            return crash(String(localized: "Startet nicht (kein Ready binnen 3 s)", bundle: Bundle.app), kill: true, now: now)
        case .flood(let reason): return crash(reason, kill: true, now: now)
        case .restartDue: return restartDue()
        }
    }

    private mutating func enable() -> [SupervisorAction] {
        guard state == .off || isFailed else { return [] }
        crashes = []
        consecutive = 0
        readyAt = nil
        restartPending = false
        state = .starting
        return [.spawn]
    }

    private mutating func disable() -> [SupervisorAction] {
        guard state != .off else { return [] }
        let alive = state == .starting || state == .running || state == .reloading
        state = .off
        restartPending = false
        readyAt = nil
        return alive ? [.terminate, .clearUI] : [.clearUI]
    }

    private mutating func reload() -> [SupervisorAction] {
        guard state == .starting || state == .running else { return [] }
        state = .reloading
        readyAt = nil
        return [.terminate, .clearUI]
    }

    private mutating func ready(_ now: Date) -> [SupervisorAction] {
        guard state == .starting else { return [] }
        state = .running
        readyAt = now
        return []
    }

    private mutating func exited(expected: Bool, reason: String, now: Date) -> [SupervisorAction] {
        if state == .reloading {
            state = .starting
            return [.spawn]
        }
        return expected ? [] : crash(reason, kill: false, now: now)
    }

    private mutating func restartDue() -> [SupervisorAction] {
        guard isFailed, restartPending else { return [] }
        restartPending = false
        state = .starting
        return [.spawn]
    }

    private mutating func crash(_ reason: String, kill: Bool, now: Date) -> [SupervisorAction] {
        guard state == .starting || state == .running else { return [] }
        let lead: [SupervisorAction] = (kill ? [.terminate] : []) + [.clearUI]
        let ranStable = readyAt.map { now.timeIntervalSince($0) >= Self.stableRun } ?? false
        readyAt = nil
        consecutive = (ranStable ? 0 : consecutive) + 1
        crashes = crashes.filter { now.timeIntervalSince($0) < Self.crashWindow } + [now]
        if crashes.count >= Self.maxCrashesInWindow {
            state = .failed(String(localized: "3 Abstürze in 2 min, bleibt aus", bundle: Bundle.app))
            return lead
        }
        state = .failed(reason)
        restartPending = true
        return lead + [.scheduleRestart(Self.backoff[min(consecutive, Self.backoff.count) - 1])]
    }

    private var isFailed: Bool {
        if case .failed = state { return true }
        return false
    }
}
