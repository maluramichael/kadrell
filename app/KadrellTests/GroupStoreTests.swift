import XCTest
@testable import Kadrell

@MainActor
final class GroupStoreTests: XCTestCase {
    nonisolated(unsafe) var url: URL!

    override func setUp() {
        url = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)/groups.json")
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: url.deletingLastPathComponent())
    }

    func session(_ id: String, cwd: String) -> Session {
        Session(id: id, cwd: cwd, startedAt: 0, sessionId: id, name: id)
    }

    func testAutoAssignByCwdAndPersistence() throws {
        let store = GroupStore(url: url)
        XCTAssertTrue(store.assign([session("s1", cwd: "/p/homelab"), session("s2", cwd: "/p/homelab"), session("s3", cwd: "/p/malura")]))
        XCTAssertEqual(store.groups.count, 2)
        XCTAssertEqual(store.groups[0].name, "homelab")
        XCTAssertEqual(store.groups[0].sessionIds, ["s1", "s2"])
        XCTAssertEqual(store.groups[1].name, "malura")
        XCTAssertEqual(store.groups[0].color, Theme.palette[0])
        XCTAssertEqual(store.groups[1].color, Theme.palette[1])
        XCTAssertFalse(store.assign([session("s1", cwd: "/p/homelab"), session("s2", cwd: "/p/homelab"), session("s3", cwd: "/p/malura")]))

        let reloaded = GroupStore(url: url)
        XCTAssertEqual(reloaded.groups, store.groups)
    }

    func testEditSurvivesReloadAndSessionKeepsGroup() throws {
        let store = GroupStore(url: url)
        store.assign([session("s1", cwd: "/p/homelab")])
        var g = store.groups[0]
        g.name = "Heim"; g.color = "#cba6f7"; g.cwd = "/p/other"
        store.update(g)
        XCTAssertEqual(GroupStore(url: url).groups[0].name, "Heim")
        // Session bleibt trotz geändertem Gruppen-cwd in ihrer Gruppe, keine neue Gruppe
        XCTAssertFalse(store.assign([session("s1", cwd: "/p/homelab")]))
        XCTAssertEqual(store.groups.count, 1)
    }

    func testMoveSessionAndGroupPersists() throws {
        let store = GroupStore(url: url)
        store.assign([session("s1", cwd: "/p/a"), session("s2", cwd: "/p/a"), session("s3", cwd: "/p/a"), session("t1", cwd: "/p/b")])
        store.moveSession("s1", to: "s3")
        XCTAssertEqual(store.groups[0].sessionIds, ["s2", "s3", "s1"])
        store.moveSession("s1", to: "s2")
        XCTAssertEqual(store.groups[0].sessionIds, ["s1", "s2", "s3"])
        // über Gruppengrenzen verschiebt moveSession nichts
        store.moveSession("s1", to: "t1")
        XCTAssertEqual(store.groups[0].sessionIds, ["s1", "s2", "s3"])
        store.moveGroup(store.groups[1].id, to: store.groups[0].id)
        XCTAssertEqual(store.groups.map(\.cwd), ["/p/b", "/p/a"])
        XCTAssertEqual(GroupStore(url: url).groups, store.groups)
        // Reihenfolge übersteht den nächsten Poll
        XCTAssertFalse(store.assign([session("s1", cwd: "/p/a"), session("s2", cwd: "/p/a"), session("s3", cwd: "/p/a"), session("t1", cwd: "/p/b")]))
    }

    func testPrunesVanishedSessionsAndRemovesGroups() throws {
        let store = GroupStore(url: url)
        store.assign([session("s1", cwd: "/p/a"), session("s2", cwd: "/p/a")])
        XCTAssertTrue(store.assign([session("s2", cwd: "/p/a")]))
        XCTAssertEqual(store.groups[0].sessionIds, ["s2"])
        // ohne Sessions verschwindet die Gruppe
        XCTAssertTrue(store.assign([]))
        XCTAssertTrue(store.groups.isEmpty)
        XCTAssertTrue(GroupStore(url: url).groups.isEmpty)
    }
}
