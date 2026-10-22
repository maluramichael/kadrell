import XCTest
@testable import Kadrell

@MainActor
final class StatusDisplayTests: XCTestCase {
    override func tearDown() {
        Profile.defaults.removeObject(forKey: Settings.languageKey)
        Localization.apply(.system)
        super.tearDown()
    }

    func testOnlyNonZeroStatesInOrder() {
        Settings.language = .de
        XCTAssertEqual(StatusDisplay.text(StatusCounts(running: 2, waiting: 1, idle: 3, error: 0, detached: 1), compact: false), "2 ARBEITEN · 1 WARTET")
        XCTAssertEqual(StatusDisplay.text(StatusCounts(running: 1, waiting: 0, idle: 0, error: 2, detached: 0), compact: false), "1 ARBEITET · 2 FEHLER")
        XCTAssertEqual(StatusDisplay.text(StatusCounts(), compact: false), "")
        XCTAssertEqual(StatusDisplay.text(StatusCounts(running: 2, waiting: 1, idle: 0, error: 0, detached: 0), compact: true), "2A · 1W")
    }

    func testLampPriority() {
        XCTAssertEqual(StatusDisplay.lamp(StatusCounts(running: 1, waiting: 1, idle: 0, error: 1, detached: 0)), .waiting)
        XCTAssertEqual(StatusDisplay.lamp(StatusCounts(running: 1, waiting: 0, idle: 0, error: 1, detached: 0)), .error)
        XCTAssertEqual(StatusDisplay.lamp(StatusCounts(running: 0, waiting: 0, idle: 2, error: 0, detached: 3)), .ok)
        XCTAssertEqual(StatusDisplay.lamp(StatusCounts(running: 0, waiting: 0, idle: 0, error: 0, detached: 3)), .off)
    }

    func testDisplaySwitchesLanguage() {
        let c = StatusCounts(running: 2, waiting: 1, idle: 0, error: 1, detached: 0)
        Settings.language = .en
        XCTAssertEqual(StatusDisplay.text(c, compact: false), "2 RUNNING · 1 WAITING · 1 ERROR")
        XCTAssertEqual(StatusDisplay.text(c, compact: true), "2R · 1W · 1E")
        Settings.language = .de
        XCTAssertEqual(StatusDisplay.text(c, compact: false), "2 ARBEITEN · 1 WARTET · 1 FEHLER")
        XCTAssertEqual(StatusDisplay.text(c, compact: true), "2A · 1W · 1F")
    }

    func testEnglishPlural() {
        Settings.language = .en
        XCTAssertEqual(StatusDisplay.text(StatusCounts(running: 1, waiting: 2, idle: 0, error: 2, detached: 0), compact: false), "1 RUNNING · 2 WAITING · 2 ERRORS")
    }

    func testSpokenListsEverything() {
        Settings.language = .de
        XCTAssertEqual(StatusDisplay.spoken(StatusCounts(running: 2, waiting: 1, idle: 3, error: 0, detached: 1)), "2 arbeiten, 1 warten, 3 fertig, 1 getrennt · 7 Sessions")
        XCTAssertEqual(StatusDisplay.spoken(StatusCounts(running: 0, waiting: 0, idle: 1, error: 2, detached: 0)), "0 arbeiten, 0 warten, 1 fertig, 2 mit Fehler, 0 getrennt · 3 Sessions")
    }
}
