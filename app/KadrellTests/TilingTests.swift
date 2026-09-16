import XCTest
@testable import Kadrell

final class TilingTests: XCTestCase {
    let b = CGRect(x: 0, y: 0, width: 1000, height: 600)

    func testGridColumnsAndFill() {
        XCTAssertEqual(Tiling.grid(count: 0, in: b), [])
        XCTAssertEqual(Tiling.grid(count: 1, in: b), [b])
        let five = Tiling.grid(count: 5, in: b)   // 3 Spalten, 2 Zeilen
        XCTAssertEqual(five.count, 5)
        XCTAssertEqual(five[0].minX, 0); XCTAssertEqual(five[2].maxX, 1000)
        XCTAssertEqual(five[1].minX, five[0].maxX + Tiling.gap)
        XCTAssertEqual(five[3].minY, five[0].maxY + Tiling.gap)
        XCTAssertEqual(five[3].minX, 0); XCTAssertEqual(five[4].maxY, 600)
        for r in five { XCTAssertEqual(r.minX, r.minX.rounded()); XCTAssertEqual(r.width, r.width.rounded()) }
        let wide = Tiling.grid(count: 2, in: b, gap: 40)
        XCTAssertEqual(wide[1].minX, wide[0].maxX + 40); XCTAssertEqual(wide[1].maxX, 1000)
    }

    func testStackAccordionKeepsRowsBelowActive() {
        let (rows, body) = Tiling.stack(count: 4, active: 1, in: b)
        XCTAssertEqual(rows.count, 4)
        XCTAssertEqual(rows[0].minY, 0)
        XCTAssertEqual(rows[1].minY, Tiling.rowHeight)
        XCTAssertEqual(body.minY, rows[1].maxY)                 // Körper direkt unter der aktiven Zeile
        XCTAssertEqual(rows[2].minY, body.maxY)                 // die Zeilen danach bleiben darunter
        XCTAssertEqual(rows[3].maxY, 600)
        XCTAssertEqual(body.height, 600 - 4 * Tiling.rowHeight)
    }

    func testNeighbor() {
        // 5 Kacheln im Grid: 3 Spalten → Indizes 0 1 2 / 3 4
        XCTAssertEqual(Tiling.neighbor(of: 0, count: 5, mode: .grid, .right), 1)
        XCTAssertNil(Tiling.neighbor(of: 2, count: 5, mode: .grid, .right))
        XCTAssertNil(Tiling.neighbor(of: 3, count: 5, mode: .grid, .left))
        XCTAssertEqual(Tiling.neighbor(of: 1, count: 5, mode: .grid, .down), 4)
        XCTAssertNil(Tiling.neighbor(of: 2, count: 5, mode: .grid, .down))
        XCTAssertEqual(Tiling.neighbor(of: 4, count: 5, mode: .grid, .up), 1)
        XCTAssertEqual(Tiling.neighbor(of: 1, count: 3, mode: .stack, .down), 2)
        XCTAssertNil(Tiling.neighbor(of: 0, count: 3, mode: .stack, .up))
        XCTAssertNil(Tiling.neighbor(of: 7, count: 3, mode: .grid, .up))
    }
}
