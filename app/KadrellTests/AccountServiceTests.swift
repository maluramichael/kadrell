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
}
