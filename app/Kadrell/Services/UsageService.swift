import Foundation
import os

/// Claude-Nutzung (5 h, 7 Tage, Fable-Woche) von `api.anthropic.com/api/oauth/usage`, derselbe Endpunkt wie die CLI.
/// Alles optional: ändert sich das Format oder fehlt der Token, bleiben die Werte nil und die Leiste zeigt „–%“.
struct Usage: Equatable, Sendable {
    var session: Int?
    var weekly: Int?
    var fable: Int?
    var sessionResets: Date?
    var weeklyResets: Date?
    static let empty = Usage()

    /// Der Stand, den man bei gleichmäßiger Verteilung über die Woche gerade haben dürfte, tageweise hochgerechnet:
    /// Tag 1 gibt gleich nach dem Reset rund 100/7 % frei, jeder weitere angebrochene Tag rund 100/7 % mehr, der
    /// 7. Tag 100 %. Bewusst nicht sekündlich, sonst stünde direkt nach dem Reset 0 % und jede Nutzung wäre „über
    /// Plan". Die Fensterlänge liefert der Endpunkt nicht mit, nur `resets_at`, sie ist hier 7 Tage. Nil ohne Reset.
    func weeklyPlan(now: Date = Date()) -> Int? {
        guard let weeklyResets else { return nil }
        let window: TimeInterval = 7 * 24 * 3600, day: TimeInterval = 24 * 3600
        let elapsed = min(max(window - weeklyResets.timeIntervalSince(now), 0), window)
        let daysElapsed = floor(elapsed / day)   // 0…6, erst genau am Reset 7
        return min(100, Int((daysElapsed + 1) * 100 / 7))
    }

    /// Bei gleichbleibendem Tempo hochgerechnet: Zeitpunkt, an dem das 7-Tage-Limit 100 % erreicht. Nil, solange
    /// man nicht über dem tageweisen Plan liegt (sonst schlüge die Warnung gleich nach dem Reset an), und nil, wenn
    /// die Hochrechnung erst am oder nach dem Reset läge: dann läuft man nicht früher in die Wand.
    func weeklyLockout(now: Date = Date()) -> Date? {
        guard let weeklyResets, let weekly, let plan = weeklyPlan(now: now), weekly > plan else { return nil }
        let window: TimeInterval = 7 * 24 * 3600
        let elapsed = window - weeklyResets.timeIntervalSince(now)
        guard elapsed > 0 else { return nil }
        let ratePerSecond = Double(weekly) / elapsed
        let lockout = now.addingTimeInterval(Double(100 - weekly) / ratePerSecond)
        return lockout < weeklyResets ? lockout : nil
    }

    /// Bevorzugt das `limits`-Array (kind session / weekly_all / weekly_scoped mit Modellname),
    /// fällt auf `five_hour` / `seven_day` zurück.
    static func parse(_ data: Data) -> Usage {
        guard let root = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else { return .empty }
        var u = Usage()
        let iso = ISO8601DateFormatter()
        iso.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        func date(_ v: Any?) -> Date? {
            guard let s = v as? String else { return nil }
            return iso.date(from: s) ?? ISO8601DateFormatter().date(from: s)
        }
        func pct(_ v: Any?) -> Int? {
            if let d = v as? Double { return Int(d.rounded()) }
            if let i = v as? Int { return i }
            return nil
        }
        if let limits = root["limits"] as? [[String: Any]] {
            for l in limits {
                let kind = l["kind"] as? String ?? ""
                let p = pct(l["percent"])
                switch kind {
                case "session": u.session = p; u.sessionResets = date(l["resets_at"])
                case "weekly_all": u.weekly = p; u.weeklyResets = date(l["resets_at"])
                case "weekly_scoped":
                    let name = ((l["scope"] as? [String: Any])?["model"] as? [String: Any])?["display_name"] as? String ?? ""
                    if u.fable == nil || name.lowercased().contains("fable") { u.fable = p }
                default: break
                }
            }
        }
        if u.session == nil, let f = root["five_hour"] as? [String: Any] { u.session = pct(f["utilization"]); u.sessionResets = date(f["resets_at"]) }
        if u.weekly == nil, let s = root["seven_day"] as? [String: Any] { u.weekly = pct(s["utilization"]); u.weeklyResets = date(s["resets_at"]) }
        if u.fable == nil, let o = root["seven_day_opus"] as? [String: Any] { u.fable = pct(o["utilization"]) }
        return u
    }
}

@MainActor
final class UsageService {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "usage")
    private(set) var usage = Usage.empty
    var onChange: ((Usage) -> Void)?
    private var task: Task<Void, Never>?
    /// Wann der geseedete Stand gemessen wurde, um den ersten Poll entsprechend nach hinten zu schieben.
    private var seededAt: Date?
    /// Steigt mit jedem `refreshNow` (etwa nach einem Account-Wechsel). Ein Fetch, der unter einer älteren Generation
    /// begann, lief noch mit dem alten Token; sein Ergebnis wird verworfen.
    private var generation = 0
    private let fetcher: () async -> Result

    init(fetcher: @escaping () async -> Result = UsageService.fetch) {
        self.fetcher = fetcher
    }

    /// Beim Start mit dem letzten bekannten Stand (persistierter Account-Cache) vorbelegen, damit die Leiste nicht
    /// bei „–%“ steht, bis der erste Poll durch ist. Löst bewusst kein `onChange` aus (kein Auto-Wechsel auf
    /// veraltete Werte); die Leiste setzt der Aufrufer direkt. Greift nur, solange noch kein Live-Stand da ist.
    func seed(_ u: Usage, at: Date) {
        guard usage == .empty else { return }
        usage = u
        seededAt = at
    }

    /// Alle drei Minuten; bei HTTP 429 (der Endpunkt limitiert streng) verdoppelt sich die Pause bis 15 min,
    /// `Retry-After` wird beachtet. Ein Fehler verwirft nie die letzten bekannten Werte.
    func start(interval: TimeInterval = 180) {
        task?.cancel()
        task = Task { [weak self] in
            var delay = interval
            // Ist der geseedete Stand noch frisch, den ersten Poll bis zum Ende des Intervalls aufschieben: sonst
            // hämmert jeder (Debug-)Neustart sofort gegen den streng limitierten Endpunkt und holt sich ein 429.
            if let seededAt = self?.seededAt {
                let wait = interval - Date().timeIntervalSince(seededAt)
                if wait > 0 { try? await Task.sleep(for: .seconds(wait)) }
            }
            while !Task.isCancelled {
                if let self {
                    let gen = self.generation
                    let r = await self.fetcher()
                    guard gen == self.generation else { try? await Task.sleep(for: .seconds(delay)); continue }
                    switch r {
                    case .ok(let fresh):
                        delay = interval
                        if fresh != self.usage { self.usage = fresh; self.onChange?(fresh) }
                    case .rateLimited(let retryAfter):
                        delay = min(900, max(retryAfter ?? delay * 2, interval))
                        UsageService.log.warning("usage: 429, nächster Versuch in \(Int(delay), privacy: .public) s")
                    case .failed:
                        delay = min(900, delay * 2)
                    }
                }
                try? await Task.sleep(for: .seconds(delay))
            }
        }
    }

    /// Sofort einmal abfragen, etwa direkt nach einem Account-Wechsel, ohne den laufenden Poll-Takt zu stören.
    func refreshNow() {
        generation += 1
        let gen = generation
        Task { [weak self] in
            guard let fetch = self?.fetcher else { return }
            if case .ok(let fresh) = await fetch(), let self, gen == self.generation, fresh != self.usage {
                self.usage = fresh
                self.onChange?(fresh)
            }
        }
    }

    enum Result { case ok(Usage), rateLimited(TimeInterval?), failed }

    /// OAuth-Token aus dem Schlüsselbund-Eintrag „Claude Code-credentials“ (wie die CLI ihn ablegt).
    static func token() async -> String? {
        guard let raw = await Keychain.read(service: AccountService.liveService) else {
            log.warning("usage: Schlüsselbund-Eintrag nicht lesbar"); return nil
        }
        guard let t = AccountService.oauth(from: raw)?["accessToken"] as? String, !t.isEmpty else {
            log.warning("usage: Schlüsselbund-Eintrag ohne accessToken"); return nil
        }
        return t
    }

    /// Unbekanntes Format nur einmal melden, nicht bei jedem Poll.
    private static var loggedUnknownFormat = false

    /// HTTP-200-Antwort deuten. Enthält sie keine einzige Zahl, ist das Format unbekannt: `.failed`, damit die letzten
    /// bekannten Werte stehen bleiben.
    static func result(for data: Data) -> Result {
        let u = Usage.parse(data)
        guard u.session == nil, u.weekly == nil, u.fable == nil else { return .ok(u) }
        if !loggedUnknownFormat {
            loggedUnknownFormat = true
            let keys = ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])?.keys.sorted() ?? []
            log.warning("usage: unbekanntes Antwortformat, Keys: \(keys.joined(separator: ","), privacy: .public)")
        }
        return .failed
    }

    static func fetch() async -> Result {
        guard let t = await token(), let url = URL(string: "https://api.anthropic.com/api/oauth/usage") else { return .failed }
        var req = URLRequest(url: url, timeoutInterval: 15)
        req.setValue("Bearer \(t)", forHTTPHeaderField: "Authorization")
        req.setValue("oauth-2025-04-20", forHTTPHeaderField: "anthropic-beta")
        req.setValue("Kadrell", forHTTPHeaderField: "User-Agent")
        do {
            let (data, resp) = try await URLSession.shared.data(for: req)
            let http = resp as? HTTPURLResponse
            if http?.statusCode == 429 {
                return .rateLimited(http?.value(forHTTPHeaderField: "Retry-After").flatMap { TimeInterval($0) })
            }
            guard http?.statusCode == 200 else {
                log.warning("usage: HTTP \(http?.statusCode ?? -1, privacy: .public)")
                return .failed
            }
            let r = result(for: data)
            if case .ok(let u) = r { log.notice("usage: 5h \(u.session ?? -1) 7d \(u.weekly ?? -1) fable \(u.fable ?? -1)") }
            return r
        } catch {
            log.warning("usage: \(String(describing: error), privacy: .public)")
            return .failed
        }
    }
}
