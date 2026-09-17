import XCTest
@testable import Kadrell

final class StatusMenuTests: XCTestCase {
    func testRowsSplitWaitingAndUnseenDone() {
        let order = ["a", "b", "c", "d"]
        let waiting: Set<String> = ["b", "d"]
        let unseen: Set<String> = ["a", "d"]   // "d" ist wartend UND unseen: zählt nur als wartend.
        let rows = StatusMenu.rows(order: order, waiting: waiting, unseen: unseen)
        XCTAssertEqual(rows.waiting, ["b", "d"])
        XCTAssertEqual(rows.done, ["a"])
    }

    func testRowsEmptyWithoutWaitingOrUnseen() {
        let rows = StatusMenu.rows(order: ["a"], waiting: [], unseen: [])
        XCTAssertTrue(rows.waiting.isEmpty)
        XCTAssertTrue(rows.done.isEmpty)
    }

    func testTitleFormatsCounts() {
        XCTAssertEqual(StatusMenu.title(waiting: 0, done: 0), "")
        XCTAssertFalse(StatusMenu.title(waiting: 2, done: 1).isEmpty)
    }
}
