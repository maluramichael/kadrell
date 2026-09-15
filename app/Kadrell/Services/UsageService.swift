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

    /// Alle drei Minuten; bei HTTP 429 (der Endpunkt limitiert streng) verdoppelt sich die Pause bis 15 min,
    /// `Retry-After` wird beachtet. Ein Fehler verwirft nie die letzten bekannten Werte.
    func start(interval: TimeInterval = 180) {
        task?.cancel()
        task = Task { [weak self] in
            var delay = interval
            while !Task.isCancelled {
                if let self {
                    let r = await UsageService.fetch()
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

    enum Result { case ok(Usage), rateLimited(TimeInterval?), failed }

    /// OAuth-Token aus dem Schlüsselbund-Eintrag „Claude Code-credentials“ (wie die CLI ihn ablegt).
    static func token() async -> String? {
        guard let r = try? await ClaudeCLI.runRaw("/usr/bin/security", ["find-generic-password", "-s", "Claude Code-credentials", "-w"], environment: nil, cwd: nil) else {
            log.warning("usage: security nicht startbar"); return nil
        }
        guard r.status == 0 else { log.warning("usage: security exit \(r.status, privacy: .public): \(r.output.prefix(120), privacy: .public)"); return nil }
        guard let root = (try? JSONSerialization.jsonObject(with: Data(r.output.utf8))) as? [String: Any],
              let oauth = root["claudeAiOauth"] as? [String: Any],
              let t = oauth["accessToken"] as? String, !t.isEmpty else { log.warning("usage: Schlüsselbund-Eintrag ohne accessToken"); return nil }
        return t
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
            let u = Usage.parse(data)
            log.notice("usage: 5h \(u.session ?? -1) 7d \(u.weekly ?? -1) fable \(u.fable ?? -1)")
            return .ok(u)
        } catch {
            log.warning("usage: \(String(describing: error), privacy: .public)")
            return .failed
        }
    }
}
