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
        store.assign([session("s1", cwd: "/p/a"), session("s2", cwd: "/p/a"), session("t1", cwd: "/p/b")])
        XCTAssertTrue(store.assign([session("s2", cwd: "/p/a"), session("t1", cwd: "/p/b")]))
        XCTAssertEqual(store.groups[0].sessionIds, ["s2"])
        // letzte Session einer unveränderten Gruppe verschwindet: die Gruppe fliegt raus
        XCTAssertTrue(store.assign([session("t1", cwd: "/p/b")]))
        XCTAssertEqual(store.groups.map(\.cwd), ["/p/b"])
        XCTAssertEqual(GroupStore(url: url).groups.map(\.cwd), ["/p/b"])
    }

    func testEmptyPollNeitherPrunesNorRemovesGroups() throws {
        let store = GroupStore(url: url)
        store.assign([session("s1", cwd: "/p/a")])
        let before = store.groups
        // Eine leere Sessions-Liste ist eher ein Aussetzer der CLI als "alle Sessions weg"
        XCTAssertFalse(store.assign([]))
        XCTAssertEqual(store.groups, before)
        XCTAssertEqual(GroupStore(url: url).groups, before)
    }

    func testManuallyEditedEmptyGroupSurvivesPruning() throws {
        let store = GroupStore(url: url)
        store.assign([session("s1", cwd: "/p/a"), session("t1", cwd: "/p/b")])
        var g = store.groups.first { $0.cwd == "/p/a" }!
        g.name = "Mein Projekt"
        store.update(g)
        // letzte Session weg, Gruppe ist aber von Hand umbenannt: bleibt leer stehen statt zu verschwinden
        XCTAssertTrue(store.assign([session("t1", cwd: "/p/b")]))
        XCTAssertEqual(store.groups.map(\.cwd).sorted(), ["/p/a", "/p/b"])
        let kept = store.groups.first { $0.cwd == "/p/a" }!
        XCTAssertTrue(kept.sessionIds.isEmpty)
        XCTAssertEqual(kept.name, "Mein Projekt")
        // eine neue Session im selben cwd landet wieder in der alten Gruppe
        XCTAssertTrue(store.assign([session("t1", cwd: "/p/b"), session("s2", cwd: "/p/a")]))
        XCTAssertEqual(store.groups.first { $0.cwd == "/p/a" }!.sessionIds, ["s2"])
    }

    func testFavoriteSurvivesEmptyGroup() throws {
        let store = GroupStore(url: url)
        store.assign([session("s1", cwd: "/p/a"), session("t1", cwd: "/p/b")])
        store.toggleFavorite(id: store.groups[0].id)
        // s1 und t1 sind weg, nur eine Session in /p/c kommt neu rein: die favorisierte /p/a-Gruppe
        // bleibt trotzdem leer stehen, /p/b (nicht favorisiert, unverändert) fliegt raus
        XCTAssertTrue(store.assign([session("x1", cwd: "/p/c")]))
        XCTAssertEqual(store.groups.map(\.cwd).sorted(), ["/p/a", "/p/c"])
        XCTAssertTrue(store.group(forCwd: "/p/a")!.sessionIds.isEmpty)
        XCTAssertTrue(GroupStore(url: url).group(forCwd: "/p/a")!.isFavorite)
        // Wieder abgewählt, verschwindet die leere Gruppe beim nächsten Poll mit Sessions
        store.toggleFavorite(id: store.group(forCwd: "/p/a")!.id)
        XCTAssertTrue(store.assign([session("x1", cwd: "/p/c")]))
        XCTAssertEqual(store.groups.map(\.cwd), ["/p/c"])
    }

    func testSidebarSortAlphaAndStatusKeepHandOrderOnTies() {
        var s = ["b": session("b", cwd: "/x"), "a": session("a", cwd: "/x"), "c": session("c", cwd: "/y"), "d": session("d", cwd: "/y")]
        s["a"]!.rawStatus = "idle"; s["b"]!.rawStatus = "busy"; s["c"]!.rawStatus = "idle"; s["d"]!.rawStatus = "idle"
        let groups = [Group(id: "1", name: "zeta", color: "", cwd: "/x", sessionIds: ["b", "a"]),
                      Group(id: "2", name: "Alpha", color: "", cwd: "/y", sessionIds: ["d", "c"])]
        XCTAssertEqual(SidebarSort.off.apply(groups, sessions: s), groups)
        let alpha = SidebarSort.alpha.apply(groups, sessions: s)
        XCTAssertEqual(alpha.map(\.id), ["2", "1"])
        XCTAssertEqual(alpha.map(\.sessionIds), [["c", "d"], ["a", "b"]])
        s["c"]!.rawStatus = "waiting"
        let status = SidebarSort.status.apply(groups, sessions: s)
        XCTAssertEqual(status.map(\.id), ["2", "1"])
        XCTAssertEqual(status.map(\.sessionIds), [["c", "d"], ["b", "a"]])

        let tree = SidebarView(frame: .zero)
        tree.sort = .alpha
        tree.reload(groups: groups, sessions: Array(s.values))
        XCTAssertEqual(tree.sessionId(after: nil, step: 1), "c")
    }
}

@MainActor
final class GroupStoreRemoteTests: XCTestCase {
    func testRemoteSessionsGroupByHostNotCwd() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)/groups.json")
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let store = GroupStore(url: url)
        let home = NSHomeDirectory()
        var r1 = Session(id: "ssh-1", cwd: home, startedAt: 0, sessionId: "ssh-1", name: ""); r1.host = "examplehost"
        var r2 = Session(id: "ssh-2", cwd: home, startedAt: 0, sessionId: "ssh-2", name: ""); r2.host = "examplehost"; r2.tmuxSession = "test"
        let local = Session(id: "shell-1", cwd: home, startedAt: 0, sessionId: "shell-1", name: "")
        store.assign([r1, local, r2])
        XCTAssertEqual(store.groups.map(\.name), ["examplehost", URL(fileURLWithPath: home).lastPathComponent])
        XCTAssertEqual(store.groups[0].sessionIds, ["ssh-1", "ssh-2"])
        XCTAssertEqual(store.groups[0].host, "examplehost")
        XCTAssertNil(store.groups[1].host)
        XCTAssertEqual(r1.title, "examplehost")
        XCTAssertEqual(r2.title, "test")
        let data = try JSONEncoder().encode([r2])
        XCTAssertEqual(try JSONDecoder().decode([Session].self, from: data).first?.tmuxSession, "test")
        XCTAssertEqual(GroupStore(url: url).groups, store.groups)
    }
}
