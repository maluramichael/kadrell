import Foundation

enum LayoutMode: String, CaseIterable, Sendable {
    case grid, main, spiral, custom, scroll, stack
    /// Reihum wie tmux `next-layout`.
    var next: LayoutMode { Self.allCases[(Self.allCases.firstIndex(of: self)! + 1) % Self.allCases.count] }
    var title: String {
        switch self {
        case .grid: String(localized: "Grid")
        case .main: String(localized: "Haupt + Spalte")
        case .spiral: String(localized: "Spirale")
        case .custom: String(localized: "Frei")
        case .scroll: String(localized: "Scrollen")
        case .stack: String(localized: "Stack")
        }
    }
}

/// Ziehbare Grenze einer Vorlage: gehört zur Verhältnisliste `key` mit `count` Feldern, liegt hinter Feld `index`.
struct SplitLine: Equatable {
    let key: String
    let index: Int
    let count: Int
    /// Senkrechte Linie, wird waagerecht gezogen.
    let vertical: Bool
    /// Griffzone, mindestens 8 pt breit, auch bei Abstand 0.
    let rect: CGRect
    /// Fläche, die diese Liste aufteilt.
    let span: CGRect
}

/// Reine Kachel-Mathematik der Arbeitsfläche, alles in Bildschirmpunkten.
enum Tiling {
    /// Titelzeile einer Kachel (Grid) bzw. einer Stack-Zeile.
    static let rowHeight: CGFloat = 26

    enum Direction { case left, right, up, down }

    static func columns(for n: Int) -> Int { n <= 1 ? 1 : Int(Double(n).squareRoot().rounded(.up)) }

    /// Verhältnisse einer Vorlage, nil = gleich verteilt.
    typealias Ratios = (_ key: String, _ count: Int) -> [Double]?

    /// Die Vorlage des Layouts für n Kacheln: Felder in Reihenfolge und ihre ziehbaren Grenzen. Die Terminals werden
    /// nur eingefüllt, die Größen gehören der Vorlage (`ratios`), nicht einzelnen Sessions. Stack hat keine Vorlage.
    /// `columns` 0 = Grid wählt ⌈√n⌉ Spalten. `splits`: Frei, Zeichen i teilt Feld i „r“ rechts oder „d“ unten.
    static func layout(_ mode: LayoutMode, count n: Int, in b: CGRect, gap: CGFloat, columns fixed: Int = 0, splits: String = "",
                       ratios: @escaping Ratios = { _, _ in nil }) -> (frames: [CGRect], dividers: [SplitLine]) {
        guard n > 0 else { return ([], []) }
        var s = Splitter(gap: gap, ratios: ratios)
        let frames = switch mode {
        case .grid, .stack: s.grid(n, in: b, columns: fixed)
        case .main: s.main(n, in: b)
        case .spiral, .custom: s.chain(n, in: b, mode: mode, splits: Array(splits))
        case .scroll: scroll(n, in: b, gap: gap, widths: ratios("scroll.widths", n) ?? [])
        }
        return (frames, s.dividers)
    }

    /// Teilt Flächen nach den Verhältnissen der Vorlage und sammelt dabei die ziehbaren Grenzen.
    private struct Splitter {
        let gap: CGFloat
        let ratios: Ratios
        var dividers: [SplitLine] = []

        /// Gespeicherte Verhältnisse normiert, ungültige oder fehlende gleich verteilt.
        func normalized(_ key: String, _ count: Int) -> [Double] {
            guard let v = ratios(key, count), v.count == count, v.allSatisfy({ $0 > 0 }) else { return Array(repeating: 1 / Double(count), count: count) }
            let sum = v.reduce(0, +)
            return v.map { $0 / sum }
        }

        /// Teilt `area` entlang einer Achse nach `key` und merkt sich die Grenzen.
        mutating func split(_ area: CGRect, vertical: Bool, key: String, count: Int) -> [CGRect] {
            let parts = segments(start: vertical ? area.minX : area.minY, length: vertical ? area.width : area.height, gap: gap, ratios: normalized(key, count))
            for i in 0..<(count - 1) {
                let a = parts[i].end, z = parts[i + 1].start, mid = (a + z) / 2, w = max(z - a, 8)
                let grip = vertical ? CGRect(x: mid - w / 2, y: area.minY, width: w, height: area.height)
                                    : CGRect(x: area.minX, y: mid - w / 2, width: area.width, height: w)
                dividers.append(SplitLine(key: key, index: i, count: count, vertical: vertical, rect: grip, span: area))
            }
            return parts.map { vertical ? CGRect(x: $0.start, y: area.minY, width: $0.end - $0.start, height: area.height)
                                        : CGRect(x: area.minX, y: $0.start, width: area.width, height: $0.end - $0.start) }
        }

        mutating func grid(_ n: Int, in b: CGRect, columns fixed: Int) -> [CGRect] {
            let cols = fixed > 0 ? min(fixed, n) : columns(for: n), rows = (n + cols - 1) / cols
            let xs = cols > 1 ? split(b, vertical: true, key: "grid.cols.\(cols)", count: cols) : [b]
            let ys = rows > 1 ? split(b, vertical: false, key: "grid.rows.\(rows)", count: rows) : [b]
            return (0..<n).map { CGRect(x: xs[$0 % cols].minX, y: ys[$0 / cols].minY, width: xs[$0 % cols].width, height: ys[$0 / cols].height) }
        }

        mutating func main(_ n: Int, in b: CGRect) -> [CGRect] {
            guard n > 1 else { return [b] }
            let cols = split(b, vertical: true, key: "main.cols", count: 2)
            let rest = n > 2 ? split(cols[1], vertical: false, key: "main.rows.\(n - 1)", count: n - 1) : [cols[1]]
            return [cols[0]] + rest
        }

        /// Kette: jede Kachel teilt den Rest. Spirale (bspwm) abwechselnd senkrecht und waagerecht. Frei (i3/bspwm-Insert):
        /// Kachel i+1 teilt Feld i rechts oder unten, ohne Vorgabe entlang der längeren Seite.
        /// ponytail: geteilt wird immer der Rest (Kette), kein Baum; 2×2 geht so nicht, dafür gibt es das Grid.
        mutating func chain(_ n: Int, in b: CGRect, mode: LayoutMode, splits dirs: [Character]) -> [CGRect] {
            var area = b, frames: [CGRect] = []
            for i in 0..<(n - 1) {
                let given = dirs.indices.contains(i) && dirs[i] != "a" ? dirs[i] == "r" : area.width >= area.height
                let parts = split(area, vertical: mode == .spiral ? i % 2 == 0 : given, key: "\(mode.rawValue).\(i)", count: 2)
                frames.append(parts[0])
                area = parts[1]
            }
            return frames + [area]
        }
    }

    /// niri: Spalten fester Breite (Anteil der Fläche) nebeneinander, was nicht passt, ragt rechts hinaus.
    private static func scroll(_ n: Int, in b: CGRect, gap: CGFloat, widths v: [Double]) -> [CGRect] {
        var x = b.minX, frames: [CGRect] = []
        for i in 0..<n {
            let w = ((b.width + gap) * CGFloat(min(max(v.indices.contains(i) ? v[i] : 0.5, 0.1), 1)) - gap).rounded()
            frames.append(CGRect(x: x, y: b.minY, width: w, height: b.height))
            x += w + gap
        }
        return frames
    }

    /// Spaltenbreiten beim Scrollen, ⌃⌥←/→ schaltet eine Stufe weiter.
    static let scrollWidths: [Double] = [1.0 / 3, 0.5, 2.0 / 3]

    /// Neuer Versatz der scrollenden Fläche: `reveal` (Fokus-Feld ohne Versatz) wird ganz sichtbar, der Rand bleibt im Inhalt.
    static func scrollOffset(_ current: CGFloat, reveal: CGRect?, contentMaxX: CGFloat, in b: CGRect) -> CGFloat {
        var x = current
        if let r = reveal {
            if r.maxX - x > b.maxX { x = r.maxX - b.maxX }
            if r.minX - x < b.minX { x = r.minX - b.minX }
        }
        return min(max(0, x), max(0, contentMaxX - b.maxX))
    }

    /// Abschnitte auf einer Achse nach Verhältnissen, dazwischen `gap`; Kanten auf ganze Punkte, das letzte Ende sitzt genau am Rand.
    static func segments(start: CGFloat, length: CGFloat, gap: CGFloat, ratios: [Double]) -> [(start: CGFloat, end: CGFloat)] {
        let usable = max(0, length - gap * CGFloat(ratios.count - 1))
        var acc = 0.0, out: [(CGFloat, CGFloat)] = []
        for (i, v) in ratios.enumerated() {
            let s = (start + CGFloat(acc) * usable + CGFloat(i) * gap).rounded()
            acc += v
            let e = i == ratios.count - 1 ? start + length : (start + CGFloat(acc) * usable + CGFloat(i) * gap).rounded()
            out.append((s, e))
        }
        return out
    }

    /// Neue Verhältnisse, wenn Grenze `d` an Position `p` gezogen wird. Kein Feld wird kleiner als 5 %.
    static func drag(_ d: SplitLine, to p: CGPoint, gap: CGFloat, ratios current: [Double]) -> [Double] {
        var v = current.count == d.count ? current : Array(repeating: 1 / Double(d.count), count: d.count)
        let start = d.vertical ? d.span.minX : d.span.minY, length = d.vertical ? d.span.width : d.span.height
        let usable = length - gap * CGFloat(d.count - 1)
        guard usable > 0 else { return v }
        let before = v[..<d.index].reduce(0, +), pair = v[d.index] + v[d.index + 1]
        let pos = Double(((d.vertical ? p.x : p.y) - start - CGFloat(d.index) * gap - gap / 2) / usable)
        v[d.index] = min(max(pos - before, 0.05), pair - 0.05)
        v[d.index + 1] = pair - v[d.index]
        return v
    }

    /// Nachbar nach Geometrie: nächstes Feld in Richtung `d`, bevorzugt eines, das sich auf der anderen Achse überlappt.
    static func neighbor(of i: Int, frames: [CGRect], _ d: Direction) -> Int? {
        guard frames.indices.contains(i) else { return nil }
        let a = frames[i]
        func score(_ b: CGRect) -> (Bool, CGFloat, CGFloat)? {
            let dist: CGFloat, overlap: CGFloat
            switch d {
            case .left: guard b.maxX <= a.minX + 1 else { return nil }; dist = a.minX - b.maxX; overlap = min(a.maxY, b.maxY) - max(a.minY, b.minY)
            case .right: guard b.minX >= a.maxX - 1 else { return nil }; dist = b.minX - a.maxX; overlap = min(a.maxY, b.maxY) - max(a.minY, b.minY)
            case .up: guard b.maxY <= a.minY + 1 else { return nil }; dist = a.minY - b.maxY; overlap = min(a.maxX, b.maxX) - max(a.minX, b.minX)
            case .down: guard b.minY >= a.maxY - 1 else { return nil }; dist = b.minY - a.maxY; overlap = min(a.maxX, b.maxX) - max(a.minX, b.minX)
            }
            return (overlap <= 0, dist, -overlap)
        }
        return frames.indices.filter { $0 != i }.compactMap { j in score(frames[j]).map { (j, $0) } }
            .min { ($0.1.0 ? 1 : 0, $0.1.1, $0.1.2) < ($1.1.0 ? 1 : 0, $1.1.1, $1.1.2) }?.0
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
