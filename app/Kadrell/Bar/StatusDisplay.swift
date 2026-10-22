import Foundation

/// Text und Lampe des Status-Displays in der Leiste. Reine Logik, gezeichnet wird anderswo.
enum StatusDisplay {
    enum Lamp { case waiting, error, ok, off }

    /// Lampe nach Dringlichkeit: wartet etwas, ist es Gelb, sonst Rot bei Fehlern, sonst Grün, solange etwas lebt.
    static func lamp(_ c: StatusCounts) -> Lamp {
        if c.waiting > 0 { return .waiting }
        if c.error > 0 { return .error }
        return c.running + c.idle > 0 ? .ok : .off
    }

    /// Nur Zustände mit Sessions, in der Reihenfolge arbeiten, warten, Fehler, getrennt durch " · ", in Versalien.
    /// Kompakt als Zahl mit Kürzel („2A · 1W“), wenn die Leiste für die lange Form zu schmal ist.
    static func text(_ c: StatusCounts, compact: Bool) -> String {
        let parts = compact ? compactParts(c) : longParts(c)
        return parts.joined(separator: " · ")
    }

    /// Langform aller Zustände für Tooltip und Bedienhilfe, wie der bisherige Tooltip der Zähler.
    /// Fehler stehen nur da, wenn es welche gibt.
    static func spoken(_ c: StatusCounts) -> String {
        let rows: [(n: Int, label: String, always: Bool)] = [
            (c.running, String(localized: "arbeiten", bundle: Bundle.app), true),
            (c.waiting, String(localized: "warten", bundle: Bundle.app), true),
            (c.idle, String(localized: "fertig", bundle: Bundle.app), true),
            (c.error, String(localized: "mit Fehler", bundle: Bundle.app), false),
            (c.detached, String(localized: "getrennt", bundle: Bundle.app), true),
        ]
        let states = rows.filter { $0.n > 0 || $0.always }.map { "\($0.n) \($0.label)" }
        return states.joined(separator: ", ") + String(localized: " · \(c.total) Sessions", bundle: Bundle.app)
    }

    private static func longParts(_ c: StatusCounts) -> [String] {
        var parts: [String] = []
        if c.running > 0 { parts.append(String(localized: "\(c.running) ARBEITEN", bundle: Bundle.app)) }
        if c.waiting > 0 { parts.append(String(localized: "\(c.waiting) WARTEN", bundle: Bundle.app)) }
        if c.error > 0 { parts.append(String(localized: "\(c.error) FEHLER", bundle: Bundle.app)) }
        return parts
    }

    private static func compactParts(_ c: StatusCounts) -> [String] {
        var parts: [String] = []
        if c.running > 0 { parts.append(String(localized: "\(c.running)A", bundle: Bundle.app)) }
        if c.waiting > 0 { parts.append(String(localized: "\(c.waiting)W", bundle: Bundle.app)) }
        if c.error > 0 { parts.append(String(localized: "\(c.error)F", bundle: Bundle.app)) }
        return parts
    }
}
