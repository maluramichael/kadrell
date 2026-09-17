import XCTest
@testable import Kadrell

/// Nur die reine Entscheidung, kein echter UNUserNotificationCenter-Zugriff (bräuchte eine signierte App-Bundle-Identität).
@MainActor
final class NotificationsTests: XCTestCase {
    func testOffNeverNotifies() {
        XCTAssertFalse(Notifications.shouldNotify(level: .off, waiting: true))
        XCTAssertFalse(Notifications.shouldNotify(level: .off, waiting: false))
    }

    func testWaitingLevelOnlyWaiting() {
        XCTAssertTrue(Notifications.shouldNotify(level: .waiting, waiting: true))
        XCTAssertFalse(Notifications.shouldNotify(level: .waiting, waiting: false))
    }

    func testAllLevelNotifiesBoth() {
        XCTAssertTrue(Notifications.shouldNotify(level: .all, waiting: true))
        XCTAssertTrue(Notifications.shouldNotify(level: .all, waiting: false))
    }
}
