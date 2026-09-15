import Foundation

enum LayoutMode: String, CaseIterable, Sendable {
    case grid, stack
    var other: LayoutMode { self == .grid ? .stack : .grid }
}

/// Reine Kachel-Mathematik der Arbeitsfläche, alles in Bildschirmpunkten.
enum Tiling {
    static let gap: CGFloat = 6
    /// Titelzeile einer Kachel (Grid) bzw. einer Stack-Zeile.
    static let rowHeight: CGFloat = 26

    enum Direction { case left, right, up, down }

    static func columns(for n: Int) -> Int { n <= 1 ? 1 : Int(Double(n).squareRoot().rounded(.up)) }

    /// Grid: n Kacheln in ceil(√n) Spalten, Zeilen von oben; Kanten auf ganze Punkte gerundet.
    static func grid(count n: Int, in b: CGRect) -> [CGRect] {
        guard n > 0 else { return [] }
        let cols = columns(for: n), rows = (n + cols - 1) / cols
        let w = (b.width - CGFloat(cols - 1) * gap) / CGFloat(cols)
        let h = (b.height - CGFloat(rows - 1) * gap) / CGFloat(rows)
        return (0..<n).map { i in
            let c = CGFloat(i % cols), r = CGFloat(i / cols)
            let x0 = (b.minX + c * (w + gap)).rounded(), x1 = (b.minX + c * (w + gap) + w).rounded()
            let y0 = (b.minY + r * (h + gap)).rounded(), y1 = (b.minY + r * (h + gap) + h).rounded()
            return CGRect(x: x0, y: y0, width: x1 - x0, height: y1 - y0)
        }
    }

    /// Stack (i3-Akkordeon): jede Kachel eine Titelzeile, die aktive bekommt ihren Körper direkt darunter,
    /// die Zeilen danach bleiben darunter.
    static func stack(count n: Int, active: Int, in b: CGRect, rowHeight: CGFloat = Tiling.rowHeight) -> (rows: [CGRect], body: CGRect) {
        var rows: [CGRect] = [], body = CGRect.zero
        var y = b.minY.rounded()
        let bodyH = max(0, b.height - CGFloat(n) * rowHeight).rounded()
        for i in 0..<n {
            rows.append(CGRect(x: b.minX, y: y, width: b.width, height: rowHeight))
            y += rowHeight
            if i == active { body = CGRect(x: b.minX, y: y, width: b.width, height: bodyH); y += bodyH }
        }
        return (rows, body)
    }

    /// Nachbar-Index für die Fokusbewegung; nil am Rand.
    static func neighbor(of i: Int, count n: Int, mode: LayoutMode, _ d: Direction) -> Int? {
        guard (0..<n).contains(i) else { return nil }
        let j: Int
        if mode == .stack {
            j = (d == .down || d == .right) ? i + 1 : i - 1
        } else {
            let cols = columns(for: n)
            switch d {
            case .left: j = i % cols == 0 ? -1 : i - 1
            case .right: j = i % cols == cols - 1 ? -1 : i + 1
            case .up: j = i - cols
            case .down: j = i + cols
            }
        }
        return (0..<n).contains(j) ? j : nil
    }
}
