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

    func testTemplatesFillInOrder() {
        // Haupt + Spalte: erste Kachel links über die volle Höhe, der Rest rechts untereinander.
        let main = Tiling.layout(.main, count: 3, in: b, gap: 0, ratios: { k, _ in k == "main.cols" ? [0.7, 0.3] : nil })
        XCTAssertEqual(main.frames[0], CGRect(x: 0, y: 0, width: 700, height: 600))
        XCTAssertEqual(main.frames[1], CGRect(x: 700, y: 0, width: 300, height: 300))
        XCTAssertEqual(main.frames[2].maxY, 600)
        XCTAssertEqual(main.dividers.map(\.key), ["main.cols", "main.rows.2"])
        // Spirale: halbiert abwechselnd senkrecht und waagerecht.
        let spiral = Tiling.layout(.spiral, count: 3, in: b, gap: 0).frames
        XCTAssertEqual(spiral[0], CGRect(x: 0, y: 0, width: 500, height: 600))
        XCTAssertEqual(spiral[1], CGRect(x: 500, y: 0, width: 500, height: 300))
        XCTAssertEqual(spiral[2], CGRect(x: 500, y: 300, width: 500, height: 300))
        // Grid mit fester Spaltenzahl und gezogenen Spaltenbreiten.
        let grid = Tiling.layout(.grid, count: 3, in: b, gap: 0, columns: 3, ratios: { k, _ in k == "grid.cols.3" ? [0.5, 0.25, 0.25] : nil })
        XCTAssertEqual(grid.frames.map(\.width), [500, 250, 250])
        // Falsche Länge gespeichert: gleich verteilt statt kaputt.
        let bad = Tiling.layout(.grid, count: 2, in: b, gap: 0, ratios: { _, _ in [1] })
        XCTAssertEqual(bad.frames.map(\.width), [500, 500])
    }

    func testDragMovesBoundaryAndClamps() {
        let d = Tiling.layout(.main, count: 2, in: b, gap: 10).dividers[0]
        XCTAssertTrue(d.vertical)
        let moved = Tiling.drag(d, to: CGPoint(x: 305, y: 0), gap: 10, ratios: [])
        XCTAssertEqual(moved[0], 300.0 / 990, accuracy: 0.001)
        XCTAssertEqual(moved.reduce(0, +), 1, accuracy: 0.0001)
        XCTAssertEqual(Tiling.drag(d, to: CGPoint(x: -500, y: 0), gap: 10, ratios: [])[0], 0.05, accuracy: 0.0001)
    }

    func testNeighborByFrames() {
        let f = Tiling.layout(.main, count: 3, in: b, gap: 0).frames   // 0 links, 1 rechts oben, 2 rechts unten
        XCTAssertEqual(Tiling.neighbor(of: 0, frames: f, .right), 1)
        XCTAssertEqual(Tiling.neighbor(of: 2, frames: f, .left), 0)
        XCTAssertEqual(Tiling.neighbor(of: 1, frames: f, .down), 2)
        XCTAssertNil(Tiling.neighbor(of: 0, frames: f, .left))
    }
}
