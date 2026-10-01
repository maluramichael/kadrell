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

    @MainActor
    private func store() -> GroupStore {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-statusmenu-\(UUID().uuidString)/groups.json")
        addTeardownBlock { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        return GroupStore(url: url)
    }

    /// `update` setzt nur den Titel, das Menü entsteht erst beim Öffnen, mit wartenden und neuen Zeilen wie bisher.
    @MainActor
    func testMenuIsBuiltOnOpenNotOnUpdate() {
        let store = store()
        var waiting = Session(id: "w", cwd: "/p/a", startedAt: 0, sessionId: "w", name: "Wartet")
        waiting.rawStatus = "waiting"
        let done = Session(id: "d", cwd: "/p/a", startedAt: 0, sessionId: "d", name: "Fertig")
        store.assign([waiting, done])
        let c = StatusItemController(store: store)
        c.update([waiting, done], unseen: ["d"])
        XCTAssertTrue(c.menu.items.isEmpty, "update baut kein Menü")
        c.menuNeedsUpdate(c.menu)
        let titles = c.menu.items.map(\.title)
        XCTAssertEqual(titles.count, 6)
        XCTAssertTrue(titles[0].hasPrefix("a › Wartet · "))
        XCTAssertTrue(titles[1].hasPrefix("a › Fertig · "))
        XCTAssertTrue(c.menu.items[2].isSeparatorItem)
        XCTAssertEqual(titles[3], String(localized: "Nächste wartende Session", bundle: Bundle.app))
        XCTAssertTrue(c.menu.items[3].isEnabled)
        XCTAssertTrue(c.menu.items[4].isSeparatorItem)
        XCTAssertEqual(titles[5], String(localized: "Kadrell öffnen", bundle: Bundle.app))
        XCTAssertEqual(c.menu.items[0].representedObject as? String, "w")

        c.update([], unseen: [])
        c.menuNeedsUpdate(c.menu)
        XCTAssertEqual(c.menu.items.map(\.title), [String(localized: "Keine wartenden Sessions", bundle: Bundle.app), "", String(localized: "Kadrell öffnen", bundle: Bundle.app)])
    }

    /// Doppelte Session-Id (z. B. kurzzeitig doppelt gemeldet) darf nicht abstürzen, die erste gewinnt.
    @MainActor
    func testDuplicateSessionIdDoesNotCrash() {
        let store = store()
        var s = Session(id: "x", cwd: "/p/a", startedAt: 0, sessionId: "x", name: "Erste")
        s.rawStatus = "waiting"
        store.assign([s])
        var dup = s
        dup.name = "Zweite"
        let c = StatusItemController(store: store)
        c.update([s, dup], unseen: [])
        c.menuNeedsUpdate(c.menu)
        XCTAssertTrue(c.menu.items[0].title.hasPrefix("a › Erste · "))
    }
}
