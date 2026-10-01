import XCTest
@testable import Kadrell

@MainActor
final class AccountServiceTests: XCTestCase {
    /// Vollständiges Credential-JSON mit MCP-Logins und einem Account-Block.
    private func live(access: String, mcp: String) -> String {
        """
        {"mcpOAuth":{"srv":{"accessToken":"\(mcp)"}},"claudeAiOauth":{"accessToken":"\(access)","refreshToken":"r-\(access)","subscriptionType":"max"}}
        """
    }

    func testOauthExtractionReadsClaudeBlock() {
        let oauth = AccountService.oauth(from: live(access: "A", mcp: "M"))
        XCTAssertEqual(oauth?["accessToken"] as? String, "A")
        XCTAssertEqual(oauth?["subscriptionType"] as? String, "max")
    }

    /// Der Kern des Global-Swaps: nur `claudeAiOauth` wird getauscht, `mcpOAuth` (account-unabhängig) bleibt stehen.
    func testSwapReplacesClaudeBlockButKeepsMcp() throws {
        var full = try XCTUnwrap(AccountService.object(from: live(access: "A", mcp: "M")))
        let targetOauth = try XCTUnwrap(AccountService.oauth(from: live(access: "B", mcp: "ignored")))
        full["claudeAiOauth"] = targetOauth
        let merged = try XCTUnwrap(AccountService.string(from: full))
        let back = try XCTUnwrap(AccountService.object(from: merged))

        XCTAssertEqual((back["claudeAiOauth"] as? [String: Any])?["accessToken"] as? String, "B", "Account-Block getauscht")
        let mcp = (back["mcpOAuth"] as? [String: Any])?["srv"] as? [String: Any]
        XCTAssertEqual(mcp?["accessToken"] as? String, "M", "MCP-Logins unangetastet")
    }

    func testObjectFromGarbageIsNil() {
        XCTAssertNil(AccountService.object(from: "kein json"))
        XCTAssertNil(AccountService.oauth(from: "{}"))
    }

    // MARK: Auto-Wechsel: 7d-bewusste Zielwahl

    private let now = Date(timeIntervalSince1970: 1_000_000)
    /// Account mit gesetztem Stand; Fenster resetten in der Zukunft, damit die Werte nicht reset-bereinigt auf 0 fallen.
    private func acct(_ id: String, session: Int?, weekly: Int?, weeklyResetsIn hours: Double) -> Account {
        Account(id: id, alias: id, email: nil, subscription: nil,
                usage: CachedUsage(session: session, weekly: weekly, fable: nil,
                                   sessionResets: now.addingTimeInterval(3600),
                                   weeklyResets: now.addingTimeInterval(hours * 3600), at: now))
    }
    /// Reset in 84 h ⇒ Wochen-Soll (weeklyPlan) = 50 %.
    private func target(_ accounts: [Account], activeUsed: Int) -> String? {
        AccountService.bestTarget(among: accounts, activeId: "live", activeUsed: activeUsed,
                                  threshold: 90, hysteresis: 10, now: now)
    }

    /// Bei gleichem Reset zählt nicht die niedrigste Auslastung, sondern der größte Abstand zum Wochen-Soll.
    func testTargetPrefersMoreWeeklyPaceHeadroom() {
        let accounts = [acct("live", session: 95, weekly: 30, weeklyResetsIn: 84),
                        acct("x", session: 10, weekly: 10, weeklyResetsIn: 84),   // Soll 50, Vorrat 40
                        acct("y", session: 10, weekly: 45, weeklyResetsIn: 84)]   // Soll 50, Vorrat 5
        XCTAssertEqual(target(accounts, activeUsed: 95), "x")
    }

    /// Ein Account mit mehr 7d-Vorrat, aber vollem 5-Stunden-Fenster, bringt sofort nichts und wird übersprungen.
    func testTargetSkipsAccountsWithFullSession() {
        let accounts = [acct("live", session: 95, weekly: 30, weeklyResetsIn: 84),
                        acct("full5h", session: 99, weekly: 5, weeklyResetsIn: 84),  // bester 7d-Vorrat, aber 5h voll
                        acct("free", session: 10, weekly: 40, weeklyResetsIn: 84)]
        XCTAssertEqual(target(accounts, activeUsed: 95), "free")
    }

    /// Ist der beste Kandidat kaum leerer als der aktive, bleibt es beim aktiven (Hysterese, kein Flattern).
    func testTargetNilWhenNoRealImprovement() {
        let accounts = [acct("live", session: 95, weekly: 30, weeklyResetsIn: 84),
                        acct("almostfull", session: 88, weekly: 30, weeklyResetsIn: 84)] // used 88 > 95-10
        XCTAssertNil(target(accounts, activeUsed: 95))
    }

    /// Pace-Ziel: Soll (weeklyPlan) = 50 % bei Reset in 84 h.
    private func paceTarget(_ accounts: [Account], activeHeadroom: Int) -> String? {
        AccountService.bestPaceTarget(among: accounts, activeId: "live", activeHeadroom: activeHeadroom,
                                      threshold: 90, hysteresis: 10, now: now)
    }

    /// Aktiver Account brennt über Plan (negativer Vorrat), ein Account mit klar mehr Wochen-Vorrat wird gewählt,
    /// obwohl dessen Füllstand die Schwelle nie erreicht.
    func testPaceTargetSwitchesToMoreRunway() {
        let live = acct("live", session: 40, weekly: 60, weeklyResetsIn: 84)
        let accounts = [live, acct("x", session: 10, weekly: 10, weeklyResetsIn: 84)]
        XCTAssertEqual(paceTarget(accounts, activeHeadroom: AccountService.paceHeadroom(live, now: now)), "x")
    }

    /// Ist der beste Kandidat nicht spürbar entspannter als der aktive, bleibt es beim aktiven (kein Flattern).
    func testPaceTargetNilWhenNoRealImprovement() {
        let live = acct("live", session: 40, weekly: 60, weeklyResetsIn: 84)
        let accounts = [live, acct("barely", session: 10, weekly: 55, weeklyResetsIn: 84)]
        XCTAssertNil(paceTarget(accounts, activeHeadroom: AccountService.paceHeadroom(live, now: now)))
    }

    /// Ein noch nie gesehener Account (kein Stand) gilt als leer mit vollem Vorrat und kommt zuerst dran.
    func testTargetPrefersFreshAccount() {
        let fresh = Account(id: "fresh", alias: "fresh", email: nil, subscription: nil, usage: nil)
        let accounts = [acct("live", session: 95, weekly: 30, weeklyResetsIn: 84),
                        acct("seen", session: 10, weekly: 45, weeklyResetsIn: 84), fresh]
        XCTAssertEqual(target(accounts, activeUsed: 95), "fresh")
    }

    /// Seed für die Leiste: abgelaufene Fenster kommen reset-bereinigt als 0 zurück, laufende behalten ihren Wert.
    func testAsUsageAppliesResets() {
        let now = Date()
        let cached = CachedUsage(session: 18, weekly: 60, fable: 19,
                                 sessionResets: now.addingTimeInterval(-60),   // 5h schon zurückgesetzt
                                 weeklyResets: now.addingTimeInterval(3600),    // 7d/Fable noch offen
                                 at: now)
        let u = cached.asUsage(at: now)
        XCTAssertEqual(u.session, 0, "abgelaufenes 5h-Fenster ist frei")
        XCTAssertEqual(u.weekly, 60, "laufendes 7d-Fenster behält den Wert")
        XCTAssertEqual(u.fable, 19, "Fable folgt dem 7d-Reset, hier noch offen")
    }
    // MARK: Wechsel und Aufnahme gegen einen Fake-Schlüsselbund

    private var savedAccounts: Data?
    private var savedActive: String?

    override func setUp() async throws {
        savedAccounts = Settings.accountsData
        savedActive = Settings.activeAccountId
        Settings.accountsData = nil
        Settings.activeAccountId = nil
    }

    override func tearDown() async throws {
        Settings.accountsData = savedAccounts
        Settings.activeAccountId = savedActive
    }

    private func block(_ access: String) -> String {
        #"{"accessToken":"\#(access)","refreshToken":"r-\#(access)","subscriptionType":"max"}"#
    }

    /// Service mit vorbelegten Accounts (id = alias), aktiv `active`, E-Mails je accessToken aus `emails`.
    private func service(_ fake: FakeKeychain, accounts: [(String, String?)], active: String?,
                         emails: [String: String] = [:]) -> AccountService {
        let list = accounts.map { Account(id: $0.0, alias: $0.0, email: $0.1, subscription: nil) }
        Settings.accountsData = try? JSONEncoder().encode(list)
        Settings.activeAccountId = active
        return AccountService(keychain: fake.access, profileEmail: { emails[$0] })
    }

    /// Scheitert das Sichern des Live-Blocks, wird nicht getauscht: live bleibt, Fehlertext steht.
    func testSwitchAbortsWhenCaptureFails() async {
        let fake = FakeKeychain()
        fake.set(AccountService.liveService, live(access: "A", mcp: "M"))
        fake.set(AccountService.storeService, "B", block("B"))
        fake.failWritesTo = AccountService.storeService
        let s = service(fake, accounts: [("A", "a@x"), ("B", "b@x")], active: "A", emails: ["A": "a@x"])

        let ok = await s.switchTo("B")
        XCTAssertFalse(ok)
        XCTAssertEqual(fake.get(AccountService.liveService), live(access: "A", mcp: "M"), "live unverändert")
        XCTAssertNotNil(s.lastError)
        XCTAssertEqual(s.active?.id, "A")
    }

    /// Hat sich außerhalb von Kadrell B angemeldet, landet B nicht im Slot des aktiven A.
    func testSwitchCapturesLiveIntoOwnerNotActive() async throws {
        let fake = FakeKeychain()
        fake.set(AccountService.liveService, live(access: "B", mcp: "M"))
        fake.set(AccountService.storeService, "A", block("A"))
        fake.set(AccountService.storeService, "C", block("C"))
        let s = service(fake, accounts: [("A", "a@x"), ("C", "c@x")], active: "A", emails: ["B": "b@x"])

        let ok = await s.switchTo("C")
        XCTAssertTrue(ok)
        XCTAssertNil(s.lastError)
        XCTAssertEqual(fake.get(AccountService.storeService, "A"), block("A"), "Slot A byte-gleich")
        let b = try XCTUnwrap(s.accounts.first { $0.email == "b@x" }, "B als neuer Account aufgenommen")
        XCTAssertEqual(AccountService.object(from: fake.get(AccountService.storeService, b.id) ?? "")?["accessToken"] as? String, "B")
        XCTAssertEqual(AccountService.oauth(from: fake.get(AccountService.liveService) ?? "")?["accessToken"] as? String, "C")
        let mcp = (AccountService.object(from: fake.get(AccountService.liveService) ?? "")?["mcpOAuth"] as? [String: Any])?["srv"]
        XCTAssertEqual((mcp as? [String: Any])?["accessToken"] as? String, "M", "MCP-Logins unangetastet")
    }

    /// Ohne E-Mail wird über den Token dedupliziert: zweimal aufnehmen ergibt einen Account.
    func testAddCurrentWithoutEmailDeduplicatesByToken() async {
        let fake = FakeKeychain()
        fake.set(AccountService.liveService, live(access: "A", mcp: "M"))
        let s = service(fake, accounts: [], active: nil)

        let first = await s.addCurrent()
        let second = await s.addCurrent()
        XCTAssertTrue(first && second)
        XCTAssertEqual(s.accounts.count, 1)
        XCTAssertEqual(s.active?.id, s.accounts.first?.id)
    }

    /// Rotiert die CLI den Token zwischen erstem Lesen und Überschreiben, landet der neuere Block im Slot.
    func testSwitchSavesRotatedLiveBlock() async {
        let fake = FakeKeychain()
        fake.set(AccountService.liveService, live(access: "A1", mcp: "M"))
        fake.liveQueue = [live(access: "A1", mcp: "M"), live(access: "A2", mcp: "M")]
        fake.set(AccountService.storeService, "A", block("A0"))
        fake.set(AccountService.storeService, "C", block("C"))
        let s = service(fake, accounts: [("A", "a@x"), ("C", "c@x")], active: "A", emails: ["A1": "a@x"])

        let ok = await s.switchTo("C")
        XCTAssertTrue(ok)
        XCTAssertEqual(AccountService.object(from: fake.get(AccountService.storeService, "A") ?? "")?["accessToken"] as? String, "A2")
        XCTAssertEqual(AccountService.oauth(from: fake.get(AccountService.liveService) ?? "")?["accessToken"] as? String, "C")
    }

    // MARK: UsageService

    func testUnknownUsageFormatFailsInsteadOfEmpty() {
        guard case .failed = UsageService.result(for: Data(#"{"foo":{"bar":1}}"#.utf8)) else { return XCTFail("kein .failed") }
        guard case .ok(let u) = UsageService.result(for: Data(#"{"five_hour":{"utilization":12}}"#.utf8)) else { return XCTFail("kein .ok") }
        XCTAssertEqual(u.session, 12)
    }

    func testFailedFetchDoesNotCallOnChange() async {
        let s = UsageService(fetcher: { UsageService.result(for: Data("{}".utf8)) })
        var calls = 0
        s.onChange = { _ in calls += 1 }
        s.refreshNow()
        try? await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(calls, 0)
    }

    /// Ein Poll, der vor dem Wechsel mit dem alten Token begann, darf den neuen Stand nicht überschreiben.
    func testStalePollResultIsDiscarded() async {
        let fake = FakeFetch()
        let s = UsageService(fetcher: { await fake.fetch() })
        var seen: [Usage] = []
        s.onChange = { seen.append($0) }
        s.start(interval: 3600)
        await until { fake.pending.count == 1 }
        s.refreshNow()
        await until { fake.pending.count == 2 }
        let fresh = Usage(session: 5), old = Usage(session: 90)
        fake.pending[1].resume(returning: .ok(fresh))
        await until { seen.count == 1 }
        fake.pending[0].resume(returning: .ok(old))
        try? await Task.sleep(for: .milliseconds(100))
        XCTAssertEqual(seen, [fresh])
        XCTAssertEqual(s.usage, fresh)
    }

    private func until(_ cond: () -> Bool) async {
        for _ in 0..<200 where !cond() { try? await Task.sleep(for: .milliseconds(10)) }
    }
}

/// In-Memory-Schlüsselbund; `liveQueue` liefert nacheinander wechselnde Live-Werte (Token-Rotation).
final class FakeKeychain: @unchecked Sendable {
    private let lock = NSLock()
    private var items: [String: String] = [:]
    var liveQueue: [String] = []
    var failWritesTo: String?

    private func key(_ service: String, _ account: String) -> String { service + "\u{0}" + account }
    func set(_ service: String, _ account: String, _ value: String) { lock.withLock { items[key(service, account)] = value } }
    func set(_ service: String, _ value: String) { set(service, NSUserName(), value) }
    func get(_ service: String, _ account: String = NSUserName()) -> String? { lock.withLock { items[key(service, account)] } }

    var access: KeychainAccess {
        KeychainAccess(
            read: { service, account in self.read(service, account) },
            write: { service, account, value in
                self.lock.withLock {
                    guard self.failWritesTo != service else { return false }
                    self.items[self.key(service, account)] = value
                    return true
                }
            },
            delete: { service, account in _ = self.lock.withLock { self.items.removeValue(forKey: self.key(service, account)) } })
    }

    private func read(_ service: String, _ account: String?) -> String? {
        lock.withLock {
            if service == AccountService.liveService, !liveQueue.isEmpty { return liveQueue.removeFirst() }
            if let account { return items[key(service, account)] }
            return items.first { $0.key.hasPrefix(service + "\u{0}") }?.value
        }
    }
}

@MainActor
final class FakeFetch {
    var pending: [CheckedContinuation<UsageService.Result, Never>] = []
    func fetch() async -> UsageService.Result { await withCheckedContinuation { pending.append($0) } }
}
