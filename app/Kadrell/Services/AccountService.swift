import Foundation
import os

/// Ein hinterlegter Claude-Account. Das OAuth-Geheimnis (`claudeAiOauth`-Block) liegt im Schlüsselbund unter
/// `storeService`/`id`, hier stehen nur die anzeigbaren Metadaten.
/// Zuletzt bekannter Nutzungsstand eines Accounts, gemerkt solange er aktiv war. So zeigt das Menü auch für
/// gerade nicht aktive Accounts den letzten Stand mit Alter, ohne jeden Account extra abzufragen.
struct CachedUsage: Codable, Equatable, Sendable {
    var session: Int?
    var weekly: Int?
    var fable: Int?
    /// Wann die Fenster laut Endpoint zurücksetzen. Ist der Zeitpunkt seit der Messung vorbei, gilt die Dimension
    /// als wieder frei, ohne dass wir den inaktiven Account extra abfragen müssen.
    var sessionResets: Date?
    var weeklyResets: Date?
    var at: Date

    func session(at now: Date) -> Int? { freed(session, sessionResets, now) }
    func weekly(at now: Date) -> Int? { freed(weekly, weeklyResets, now) }
    /// Als Live-`Usage` zum Vorbelegen der Leiste beim Start, reset-bereinigt wie im Menü (Fable folgt dem 7-Tage-Reset).
    func asUsage(at now: Date = Date()) -> Usage {
        Usage(session: session(at: now), weekly: weekly(at: now), fable: freed(fable, weeklyResets, now),
              sessionResets: sessionResets, weeklyResets: weeklyResets)
    }
    /// Bindender Wert für den Auto-Wechsel: der höhere von 5 h und 7 Tagen, jeweils reset-bereinigt.
    func used(at now: Date = Date()) -> Int { max(session(at: now) ?? 0, weekly(at: now) ?? 0) }
    private func freed(_ v: Int?, _ resets: Date?, _ now: Date) -> Int? {
        if let resets, resets <= now { return 0 }
        return v
    }
}

struct Account: Codable, Equatable, Identifiable, Sendable {
    let id: String
    var alias: String
    var email: String?
    var subscription: String?
    var usage: CachedUsage?
    /// Anzeigename: eigener Alias, sonst E-Mail, sonst ein neutraler Platzhalter.
    var title: String {
        if !alias.isEmpty { return alias }
        if let email, !email.isEmpty { return email }
        return String(localized: "Konto", bundle: Bundle.app)
    }
}

/// Mehrere Claude-Accounts in einem Fenster: den einen Schlüsselbund-Eintrag `Claude Code-credentials` gegen den
/// gespeicherten Block eines anderen Accounts tauschen (Global-Swap). Alle laufenden `claude`-Prozesse ziehen beim
/// nächsten Token-Lesen (~30 s) nach. Nur der `claudeAiOauth`-Block wird getauscht, `mcpOAuth` (MCP-Logins,
/// account-unabhängig) bleibt stehen.
@MainActor
final class AccountService {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "accounts")
    /// Der Eintrag, den die `claude`-CLI liest. Sein Account-Name ist der macOS-Benutzer.
    nonisolated static let liveService = "Claude Code-credentials"
    nonisolated static let storeService = "de.malura.kadrell.account"
    /// Nach einem Auto-Wechsel so lange nicht erneut wechseln, gegen Flattern nahe der Schwelle.
    private static let cooldown: TimeInterval = 300

    private(set) var accounts: [Account] = []
    private(set) var activeId: String? { didSet { Settings.activeAccountId = activeId } }
    var onChange: (() -> Void)?
    /// Nach einem automatischen Wechsel aufgerufen, mit dem jetzt aktiven Account (für die Meldung an den Nutzer).
    var onAutoSwitch: ((Account) -> Void)?
    private var lastSwitch = Date.distantPast
    /// Ein Wechsel läuft: kein zweiter parallel (der zweite läse den halb geschriebenen Eintrag).
    private var busy = false
    /// Verständlicher Grund, warum der letzte Wechsel oder die letzte Aufnahme gescheitert ist; nil bei Erfolg.
    private(set) var lastError: String?
    private let keychain: KeychainAccess
    private let fetchEmail: @Sendable (String) async -> String?

    init(keychain: KeychainAccess = .live,
         profileEmail: @escaping @Sendable (String) async -> String? = AccountService.profileEmail) {
        self.keychain = keychain
        fetchEmail = profileEmail
        if let data = Settings.accountsData, let list = try? JSONDecoder().decode([Account].self, from: data) { accounts = list }
        activeId = Settings.activeAccountId
    }

    var active: Account? { accounts.first { $0.id == activeId } }
    /// Für die Leiste: je Account Kürzel, Markierung des aktiven und der zwischengespeicherte Nutzungsstand.
    var menuItems: [(id: String, title: String, active: Bool, detail: String)] {
        accounts.map { ($0.id, $0.title, $0.id == activeId, Self.usageDetail($0.usage)) }
    }

    /// „5h 45% · 7d 56% · vor 12 min“ aus dem gecachten Stand, leer wenn nie ein Stand gemerkt wurde. Ein Fenster,
    /// das seit der Messung zurückgesetzt hat, steht als „frei“.
    static func usageDetail(_ u: CachedUsage?) -> String {
        guard let u else { return "" }
        let now = Date()
        var parts = [dim("5h", u.session, u.sessionResets, now), dim("7d", u.weekly, u.weeklyResets, now)].filter { !$0.isEmpty }
        guard !parts.isEmpty else { return "" }
        let mins = max(0, Int(now.timeIntervalSince(u.at) / 60))
        parts.append(mins < 1 ? String(localized: "gerade eben", bundle: Bundle.app) : String(localized: "vor \(mins) min", bundle: Bundle.app))
        return parts.joined(separator: " · ")
    }

    private static func dim(_ label: String, _ pct: Int?, _ resets: Date?, _ now: Date) -> String {
        guard let pct else { return "" }
        if let resets, resets <= now { return label + " " + String(localized: "frei", bundle: Bundle.app) }
        return "\(label) \(pct)%"
    }

    /// Nutzung des aktiven Accounts aus dem Poll merken, damit das Menü sie später auch für den inaktiven zeigt.
    func recordUsage(_ u: Usage) {
        guard let activeId, let i = accounts.firstIndex(where: { $0.id == activeId }) else { return }
        accounts[i].usage = CachedUsage(session: u.session, weekly: u.weekly, fable: u.fable,
                                        sessionResets: u.sessionResets, weeklyResets: u.weeklyResets, at: Date())
        persist()
    }

    private func persist() {
        Settings.accountsData = try? JSONEncoder().encode(accounts)
        onChange?()
    }

    // MARK: Hinzufügen

    /// Nimmt den gerade angemeldeten Account (aktueller `Claude Code-credentials`-Eintrag) als Slot auf. Ist er schon
    /// dabei (gleiche E-Mail, ohne E-Mail gleicher Token), wird nur sein Block aufgefrischt und er aktiv gesetzt.
    func addCurrent() async -> Bool {
        lastError = nil
        guard let raw = await keychain.read(Self.liveService, nil), let oauth = Self.oauth(from: raw) else {
            return fail("read live", String(localized: "Kein angemeldeter Claude-Account gefunden", bundle: Bundle.app))
        }
        guard let id = await capture(oauth, fallbackActive: false) else { return false }
        setActive(id)
        return true
    }

    // MARK: Wechseln

    /// Schaltet den aktiven Account auf `id`. Der live liegende Block wird vorher in den Slot des Accounts gesichert,
    /// dem er gehört (die `claude`-CLI rotiert die Tokens im Betrieb). Scheitert das Sichern, bleibt live unverändert.
    @discardableResult
    func switchTo(_ id: String) async -> Bool {
        lastError = nil
        guard !busy else { return fail("busy", String(localized: "Ein Wechsel läuft schon", bundle: Bundle.app)) }
        guard id != activeId, let target = accounts.first(where: { $0.id == id }) else { return false }
        busy = true
        defer { busy = false }
        guard let raw = await keychain.read(Self.liveService, nil), Self.object(from: raw) != nil else {
            return fail("read live", String(localized: "Kein angemeldeter Claude-Account gefunden", bundle: Bundle.app))
        }
        var owner: String?
        if let oauth = Self.oauth(from: raw) {
            guard let o = await capture(oauth, fallbackActive: true) else { return false }
            owner = o
        }
        guard let targetRaw = await keychain.read(Self.storeService, id), let targetOauth = Self.object(from: targetRaw) else {
            return fail("read target", String(localized: "Gespeicherter Zugang für „\(target.title)“ ist nicht lesbar", bundle: Bundle.app))
        }
        guard var full = await rereadLive(since: raw, owner: owner) else { return false }
        full["claudeAiOauth"] = targetOauth
        guard let merged = Self.string(from: full),
              await keychain.write(Self.liveService, NSUserName(), merged) else {
            return fail("write live", String(localized: "Schlüsselbund hat das Speichern abgelehnt", bundle: Bundle.app))
        }
        lastSwitch = Date()
        setActive(id)
        Self.log.notice("Account gewechselt")
        return true
    }

    /// Direkt vor dem Überschreiben live erneut lesen: hat die CLI inzwischen rotiert, den neueren Block in denselben
    /// Slot sichern. Liefert das aktuelle Live-JSON, nil bei Abbruch.
    private func rereadLive(since first: String, owner: String?) async -> [String: Any]? {
        guard let raw = await keychain.read(Self.liveService, nil), let full = Self.object(from: raw) else {
            fail("read live", String(localized: "Kein angemeldeter Claude-Account gefunden", bundle: Bundle.app))
            return nil
        }
        guard raw != first, let oauth = full["claudeAiOauth"] as? [String: Any] else { return full }
        if let owner {
            return await save(oauth, to: owner) ? full : nil
        }
        return await capture(oauth, fallbackActive: true) == nil ? nil : full
    }

    /// Den Live-Block in den Slot sichern, dem er gehört: per E-Mail, ohne E-Mail per gleichem Token, sonst (nur beim
    /// Wechsel) der aktive. Ohne Treffer wird er als neuer Account aufgenommen, damit nichts verloren geht.
    /// Liefert die Slot-Id, nil wenn das Sichern scheitert.
    private func capture(_ oauth: [String: Any], fallbackActive: Bool) async -> String? {
        let email = await (oauth["accessToken"] as? String).asyncMap(fetchEmail)
        var id = email.flatMap { e in accounts.first { $0.email == e }?.id }
        if email == nil { id = await slotMatching(oauth) }
        if id == nil, email == nil, fallbackActive, let activeId {
            Self.log.warning("Account: Identität unklar, sichere in den aktiven Slot")
            id = activeId
        }
        let slot = id ?? UUID().uuidString
        guard await save(oauth, to: slot) else { return nil }
        let sub = oauth["subscriptionType"] as? String
        if let i = accounts.firstIndex(where: { $0.id == slot }) {
            accounts[i].subscription = sub
        } else {
            let alias = email ?? String(localized: "Konto \(accounts.count + 1)", bundle: Bundle.app)
            accounts.append(Account(id: slot, alias: alias, email: email, subscription: sub))
            persist()
        }
        return slot
    }

    private func save(_ oauth: [String: Any], to slot: String) async -> Bool {
        guard let blob = Self.string(from: oauth), await keychain.write(Self.storeService, slot, blob) else {
            return fail("capture", String(localized: "Schlüsselbund hat das Speichern abgelehnt", bundle: Bundle.app))
        }
        return true
    }

    /// Slot, dessen gespeicherter Block denselben Refresh- oder Access-Token trägt. Unlesbare Slots zählen nicht.
    private func slotMatching(_ oauth: [String: Any]) async -> String? {
        for a in accounts {
            guard let raw = await keychain.read(Self.storeService, a.id), let stored = Self.object(from: raw) else { continue }
            if Self.sameToken(stored, oauth) { return a.id }
        }
        return nil
    }

    static func sameToken(_ a: [String: Any], _ b: [String: Any]) -> Bool {
        ["refreshToken", "accessToken"].contains { key in (a[key] as? String).map { $0 == b[key] as? String } ?? false }
    }

    /// Abbruch: Schritt loggen (ohne Werte), Text für den Nutzer merken.
    @discardableResult
    private func fail(_ step: String, _ message: String) -> Bool {
        Self.log.error("Account: Abbruch bei \(step, privacy: .public)")
        lastError = message
        return false
    }

    // MARK: Verwalten

    func remove(_ id: String) async {
        await keychain.delete(Self.storeService, id)
        accounts.removeAll { $0.id == id }
        if activeId == id { activeId = nil }
        persist()
    }

    private func setActive(_ id: String) {
        activeId = id
        persist()
    }

    // MARK: Auto-Wechsel

    /// Marge, um die ein Kandidat den aktiven Account unterbieten muss, damit gewechselt wird. Verhindert das
    /// Hin-und-Her, wenn zwei Accounts ähnlich voll sind.
    private static let hysteresis = 10

    /// Nach jedem Usage-Update: ist der aktive Account über der Schwelle und die Sperrzeit vorbei, auf den Account
    /// mit dem meisten Rest wechseln, aber nur wenn der spürbar leerer ist.
    func considerAutoSwitch(_ usage: Usage) {
        guard Settings.autoswitchEnabled, accounts.count > 1, let activeId,
              Date().timeIntervalSince(lastSwitch) >= Self.cooldown,
              let target = autoSwitchTarget(usage, activeId: activeId) else { return }
        Task {
            if await switchTo(target), let to = accounts.first(where: { $0.id == target }) {
                Feedback.play(.toggle)
                onAutoSwitch?(to)
            }
        }
    }

    /// Ziel des Auto-Wechsels: bei erreichter Füllstand-Schwelle der leerste Account. Zusätzlich, wenn `autoswitchOnPace`
    /// an ist und ein früher Wochen-Lockout droht, der Account mit spürbar mehr Wochen-Runway. Sonst nil (kein Wechsel).
    private func autoSwitchTarget(_ usage: Usage, activeId: String) -> String? {
        let used = max(usage.session ?? 0, usage.weekly ?? 0)
        if used >= Settings.autoswitchThreshold, let t = bestTarget(excluding: activeId, activeUsed: used) { return t }
        guard Settings.autoswitchOnPace, usage.weeklyLockout() != nil else { return nil }
        let headroom = (usage.weeklyPlan() ?? 100) - (usage.weekly ?? 0)
        return bestPaceTarget(excluding: activeId, activeHeadroom: headroom)
    }

    /// Der Account, auf den der Auto-Wechsel zielt, aus den echten Einstellungen und der aktuellen Zeit.
    private func bestTarget(excluding id: String, activeUsed: Int) -> String? {
        Self.bestTarget(among: accounts, activeId: id, activeUsed: activeUsed,
                        threshold: Settings.autoswitchThreshold, hysteresis: Self.hysteresis, now: Date())
    }

    /// Reine Zielwahl (testbar): unter den Accounts mit noch freiem 5-Stunden-Fenster der mit dem meisten
    /// 7-Tage-Pace-Vorrat, aber nur wenn er den aktiven um die Hysterese-Marge an Gesamtauslastung unterbietet.
    /// Sind alle etwa gleich voll oder alle beim 5-Stunden-Limit, kommt nil zurück (kein Toggeln, kein Leerlauf).
    static func bestTarget(among accounts: [Account], activeId: String, activeUsed: Int,
                           threshold: Int, hysteresis: Int, now: Date) -> String? {
        let candidates = accounts.filter { $0.id != activeId && sessionUsed($0, now: now) < threshold }
        guard let best = candidates.max(by: { paceHeadroom($0, now: now) < paceHeadroom($1, now: now) }),
              cachedUsed(best, now: now) <= activeUsed - hysteresis else { return nil }
        return best.id
    }

    /// Wie `bestTarget`, aber für den Pace-Fall: Ziel ist der Account mit dem meisten Wochen-Runway, gewählt nur wenn er
    /// den aktiven um die Hysterese-Marge an Pace-Vorrat übertrifft. So springt der Wechsel weg vom zu schnell brennenden
    /// Account, auch wenn dessen Füllstand die Schwelle noch nicht erreicht hat.
    private func bestPaceTarget(excluding id: String, activeHeadroom: Int) -> String? {
        Self.bestPaceTarget(among: accounts, activeId: id, activeHeadroom: activeHeadroom,
                            threshold: Settings.autoswitchThreshold, hysteresis: Self.hysteresis, now: Date())
    }

    static func bestPaceTarget(among accounts: [Account], activeId: String, activeHeadroom: Int,
                               threshold: Int, hysteresis: Int, now: Date) -> String? {
        let candidates = accounts.filter { $0.id != activeId && sessionUsed($0, now: now) < threshold }
        guard let best = candidates.max(by: { paceHeadroom($0, now: now) < paceHeadroom($1, now: now) }),
              paceHeadroom(best, now: now) >= activeHeadroom + hysteresis else { return nil }
        return best.id
    }

    /// 7-Tage-Pace-Vorrat: um wie viele Prozentpunkte der Account unter dem Soll-Stand liegt (Soll = verstrichener
    /// Wochenanteil). Höher = mehr Runway durch die Woche. Nie gesehen zählt als voller Vorrat.
    static func paceHeadroom(_ a: Account, now: Date) -> Int {
        guard let u = a.usage?.asUsage(at: now) else { return 100 }
        return (u.weeklyPlan(now: now) ?? 100) - (u.weekly ?? 0)
    }

    /// 5-Stunden-Auslastung, reset-bereinigt; nie gesehen zählt als leer.
    static func sessionUsed(_ a: Account, now: Date) -> Int { a.usage?.asUsage(at: now).session ?? 0 }

    /// Gecachte Gesamtauslastung (höherer von 5 h und 7 Tagen), reset-bereinigt; nie gesehen zählt als leer.
    static func cachedUsed(_ a: Account, now: Date) -> Int { a.usage?.used(at: now) ?? 0 }

    // MARK: JSON-Helfer (rein, testbar)

    /// Der `claudeAiOauth`-Block aus einem vollständigen Credential-JSON.
    static func oauth(from raw: String) -> [String: Any]? { object(from: raw)?["claudeAiOauth"] as? [String: Any] }

    static func object(from raw: String) -> [String: Any]? {
        (try? JSONSerialization.jsonObject(with: Data(raw.utf8))) as? [String: Any]
    }

    static func string(from object: [String: Any]) -> String? {
        (try? JSONSerialization.data(withJSONObject: object)).map { String(decoding: $0, as: UTF8.self) }
    }

    /// E-Mail des Accounts über `api.anthropic.com/api/oauth/profile`, für den Anzeigenamen. Scheitert still.
    nonisolated static func profileEmail(token: String) async -> String? {
        guard let url = URL(string: "https://api.anthropic.com/api/oauth/profile") else { return nil }
        var req = URLRequest(url: url, timeoutInterval: 15)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("oauth-2025-04-20", forHTTPHeaderField: "anthropic-beta")
        req.setValue("Kadrell", forHTTPHeaderField: "User-Agent")
        guard let (data, resp) = try? await URLSession.shared.data(for: req), (resp as? HTTPURLResponse)?.statusCode == 200,
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        let account = root["account"] as? [String: Any]
        return (account?["email"] as? String) ?? (account?["email_address"] as? String) ?? (root["email"] as? String)
    }
}

private extension Optional {
    func asyncMap<T>(_ f: (Wrapped) async -> T?) async -> T? {
        guard let self else { return nil }
        return await f(self)
    }
}
