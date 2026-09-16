import AppKit

/// Sounds und Haptik: leise macOS-Systemklänge, keine eigenen Audiodateien. Einstellung „Sounds“ regelt, was klingt.
@MainActor
enum Feedback {
    enum Level: String, CaseIterable {
        case off, important, all
        var title: String {
            switch self {
            case .off: "aus"
            case .important: "nur wartet und fertig"
            case .all: "alle"
            }
        }
    }

    enum Sound {
        /// Session wartet auf dich, Session ist fertig: zählen auch bei „nur wichtig“.
        case waiting, done
        /// Neue Session, Session entfernt, AUTO/SYNC umgeschaltet.
        case open, close, toggle

        var name: String {
            switch self {
            case .waiting: "Tink"
            case .done: "Glass"
            case .open: "Pop"
            case .close: "Bottle"
            case .toggle: "Morse"
            }
        }
        var important: Bool { self == .waiting || self == .done }
    }

    static func play(_ sound: Sound) {
        switch Settings.sounds {
        case .off: return
        case .important where !sound.important: return
        default: break
        }
        // Kopie: eine laufende NSSound-Instanz spielt nicht ein zweites Mal gleichzeitig.
        guard let s = NSSound(named: sound.name)?.copy() as? NSSound else { return }
        s.volume = 0.3
        s.play()
    }

    /// Einrasten beim Ziehen, nur auf Force-Touch-Trackpads spürbar.
    static func snap() {
        NSHapticFeedbackManager.defaultPerformer.perform(.alignment, performanceTime: .now)
    }

    /// Fortschritt einer Animation seit `start`, 0...1, zum Ende hin abbremsend. nil = vorbei.
    nonisolated static func progress(since start: CFTimeInterval, duration: CFTimeInterval, now: CFTimeInterval = CACurrentMediaTime()) -> CGFloat? {
        let p = (now - start) / duration
        guard p >= 0, p < 1 else { return nil }
        return CGFloat(1 - pow(1 - p, 3))
    }

    /// Sessions, die seit dem letzten Stand zu warten begonnen haben bzw. vom Arbeiten in den Leerlauf gewechselt sind.
    nonisolated static func transitions(from old: [String: SessionStatus], to new: [String: SessionStatus]) -> (waiting: Set<String>, done: Set<String>) {
        var waiting = Set<String>(), done = Set<String>()
        for (id, s) in new {
            guard let o = old[id], o != s else { continue }
            if s == .waiting { waiting.insert(id) }
            if o == .running, s == .idle { done.insert(id) }
        }
        return (waiting, done)
    }
}
