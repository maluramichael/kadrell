import Foundation

/// Lebenszeit-Zähler des Profils: geschickte Nachrichten, geöffnete Sessions und Terminals, Kachelwechsel und der
/// Rekord gleichzeitig offener Sessions. Sechs Zahlen brauchen keine eigene Datei, sie liegen in den Einstellungen
/// des Profils; ein Wegwerfprofil (`--profile tmp`) nimmt seine Zahlen beim Beenden mit, statt sie einzurechnen.
enum Stats {
    /// Nur für Tests austauschbar, sonst immer das laufende Profil.
    nonisolated(unsafe) static var defaults: UserDefaults = Profile.defaults

    enum Key: String, CaseIterable {
        case messages, sessions, terminals, focusSwitches
        var defaultsKey: String { "stats." + rawValue }
    }

    private static let recordKey = "stats.record"
    private static let recordDateKey = "stats.recordDate"
    private static let sinceKey = "stats.since"

    static func count(_ key: Key) -> Int { defaults.integer(forKey: key.defaultsKey) }

    static func bump(_ key: Key, by n: Int = 1) {
        guard n > 0 else { return }
        markStart()
        defaults.set(count(key) + n, forKey: key.defaultsKey)
    }

    /// Höchststand gleichzeitig offener Sessions, mit dem Tag, an dem er aufgestellt wurde.
    /// Ein niedrigerer Stand ändert nichts, auch nicht das Datum.
    static func noteConcurrent(_ n: Int) {
        markStart()
        guard n > defaults.integer(forKey: recordKey) else { return }
        defaults.set(n, forKey: recordKey)
        defaults.set(Date().timeIntervalSince1970, forKey: recordDateKey)
    }

    static var record: (count: Int, date: Date?) { (defaults.integer(forKey: recordKey), date(recordDateKey)) }

    /// Zählbeginn, gesetzt beim ersten gezählten Ereignis. Nil, solange nichts gezählt wurde.
    static var since: Date? { date(sinceKey) }

    private static func date(_ key: String) -> Date? {
        let t = defaults.double(forKey: key)
        return t > 0 ? Date(timeIntervalSince1970: t) : nil
    }

    private static func markStart() {
        guard defaults.double(forKey: sinceKey) == 0 else { return }
        defaults.set(Date().timeIntervalSince1970, forKey: sinceKey)
    }
}
