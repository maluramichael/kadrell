import Foundation

/// Lesende Systemwerte für `kadrell.system` im ext-host: CPU-Last je Kern und Plattenbelegung. Nichts davon läuft von
/// selbst, gemessen wird nur, wenn eine Extension fragt.
enum SystemProbe {
    /// Zählerstände eines Kerns seit dem Start.
    struct Ticks: Equatable { var busy, total: UInt64 }

    /// Last je Kern (0...1) zwischen zwei Messungen. Ein Kern ohne neue Ticks oder mit zurückgesprungenem Zähler
    /// (Überlauf nach Monaten) zählt als 0. Ändert sich die Kernzahl, gibt es keinen Vergleich.
    static func load(from old: [Ticks], to new: [Ticks]) -> [Double]? {
        guard old.count == new.count, !new.isEmpty else { return nil }
        return zip(old, new).map { a, b in
            guard b.total > a.total, b.busy >= a.busy else { return 0 }
            return min(1, Double(b.busy - a.busy) / Double(b.total - a.total))
        }
    }

    /// Aktuelle Zählerstände aller Kerne, nil wenn der Kernel nichts liefert.
    static func ticks() -> [Ticks]? {
        let max = 512
        var raw = [UInt64](repeating: 0, count: 2 * max)
        let n = Int(ks_cpu_ticks(&raw, Int32(max)))
        guard n > 0 else { return nil }
        return (0..<n).map { Ticks(busy: raw[2 * $0], total: raw[2 * $0 + 1]) }
    }

    /// Ergebnis für Lua: `cores` je Kern, `total` als Mittel über alle.
    static func cpuFields(_ cores: [Double]) -> [String: Any] {
        ["cores": cores, "total": cores.isEmpty ? 0 : cores.reduce(0, +) / Double(cores.count)]
    }

    /// Belegung des Volumes, auf dem `path` liegt. `free` wie im Finder: inklusive Platz, den macOS bei Bedarf
    /// freiräumt (lokale Snapshots, Caches). Bytes als Zahlen, nil wenn der Pfad nicht lesbar ist.
    static func disk(_ path: String) -> [String: Any]? {
        let url = URL(fileURLWithPath: (path as NSString).expandingTildeInPath)
        let keys: Set<URLResourceKey> = [.volumeTotalCapacityKey, .volumeAvailableCapacityForImportantUsageKey, .volumeAvailableCapacityKey, .volumeNameKey]
        guard let v = try? url.resourceValues(forKeys: keys), let total = v.volumeTotalCapacity, total > 0 else { return nil }
        let free = v.volumeAvailableCapacityForImportantUsage.map(Double.init) ?? Double(v.volumeAvailableCapacity ?? 0)
        let t = Double(total), f = min(max(free, 0), t)
        return ["total": t, "free": f, "used": t - f, "fraction": (t - f) / t, "name": v.volumeName ?? ""]
    }
}
