import Foundation

/// Ein gemessener Nutzungsstand eines Accounts. Kurze Schlüssel, weil davon über ein Jahr sechsstellig viele Zeilen entstehen.
struct UsageSample: Codable, Equatable, Sendable {
    var t: Double
    var account: String
    var session: Int?
    var weekly: Int?
    var fable: Int?

    init(t: Double, account: String, session: Int? = nil, weekly: Int? = nil, fable: Int? = nil) {
        self.t = t
        self.account = account
        self.session = session
        self.weekly = weekly
        self.fable = fable
    }

    init(account: String, usage: CachedUsage) {
        self.init(t: usage.at.timeIntervalSince1970, account: account, session: usage.session, weekly: usage.weekly, fable: usage.fable)
    }

    var isEmpty: Bool { session == nil && weekly == nil && fable == nil }
}

/// Verlauf der Limits je Account, eine JSON-Zeile pro Messung im Profilordner. Der Endpunkt liefert nur den aktuellen
/// Stand, deshalb kann der Verlauf erst ab der ersten Messung beginnen. Eine angeschnittene Zeile fällt beim Lesen raus.
@MainActor
final class UsageHistory {
    static let shared = UsageHistory(url: Profile.directory.appendingPathComponent("usage-history.jsonl"))

    private let url: URL
    /// Nil bis zum ersten Lesen: läuft das Statistik-Panel nie, muss auch die Datei nie in den Speicher.
    private var loaded: [UsageSample]?

    init(url: URL) { self.url = url }

    func append(_ sample: UsageSample) {
        guard !sample.isEmpty, let line = try? JSONEncoder().encode(sample) else { return }
        loaded?.append(sample)
        Self.write(line + Data("\n".utf8), to: url)
    }

    func samples() async -> [UsageSample] {
        if let loaded { return loaded }
        let url = url
        let read = await Task.detached(priority: .utility) { Self.parse((try? Data(contentsOf: url)) ?? Data()) }.value
        if let loaded { return loaded }   // Ein zweiter Aufrufer war schneller.
        loaded = read
        return read
    }

    nonisolated static func parse(_ data: Data) -> [UsageSample] {
        let decoder = JSONDecoder()
        return data.split(separator: UInt8(ascii: "\n")).compactMap { try? decoder.decode(UsageSample.self, from: Data($0)) }
    }

    private static func write(_ data: Data, to url: URL) {
        guard let handle = try? FileHandle(forWritingTo: url) else {
            try? data.write(to: url)
            return
        }
        defer { try? handle.close() }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: data)
    }
}
