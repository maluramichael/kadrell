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
