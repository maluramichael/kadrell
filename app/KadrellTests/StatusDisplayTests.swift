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

    /// Reihenfolge, in der die rechte Seite der Leiste Platz schafft. Jeder Teil kostet 10, der kurze Kontoname 5,
    /// das Display 20 Text (Kurzform 10, `shrink` kürzt) plus 20 für Tasten, Lampe und Uhr.
    func testBarPlanDropOrder() {
        func width(_ p: BarPlan) -> CGFloat {
            let parts = p.extensions + p.usage.count + [p.update, p.version, p.tip].filter { $0 }.count
            let account: CGFloat = p.account ? (p.shortAccount ? 5 : 10) : 0
            return CGFloat(parts) * 10 + account + max(0, (p.compact ? 10 : 20) - p.shrink) + 20
        }
        func fit(_ room: CGFloat) -> BarPlan { BarPlan.fit(extensions: 2, room: room, width: width) }
        XCTAssertEqual(fit(130), BarPlan(extensions: 2))
        XCTAssertEqual(fit(120).extensions, 1)
        XCTAssertEqual(fit(110), BarPlan(extensions: 0))
        XCTAssertEqual(fit(100).usage, ["5h", "7d"])
        XCTAssertEqual(fit(90).usage, ["5h"])
        XCTAssertTrue(fit(85).shortAccount); XCTAssertTrue(fit(85).update)
        XCTAssertFalse(fit(75).update); XCTAssertTrue(fit(75).version)
        XCTAssertFalse(fit(65).version); XCTAssertTrue(fit(65).tip)
        XCTAssertFalse(fit(55).tip); XCTAssertEqual(fit(55).usage, ["5h"])
        XCTAssertEqual(fit(45).usage, []); XCTAssertFalse(fit(45).compact)
        XCTAssertTrue(fit(35).compact); XCTAssertEqual(fit(35).shrink, 0)
        XCTAssertEqual(fit(30).shrink, 5); XCTAssertTrue(fit(30).account)
        let last = fit(20)
        XCTAssertFalse(last.account); XCTAssertEqual(last.shrink, 10); XCTAssertEqual(width(last), 20)
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
