import XCTest
@testable import Kadrell

final class LayoutTests: XCTestCase {
    func testLODThresholds() {
        XCTAssertEqual(Layout.lod(cellScreenWidth: 59), 0)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 60), 1)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 139), 1)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 140), 2)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 319), 2)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 320), 3)
    }

    func testCellsFillGroupInColumns() {
        let g = [Layout.GroupInput(id: "a", cellKeys: ["1", "2", "3"], frame: CGRect(x: 100, y: 50, width: 760, height: 500))]
        let l = Layout.compute(groups: g, scale: 1, columns: 2)
        let c1 = l.cells["1"]!, c2 = l.cells["2"]!, c3 = l.cells["3"]!
        // innen: 760 - 24 - 3 (Farbbalken) = 733 breit → zwei Spalten à 361,5; zwei Zeilen
        XCTAssertEqual(c1.width, 361.5, accuracy: 0.01)
        XCTAssertEqual(c2.minX, c1.maxX + 10, accuracy: 0.01)
        XCTAssertEqual(c3.minY, c1.maxY + 10, accuracy: 0.01)
        XCTAssertEqual(c3.maxY, 50 + 500 - 18 - 12, accuracy: 0.01)   // Fußstreifen 18 px plus Rand
        XCTAssertEqual(c1.minY, 50 + 30 + 12, accuracy: 0.01)         // Kopfstreifen 30 px plus Rand
        XCTAssertEqual(c2.maxX, 100 + 760 - 12, accuracy: 0.01)
        XCTAssertEqual(l.groups["a"], g[0].frame)
        XCTAssertEqual(l.content, g[0].frame.insetBy(dx: -24, dy: -24))
        // eine Kachel: eine Spalte, füllt alles
        let single = Layout.compute(groups: [Layout.GroupInput(id: "a", cellKeys: ["1"], frame: g[0].frame)], scale: 1, columns: 3)
        XCTAssertEqual(single.cells["1"]!.width, 733, accuracy: 0.01)
    }

    func testHeaderIsScreenConstant() {
        let g = [Layout.GroupInput(id: "a", cellKeys: ["1"], frame: CGRect(x: 0, y: 0, width: 760, height: 500))]
        let near = Layout.compute(groups: g, scale: 1)
        let far = Layout.compute(groups: g, scale: 0.1)
        XCTAssertEqual(far.headers["a"]!.height, near.headers["a"]!.height * 10, accuracy: 0.01)
        XCTAssertGreaterThan(far.cells["1"]!.minY, near.cells["1"]!.minY)
    }

    func testFocusedCellTakesViewportAspect() {
        let g = [Layout.GroupInput(id: "a", cellKeys: ["1", "2"], frame: CGRect(x: 0, y: 0, width: 760, height: 500))]
        let l = Layout.compute(groups: g, scale: 1, focused: "1", viewportAspect: 2)
        XCTAssertEqual(l.cells["1"]!.width / l.cells["1"]!.height, 2, accuracy: 0.001)
        XCTAssertLessThanOrEqual(l.cells["1"]!.maxY, 500)
        XCTAssertEqual(l.cells["1"]!.width, 760, accuracy: 0.01)
    }

    func testPlaceNewGroup() {
        XCTAssertEqual(Layout.placeNewGroup(existing: []), CGRect(origin: .zero, size: Layout.defaultGroupSize))
        let a = CGRect(x: 0, y: 0, width: 760, height: 500)
        let b = Layout.placeNewGroup(existing: [a])
        XCTAssertEqual(b.minX, 784)
        XCTAssertFalse(b.intersects(a))
        let c = Layout.placeNewGroup(existing: [a, b], maxWidth: 1600)
        XCTAssertEqual(c.minY, 544)
        XCTAssertFalse(c.intersects(a) || c.intersects(b))
    }

    func testFit() {
        let (s, o) = Layout.fit(CGRect(x: 100, y: 50, width: 200, height: 100), in: CGSize(width: 1000, height: 1000), pad: 0)
        XCTAssertEqual(s, 5)
        XCTAssertEqual(o.x, (1000 - 1000) / 2 - 500)
        XCTAssertEqual(o.y, (1000 - 500) / 2 - 250)
        XCTAssertEqual(Layout.fit(CGRect(x: 0, y: 0, width: 10, height: 10), in: CGSize(width: 1000, height: 1000), pad: 0).scale, Layout.maxScale)
        XCTAssertEqual(Layout.fit(CGRect(x: 0, y: 0, width: 100000, height: 10), in: CGSize(width: 1000, height: 1000), pad: 12).scale, Layout.minScale)
    }
}
