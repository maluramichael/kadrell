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
    static let liveService = "Claude Code-credentials"
    static let storeService = "de.malura.kadrell.account"
    /// Nach einem Auto-Wechsel so lange nicht erneut wechseln, gegen Flattern nahe der Schwelle.
    private static let cooldown: TimeInterval = 300

    private(set) var accounts: [Account] = []
    private(set) var activeId: String? { didSet { Settings.activeAccountId = activeId } }
    var onChange: (() -> Void)?
    /// Nach einem automatischen Wechsel aufgerufen, mit dem jetzt aktiven Account (für die Meldung an den Nutzer).
    var onAutoSwitch: ((Account) -> Void)?
    private var lastSwitch = Date.distantPast
    /// Ein Schlüsselbund-Vorgang läuft (Wechsel oder Token-Refresh). Alles Weitere wartet, sonst läse ein Wechsel einen
    /// halb geschriebenen Eintrag oder einen Refresh-Token, den der Refresh gerade verbraucht hat.
    private var busy = false
    /// Letzter Abfrageversuch je inaktivem Account, damit ein 429 oder ein toter Token nicht jede Minute neu anklopft.
    private var lastPoll: [String: Date] = [:]
    private var pollTask: Task<Void, Never>?
    /// Ein Auto-Wechsel prüft gerade seine Kandidaten, kein zweiter parallel.
    private var autoSwitching = false

    init() {
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
        guard let activeId else { return }
        store(u, for: activeId)
    }

    private func store(_ u: Usage, for id: String) {
        guard let i = accounts.firstIndex(where: { $0.id == id }) else { return }
        accounts[i].usage = CachedUsage(session: u.session, weekly: u.weekly, fable: u.fable,
                                        sessionResets: u.sessionResets, weeklyResets: u.weeklyResets, at: Date())
        persist()
    }

    private func persist() {
        Settings.accountsData = try? JSONEncoder().encode(accounts)
        onChange?()
    }

    // MARK: Hinzufügen

    /// Nimmt den gerade angemeldeten Account (aktueller `Claude Code-credentials`-Eintrag) als Slot auf. Ist er als
    /// E-Mail schon dabei, wird nur sein Block aufgefrischt und er aktiv gesetzt. Gibt false zurück, wenn kein
    /// angemeldeter Account gefunden wurde.
    func addCurrent() async -> Bool {
        guard let raw = await Keychain.read(service: Self.liveService),
              let oauth = Self.oauth(from: raw), let token = oauth["accessToken"] as? String else { return false }
        let email = await Self.profileEmail(token: token)
        let sub = oauth["subscriptionType"] as? String
        guard let blob = Self.string(from: oauth) else { return false }

        if let email, let i = accounts.firstIndex(where: { $0.email == email }) {
            let id = accounts[i].id
            guard await Keychain.write(service: Self.storeService, account: id, value: blob) else { return false }
            accounts[i].subscription = sub
            setActive(id)
            return true
        }
        let id = UUID().uuidString
        guard await Keychain.write(service: Self.storeService, account: id, value: blob) else { return false }
        let alias = email ?? String(localized: "Konto \(accounts.count + 1)", bundle: Bundle.app)
        accounts.append(Account(id: id, alias: alias, email: email, subscription: sub))
        setActive(id)
        return true
    }

    // MARK: Wechseln

    /// Schaltet den aktiven Account auf `id`. Der aktuell live liegende Block wird vorher in den Slot des bisher
    /// aktiven Accounts zurückgeschrieben (die `claude`-CLI rotiert die Tokens im Betrieb).
    @discardableResult
    func switchTo(_ id: String) async -> Bool {
        await exclusive { await swap(to: id) }
    }

    private func swap(to id: String) async -> Bool {
        guard id != activeId, accounts.contains(where: { $0.id == id }) else { return false }
        guard let raw = await Keychain.read(service: Self.liveService), var full = Self.object(from: raw) else { return false }
        await captureLive(full)
        guard let targetRaw = await Keychain.read(service: Self.storeService, account: id),
              let targetOauth = Self.object(from: targetRaw) else { return false }
        full["claudeAiOauth"] = targetOauth
        guard let merged = Self.string(from: full),
              await Keychain.write(service: Self.liveService, account: NSUserName(), value: merged) else { return false }
        lastSwitch = Date()
        setActive(id)
        Self.log.notice("Account gewechselt")
        return true
    }

    /// Den live liegenden `claudeAiOauth`-Block in den Slot des bisher aktiven Accounts sichern, damit dessen
    /// rotierter Refresh-Token nicht verloren geht.
    /// ponytail: schreibt immer in `activeId`. Meldet man sich außerhalb von Kadrell an einem anderen Account an
    /// und lässt dann wechseln, landet dessen Block im falschen Slot. Upgrade-Pfad: vor dem Sichern die E-Mail
    /// per `/oauth/profile` gegen den aktiven Slot prüfen.
    private func captureLive(_ full: [String: Any]) async {
        guard let curId = activeId, let oauth = full["claudeAiOauth"] as? [String: Any], let blob = Self.string(from: oauth) else { return }
        await Keychain.write(service: Self.storeService, account: curId, value: blob)
    }

    /// Schlüsselbund-Vorgänge nacheinander: wartet, bis der laufende fertig ist.
    private func exclusive<T>(_ body: () async -> T) async -> T {
        while busy { try? await Task.sleep(for: .milliseconds(50)) }
        busy = true
        defer { busy = false }
        return await body()
    }

    // MARK: Inaktive Accounts abfragen

    /// Inaktive Accounts im Hintergrund abfragen, damit ihr Stand stimmt, auch wenn sie anderswo (anderer Rechner,
    /// claude.ai) weiterlaufen. Geprüft wird jede Minute, abgefragt nur, was älter als `interval` ist.
    func startInactivePolling(interval: TimeInterval = 1200) {
        pollTask?.cancel()
        pollTask = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refreshInactive(olderThan: interval)
                try? await Task.sleep(for: .seconds(60))
            }
        }
    }

    /// Nutzung jedes inaktiven Accounts, dessen Stand (oder letzter Versuch) älter als `maxAge` ist, mit dessen eigenem
    /// Token abfragen. Scheitert eine Abfrage, bleibt der alte Stand mit seinem alten Zeitpunkt stehen.
    func refreshInactive(olderThan maxAge: TimeInterval) async {
        let now = Date()
        let due = accounts.filter { $0.id != activeId && Self.isDue($0, lastPoll: lastPoll[$0.id], maxAge: maxAge, now: now) }
        if !due.isEmpty { Self.log.notice("inaktive Accounts abfragen: \(due.count, privacy: .public)") }
        for id in due.map(\.id) {
            lastPoll[id] = now
            await pollInactive(id)
        }
    }

    private func pollInactive(_ id: String) async {
        guard let token = await exclusive({ await slotToken(id) }) else {
            Self.log.warning("inaktiver Account: kein gültiger Token")
            return
        }
        var result = await UsageService.fetch(token: token)
        // Das 429 hängt am Access-Token, nicht am Account: den alten Token des Slots fragen womöglich noch andere
        // Clients ab, ein frisch erneuerter kommt sofort durch. Deshalb einmal mit neuem Token nachfassen.
        if case .rateLimited = result, let renewed = await exclusive({ await slotToken(id, renew: true) }) {
            result = await UsageService.fetch(token: renewed)
        }
        switch result {
        case .ok(let u): if id != activeId { store(u, for: id) }
        case .rateLimited: Self.log.warning("inaktiver Account: 429, nächster Versuch im nächsten Intervall")
        case .failed: break   // `fetch` meldet den Grund selbst
        }
    }

    /// Fällig, wenn weder ein Stand noch ein Versuch jünger als `maxAge` ist.
    static func isDue(_ a: Account, lastPoll: Date?, maxAge: TimeInterval, now: Date) -> Bool {
        let last = max(a.usage?.at ?? .distantPast, lastPoll ?? .distantPast)
        return now.timeIntervalSince(last) >= maxAge
    }

    /// Access-Token eines inaktiven Slots; ist er (fast) abgelaufen oder `renew` gesetzt, wird er erneuert und der neue
    /// Block sofort zurückgeschrieben. Nur innerhalb von `exclusive` aufrufen.
    private func slotToken(_ id: String, renew: Bool = false) async -> String? {
        guard id != activeId, let raw = await Keychain.read(service: Self.storeService, account: id),
              var oauth = Self.object(from: raw) else { return nil }
        if renew || AccountToken.needsRefresh(oauth) {
            // Temp-Profile teilen den Schlüsselbund mit dem eigenen Kadrell: dort nie rotieren, sonst verbraucht die
            // Testinstanz den Refresh-Token, den das eigene Kadrell beim nächsten Wechsel braucht.
            guard !Profile.isTemporary, let req = AccountToken.refreshRequest(oauth), let data = await AccountToken.send(req),
                  let renewed = AccountToken.applying(data, to: oauth), let blob = Self.string(from: renewed) else { return nil }
            guard await Keychain.write(service: Self.storeService, account: id, value: blob) else {
                Self.log.error("Token erneuert, aber nicht gespeichert")
                return nil
            }
            oauth = renewed
        }
        return oauth["accessToken"] as? String
    }

    // MARK: Verwalten

    func remove(_ id: String) async {
        await Keychain.delete(service: Self.storeService, account: id)
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

    /// So jung muss der Stand eines Ziels sein, damit der Auto-Wechsel ihm traut. Älteres wird vorher live abgefragt.
    private static let verifyAge: TimeInterval = 120

    /// Nach jedem Usage-Update: ist der aktive Account über der Schwelle und die Sperrzeit vorbei, auf den Account
    /// mit dem meisten Rest wechseln, aber nur wenn der spürbar leerer ist. Die Kandidaten werden vorher live
    /// abgefragt; scheitert das, wird nicht gewechselt (ein alter Stand kann längst voll sein).
    func considerAutoSwitch(_ usage: Usage) {
        guard Settings.autoswitchEnabled, accounts.count > 1, let activeId, !autoSwitching,
              Date().timeIntervalSince(lastSwitch) >= Self.cooldown,
              autoSwitchTarget(usage, activeId: activeId) != nil else { return }
        autoSwitching = true
        Task {
            defer { autoSwitching = false }
            await refreshInactive(olderThan: Self.verifyAge)
            guard self.activeId == activeId, let target = verifiedTarget(usage, activeId: activeId) else { return }
            if await switchTo(target), let to = accounts.first(where: { $0.id == target }) {
                Feedback.play(.toggle)
                onAutoSwitch?(to)
            }
        }
    }

    /// Das Ziel nach der Live-Abfrage, aber nur, wenn sein Stand dabei wirklich frisch geworden ist.
    private func verifiedTarget(_ usage: Usage, activeId: String) -> String? {
        guard let target = autoSwitchTarget(usage, activeId: activeId) else { return nil }
        guard let at = accounts.first(where: { $0.id == target })?.usage?.at, Date().timeIntervalSince(at) <= Self.verifyAge * 2 else {
            Self.log.notice("Auto-Wechsel verworfen: Ziel nicht live bestätigt")
            return nil
        }
        return target
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
    static func profileEmail(token: String) async -> String? {
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
