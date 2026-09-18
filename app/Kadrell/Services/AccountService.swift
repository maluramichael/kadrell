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
    var at: Date
    /// Bindender Wert für den Auto-Wechsel: der höhere von 5 h und 7 Tagen.
    var used: Int { max(session ?? 0, weekly ?? 0) }
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
    private var lastSwitch = Date.distantPast
    /// Ein Wechsel läuft: kein zweiter parallel (der zweite läse den halb geschriebenen Eintrag).
    private var busy = false

    init() {
        if let data = Settings.accountsData, let list = try? JSONDecoder().decode([Account].self, from: data) { accounts = list }
        activeId = Settings.activeAccountId
    }

    var active: Account? { accounts.first { $0.id == activeId } }
    /// Für die Leiste: je Account Kürzel, Markierung des aktiven und der zwischengespeicherte Nutzungsstand.
    var menuItems: [(id: String, title: String, active: Bool, detail: String)] {
        accounts.map { ($0.id, $0.title, $0.id == activeId, Self.usageDetail($0.usage)) }
    }

    /// „5h 45% · 7d 56% · vor 12 min“ aus dem gecachten Stand, leer wenn nie ein Stand gemerkt wurde.
    static func usageDetail(_ u: CachedUsage?) -> String {
        guard let u else { return "" }
        var parts: [String] = []
        if let s = u.session { parts.append("5h \(s)%") }
        if let w = u.weekly { parts.append("7d \(w)%") }
        guard !parts.isEmpty else { return "" }
        let mins = max(0, Int(Date().timeIntervalSince(u.at) / 60))
        parts.append(mins < 1 ? String(localized: "gerade eben", bundle: Bundle.app) : String(localized: "vor \(mins) min", bundle: Bundle.app))
        return parts.joined(separator: " · ")
    }

    /// Nutzung des aktiven Accounts aus dem Poll merken, damit das Menü sie später auch für den inaktiven zeigt.
    func recordUsage(_ u: Usage) {
        guard let activeId, let i = accounts.firstIndex(where: { $0.id == activeId }) else { return }
        accounts[i].usage = CachedUsage(session: u.session, weekly: u.weekly, fable: u.fable, at: Date())
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
        guard !busy, id != activeId, accounts.contains(where: { $0.id == id }) else { return false }
        busy = true
        defer { busy = false }
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

    /// Nach jedem Usage-Update: ist der aktive Account über der Schwelle und die Sperrzeit vorbei, auf den Account
    /// mit dem meisten Rest wechseln, aber nur wenn der spürbar leerer ist.
    func considerAutoSwitch(_ usage: Usage) {
        guard Settings.autoswitchEnabled, accounts.count > 1, let activeId else { return }
        let used = max(usage.session ?? 0, usage.weekly ?? 0)
        guard used >= Settings.autoswitchThreshold, Date().timeIntervalSince(lastSwitch) >= Self.cooldown,
              let target = bestTarget(excluding: activeId, activeUsed: used) else { return }
        Task { if await switchTo(target) { Feedback.play(.toggle) } }
    }

    /// Der Account mit dem meisten Rest laut gecachtem Stand, aber nur wenn er den aktiven um die Hysterese-Marge
    /// schlägt. Sind alle etwa gleich voll, kommt nil zurück und es bleibt beim aktiven (kein Toggeln).
    private func bestTarget(excluding id: String, activeUsed: Int) -> String? {
        guard let best = accounts.filter({ $0.id != id }).min(by: { cachedUsed($0) < cachedUsed($1) }),
              cachedUsed(best) <= activeUsed - Self.hysteresis else { return nil }
        return best.id
    }

    /// Gecachte Auslastung; nie gesehen zählt als leer, damit ein frischer Account zuerst drankommt.
    private func cachedUsed(_ a: Account) -> Int { a.usage?.used ?? 0 }

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
