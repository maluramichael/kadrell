import XCTest
@testable import Kadrell

/// Token-Refresh inaktiver Accounts: Request wie in der `claude`-CLI, Antwort wird in den gespeicherten Block gemischt.
final class AccountTokenTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_000_000)
    private func block(expiresIn seconds: Double) -> [String: Any] {
        ["accessToken": "old", "refreshToken": "r-old", "expiresAt": Int64((now.timeIntervalSince1970 + seconds) * 1000),
         "scopes": ["user:inference", "user:profile"], "subscriptionType": "max", "rateLimitTier": "tier"]
    }

    func testNeedsRefreshOnlyCloseToExpiry() {
        XCTAssertFalse(AccountToken.needsRefresh(block(expiresIn: 3600), now: now))
        XCTAssertTrue(AccountToken.needsRefresh(block(expiresIn: 60), now: now), "innerhalb der Marge")
        XCTAssertTrue(AccountToken.needsRefresh(block(expiresIn: -60), now: now), "schon abgelaufen")
        XCTAssertFalse(AccountToken.needsRefresh(["accessToken": "x"], now: now), "ohne expiresAt einfach versuchen")
    }

    func testRefreshRequestMatchesCLI() throws {
        let req = try XCTUnwrap(AccountToken.refreshRequest(block(expiresIn: 0)))
        XCTAssertEqual(req.url?.absoluteString, "https://platform.claude.com/v1/oauth/token")
        XCTAssertEqual(req.httpMethod, "POST")
        XCTAssertEqual(req.value(forHTTPHeaderField: "User-Agent"), "Kadrell")
        let body = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(req.httpBody)) as? [String: String])
        XCTAssertEqual(body, ["grant_type": "refresh_token", "refresh_token": "r-old",
                              "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e", "scope": "user:inference user:profile"])
    }

    func testRefreshRequestNeedsRefreshToken() {
        XCTAssertNil(AccountToken.refreshRequest(["accessToken": "x"]))
    }

    /// Neuer Access- und Refresh-Token samt Ablauf, Abo und Tier bleiben stehen.
    func testApplyingResponseKeepsOtherFields() throws {
        let response = Data(#"{"access_token":"new","refresh_token":"r-new","expires_in":28800,"refresh_token_expires_in":86400,"scope":"user:inference"}"#.utf8)
        let out = try XCTUnwrap(AccountToken.applying(response, to: block(expiresIn: 0), now: now))
        XCTAssertEqual(out["accessToken"] as? String, "new")
        XCTAssertEqual(out["refreshToken"] as? String, "r-new")
        XCTAssertEqual((out["expiresAt"] as? NSNumber)?.int64Value, Int64((now.timeIntervalSince1970 + 28800) * 1000))
        XCTAssertEqual((out["refreshTokenExpiresAt"] as? NSNumber)?.int64Value, Int64((now.timeIntervalSince1970 + 86400) * 1000))
        XCTAssertEqual(out["scopes"] as? [String], ["user:inference"])
        XCTAssertEqual(out["subscriptionType"] as? String, "max")
        XCTAssertEqual(out["rateLimitTier"] as? String, "tier")
        XCTAssertFalse(AccountToken.needsRefresh(out, now: now))
    }

    /// Ohne neuen Refresh-Token gilt wie in der CLI der alte weiter.
    func testApplyingKeepsRefreshTokenWhenMissing() throws {
        let response = Data(#"{"access_token":"new","expires_in":3600}"#.utf8)
        let out = try XCTUnwrap(AccountToken.applying(response, to: block(expiresIn: 0), now: now))
        XCTAssertEqual(out["refreshToken"] as? String, "r-old")
    }

    func testApplyingRejectsErrors() {
        XCTAssertNil(AccountToken.applying(Data(#"{"error":"invalid_grant"}"#.utf8), to: block(expiresIn: 0)))
        XCTAssertNil(AccountToken.applying(Data("kaputt".utf8), to: block(expiresIn: 0)))
    }
}
