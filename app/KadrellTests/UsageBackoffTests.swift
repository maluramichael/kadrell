import XCTest
@testable import Kadrell

@MainActor
final class UsageBackoffTests: XCTestCase {
    func testRetryAfterHonoredBeyondFifteenMinutes() {
        let d = UsageService.nextDelay(after: .rateLimited(1079), current: 300, interval: 300)
        XCTAssertEqual(d, 1079)
    }

    func testRetryAfterNeverShorterThanInterval() {
        XCTAssertEqual(UsageService.nextDelay(after: .rateLimited(10), current: 300, interval: 300), 300)
    }

    func testRetryAfterCappedAtTwoHours() {
        XCTAssertEqual(UsageService.nextDelay(after: .rateLimited(99_999), current: 300, interval: 300), 7200)
    }

    func testWithoutRetryAfterDoublesUpTo900() {
        XCTAssertEqual(UsageService.nextDelay(after: .rateLimited(nil), current: 300, interval: 300), 600)
        XCTAssertEqual(UsageService.nextDelay(after: .rateLimited(nil), current: 600, interval: 300), 900)
    }

    func testFailureDoublesAndSuccessResets() {
        XCTAssertEqual(UsageService.nextDelay(after: .failed, current: 600, interval: 300), 900)
        XCTAssertEqual(UsageService.nextDelay(after: .ok(.empty), current: 900, interval: 300), 300)
    }

    /// Nach einem Wechsel zeigt die Leiste den Stand des neuen Accounts, nicht den des alten.
    func testReplaceOverwritesPreviousAccountsUsage() {
        let s = UsageService()
        s.seed(Usage(session: nil, weekly: 99), at: Date())
        s.replace(Usage(session: nil, weekly: 12), at: nil)
        XCTAssertEqual(s.usage.weekly, 12)
        XCTAssertNil(s.measuredAt)
        s.replace(.empty, at: nil)
        XCTAssertEqual(s.usage, .empty)
    }
}
