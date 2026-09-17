import XCTest
@testable import Kadrell

/// Gruppierung aus: eine flache Liste aller Sessions, mit eigener Handreihenfolge neben der aus `groups.json`.
@MainActor
final class SidebarFlatTests: XCTestCase {
    private func session(_ id: String, cwd: String, started: Double = 0, status: String = "idle") -> Session {
        var s = Session(id: id, cwd: cwd, startedAt: started, sessionId: id, name: id)
        s.rawStatus = status
        return s
    }

    private func fixture() -> (groups: [Group], sessions: [String: Session]) {
        let list = [session("alt", cwd: "/x", started: 1_000, status: "busy"),
                    session("neu", cwd: "/x", started: 9_000, status: "busy"),
                    session("fertig", cwd: "/y", started: 5_000, status: "idle"),
                    session("wartet", cwd: "/y", started: 2_000, status: "waiting")]
        let groups = [Group(id: "1", name: "x", color: "", cwd: "/x", sessionIds: ["alt", "neu"]),
                      Group(id: "2", name: "y", color: "", cwd: "/y", sessionIds: ["fertig", "wartet"])]
        return (groups, Dictionary(uniqueKeysWithValues: list.map { ($0.id, $0) }))
    }

    /// Gleicher Status: die neuere Session steht oben, in der Gruppe wie zwischen den Gruppen.
    func testStatusSortPutsNewestFirstWithinSameStatus() {
        let (groups, sessions) = fixture()
        let sorted = SidebarSort.status.apply(groups, sessions: sessions)
        XCTAssertEqual(sorted.map(\.id), ["2", "1"], "wartende Gruppe vor der laufenden")
        XCTAssertEqual(sorted.first?.sessionIds, ["wartet", "fertig"])
        XCTAssertEqual(sorted.last?.sessionIds, ["neu", "alt"], "beide laufen, die neuere zuerst")
    }

    /// Gleicher Status in beiden Gruppen: die Gruppe mit der neueren Session steht oben.
    func testStatusSortRanksGroupsByNewestSessionOnTies() {
        var (groups, sessions) = fixture()
        sessions["fertig"]?.rawStatus = "busy"
        sessions["wartet"]?.rawStatus = "busy"
        XCTAssertEqual(SidebarSort.status.apply(groups, sessions: sessions).map(\.id), ["1", "2"],
                       "Gruppe 1 hat mit 'neu' die jüngste Session")
        groups.swapAt(0, 1)
        XCTAssertEqual(SidebarSort.status.apply(groups, sessions: sessions).map(\.id), ["1", "2"],
                       "unabhängig von der Reihenfolge in groups.json")
    }

    /// Handreihenfolge der flachen Liste: gespeicherte Ordnung gilt, Unbekanntes kommt oben dazu (neueste zuerst).
    func testFlatHandOrderUsesSavedOrderAndPutsNewcomersOnTop() {
        let (groups, sessions) = fixture()
        let flat = SidebarFlat.rows(groups, sessions: sessions, sort: .off, order: ["fertig", "alt"])
        XCTAssertEqual(flat.map(\.session.id), ["neu", "wartet", "fertig", "alt"])
        XCTAssertEqual(flat.map(\.group.id), ["1", "2", "2", "1"], "jede Zeile kennt weiter ihre Gruppe")
    }

    /// Sortierung greift auch flach, über Gruppengrenzen hinweg, und ignoriert die gespeicherte Ordnung.
    func testFlatSortIgnoresSavedOrder() {
        let (groups, sessions) = fixture()
        let order = ["fertig", "alt", "neu", "wartet"]
        XCTAssertEqual(SidebarFlat.rows(groups, sessions: sessions, sort: .status, order: order).map(\.session.id),
                       ["wartet", "neu", "alt", "fertig"])
        XCTAssertEqual(SidebarFlat.rows(groups, sessions: sessions, sort: .alpha, order: order).map(\.session.id),
                       ["alt", "fertig", "neu", "wartet"])
    }

    /// Der Baum zeigt flach alle Sessions ohne Gruppenzeilen, auch aus eingeklappten Gruppen.
    func testTreeShowsEverySessionWithoutGroupRows() {
        let (groups, sessions) = fixture()
        let tree = SidebarView(frame: .zero)
        tree.grouped = false
        tree.flatOrder = ["alt", "neu", "fertig", "wartet"]
        tree.reload(groups: groups, sessions: Array(sessions.values))
        XCTAssertEqual(tree.sessionIds, ["alt", "neu", "fertig", "wartet"])
        XCTAssertEqual(tree.rowCount, 4, "keine Gruppenzeilen")
        tree.toggleAllGroups()
        XCTAssertEqual(tree.sessionIds, ["alt", "neu", "fertig", "wartet"], "Einklappen bleibt folgenlos")
    }

    /// Ziehen in der flachen Liste meldet die ganze neue Reihenfolge und fasst `groups.json` nicht an.
    func testFlatReorderReportsFullOrderAndLeavesGroupsAlone() {
        let (groups, sessions) = fixture()
        let tree = SidebarView(frame: .zero)
        tree.grouped = false
        tree.flatOrder = ["alt", "neu", "fertig", "wartet"]
        tree.reload(groups: groups, sessions: Array(sessions.values))
        var reported: [String] = []
        tree.onReorderFlat = { reported = $0 }
        tree.onMoveSession = { _, _ in XCTFail("flach wird nie in groups.json sortiert") }
        tree.moveFlat("wartet", to: "neu")
        XCTAssertEqual(reported, ["alt", "wartet", "neu", "fertig"])
        XCTAssertEqual(groups.map(\.sessionIds), [["alt", "neu"], ["fertig", "wartet"]])
    }

    /// Gruppiert bleibt alles wie bisher: Gruppenzeilen, Handreihenfolge aus `groups.json`.
    func testGroupedTreeIsUnchanged() {
        let (groups, sessions) = fixture()
        let tree = SidebarView(frame: .zero)
        tree.flatOrder = ["wartet", "fertig", "neu", "alt"]
        tree.reload(groups: groups, sessions: Array(sessions.values))
        XCTAssertEqual(tree.sessionIds, ["alt", "neu", "fertig", "wartet"])
        XCTAssertEqual(tree.rowCount, 6, "zwei Gruppenzeilen dazu")
    }
}
