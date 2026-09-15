import XCTest
@testable import Kadrell

final class LayoutTests: XCTestCase {
    func testColumnsFollowWidth() {
        XCTAssertEqual(Layout.columns(available: 700, min: 720, gap: 24), 1)
        XCTAssertEqual(Layout.columns(available: 1464, min: 720, gap: 24), 2)
        XCTAssertEqual(Layout.columns(available: 1463, min: 720, gap: 24), 1)
        XCTAssertEqual(Layout.columns(available: 652, min: 320, gap: 12), 2)
    }

    func testLODThresholds() {
        XCTAssertEqual(Layout.lod(cellScreenWidth: 59), 0)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 60), 1)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 139), 1)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 140), 2)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 319), 2)
        XCTAssertEqual(Layout.lod(cellScreenWidth: 320), 3)
    }

    func testGridLayout() {
        let groups = [Layout.GroupInput(id: "a", cellKeys: ["1", "2", "3"]), Layout.GroupInput(id: "b", cellKeys: ["4"])]
        let l = Layout.compute(groups: groups, worldWidth: 1600, scale: 1)
        // 1600 - 48 = 1552 → zwei Spalten à 764
        XCTAssertEqual(l.groups["a"]?.minX, 24)
        XCTAssertEqual(l.groups["a"]?.width ?? 0, 764, accuracy: 0.01)
        XCTAssertEqual(l.groups["b"]?.minX ?? 0, 24 + 764 + 24, accuracy: 0.01)
        // innen 732 → zwei Kachelspalten à 360, Höhe 225
        let c1 = l.cells["1"]!, c2 = l.cells["2"]!, c3 = l.cells["3"]!
        XCTAssertEqual(c1.width, 360, accuracy: 0.01)
        XCTAssertEqual(c1.height, 225, accuracy: 0.01)
        XCTAssertEqual(c2.minX, c1.maxX + 12, accuracy: 0.01)
        XCTAssertEqual(c3.minY, c1.maxY + 12, accuracy: 0.01)
        // Plus-Kachel nach der letzten Session
        XCTAssertEqual(l.plus["a"]!.minX, c2.minX, accuracy: 0.01)
        XCTAssertEqual(l.plus["a"]!.minY, c3.minY, accuracy: 0.01)
        XCTAssertGreaterThan(l.content.height, l.groups["a"]!.maxY)
        XCTAssertEqual(l.groupOrder, ["a", "b"])
    }

    func testHeaderIsScreenConstant() {
        let g = [Layout.GroupInput(id: "a", cellKeys: ["1"])]
        let near = Layout.compute(groups: g, worldWidth: 1600, scale: 1)
        let far = Layout.compute(groups: g, worldWidth: 1600, scale: 0.1)
        XCTAssertEqual(far.headers["a"]!.height, near.headers["a"]!.height * 10, accuracy: 0.01)
        XCTAssertGreaterThan(far.cells["1"]!.minY, near.cells["1"]!.minY)
    }

    func testFocusedCellTakesViewportAspect() {
        let g = [Layout.GroupInput(id: "a", cellKeys: ["1", "2"])]
        let l = Layout.compute(groups: g, worldWidth: 1600, scale: 1, focused: "1", viewportAspect: 2)
        XCTAssertEqual(l.cells["1"]!.height, l.cells["1"]!.width / 2, accuracy: 0.01)
        XCTAssertEqual(l.cells["2"]!.height, l.cells["2"]!.width / 1.6, accuracy: 0.01)
    }

    func testFit() {
        let (s, o) = Layout.fit(CGRect(x: 100, y: 50, width: 200, height: 100), in: CGSize(width: 1000, height: 1000), pad: 0)
        XCTAssertEqual(s, 5)
        XCTAssertEqual(o.x, (1000 - 1000) / 2 - 500)
        XCTAssertEqual(o.y, (1000 - 500) / 2 - 250)
        let (s2, _) = Layout.fit(CGRect(x: 0, y: 0, width: 10, height: 10), in: CGSize(width: 1000, height: 1000), pad: 0)
        XCTAssertEqual(s2, Layout.maxScale)
        let (s3, _) = Layout.fit(CGRect(x: 0, y: 0, width: 100000, height: 10), in: CGSize(width: 1000, height: 1000), pad: 12)
        XCTAssertEqual(s3, Layout.minScale)
    }
}

extension LayoutTests {
    func testWorldWidthGrowsWithGroupCount() {
        XCTAssertEqual(Layout.worldWidth(viewport: CGSize(width: 1400, height: 900), groupCount: 1), 1400)
        XCTAssertEqual(Layout.worldWidth(viewport: CGSize(width: 1400, height: 900), groupCount: 2), 2 * 744 - 24 + 48)
        // 30 Gruppen bei 16:10 → ceil(sqrt(30 · 0,45 · 1,56)) = 5 Spalten
        XCTAssertEqual(Layout.worldWidth(viewport: CGSize(width: 1400, height: 900), groupCount: 30), 5 * 744 - 24 + 48)
        // sehr breites Fenster: nie schmaler als das Fenster
        XCTAssertGreaterThanOrEqual(Layout.worldWidth(viewport: CGSize(width: 6000, height: 900), groupCount: 3), 6000)
    }
}
