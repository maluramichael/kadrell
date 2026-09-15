import Foundation

/// Layout in Weltkoordinaten, i3-artig: jede Gruppe ist ein frei platzierbares Rechteck (gespeichert),
/// die Kacheln darin füllen die Fläche als Raster mit `columns` Spalten. Die Gruppen-Kopfzeile ist
/// bildschirmkonstant und geht mit `1/scale` in die Welt ein.
struct Layout {
    static let worldPad: CGFloat = 24
    static let groupPad: CGFloat = 12
    static let cellGap: CGFloat = 10
    /// Kopfstreifen der Gruppe (Bildschirm-px) und Fußstreifen mit dem Griff, beide mit eigenem Hintergrund.
    static let groupHeaderScreen: CGFloat = 30
    static let groupFooterScreen: CGFloat = 18
    static let cellHeaderScreen: CGFloat = 26
    static let minScale: CGFloat = 0.03
    static let maxScale: CGFloat = 8
    static let defaultGroupSize = CGSize(width: 760, height: 520)
    static let minGroupSize = CGSize(width: 320, height: 200)

    struct GroupInput { let id: String; let cellKeys: [String]; let frame: CGRect }

    var groups: [String: CGRect] = [:]
    var headers: [String: CGRect] = [:]
    var footers: [String: CGRect] = [:]
    var cells: [String: CGRect] = [:]
    var content: CGRect = .zero
    var groupOrder: [String] = []

    /// LOD nach Kachelbreite auf dem Bildschirm: 0 Farbe, 1 Titel, 2 Kopf + Zeilen, 3 Terminal.
    static func lod(cellScreenWidth w: CGFloat) -> Int {
        w < 60 ? 0 : w < 140 ? 1 : w < 320 ? 2 : 3
    }

    static func compute(groups: [GroupInput], scale: CGFloat, columns: Int = 2,
                        focused: String? = nil, viewportAspect: CGFloat = 1.6) -> Layout {
        var l = Layout()
        let s = Swift.max(scale, 0.0001)
        let headerH = groupHeaderScreen / s
        let footerH = groupFooterScreen / s
        var union: CGRect?
        for g in groups {
            let f = g.frame
            l.groups[g.id] = f
            l.groupOrder.append(g.id)
            union = union.map { $0.union(f) } ?? f
            l.headers[g.id] = CGRect(x: f.minX, y: f.minY, width: f.width, height: headerH)
            l.footers[g.id] = CGRect(x: f.minX, y: f.maxY - footerH, width: f.width, height: footerH)
            let inner = CGRect(x: f.minX + groupPad, y: f.minY + headerH + groupPad,
                               width: f.width - 2 * groupPad, height: f.height - headerH - footerH - 2 * groupPad)
            let n = g.cellKeys.count
            guard n > 0, inner.width > 0, inner.height > 0 else { continue }
            let cols = Swift.max(1, Swift.min(columns, n))
            let rows = (n + cols - 1) / cols
            let cw = (inner.width - CGFloat(cols - 1) * cellGap) / CGFloat(cols)
            let ch = (inner.height - CGFloat(rows - 1) * cellGap) / CGFloat(rows)
            for (i, k) in g.cellKeys.enumerated() {
                let c = i % cols, r = i / cols
                l.cells[k] = CGRect(x: inner.minX + CGFloat(c) * (cw + cellGap), y: inner.minY + CGFloat(r) * (ch + cellGap), width: cw, height: ch)
            }
            // Fokus: die Kachel wächst auf das Fensterformat, so groß wie es in die Gruppe passt.
            if let focused, g.cellKeys.contains(focused) {
                var w = f.width, h = w / viewportAspect
                if h > f.height { h = f.height; w = h * viewportAspect }
                l.cells[focused] = CGRect(x: f.minX, y: f.minY, width: w, height: h)
            }
        }
        l.content = (union ?? CGRect(x: 0, y: 0, width: 1, height: 1)).insetBy(dx: -worldPad, dy: -worldPad)
        return l
    }

    /// Platz für eine neue Gruppe: rechts neben die bisherigen, sonst darunter; überlappt nie.
    static func placeNewGroup(existing: [CGRect], size: CGSize = defaultGroupSize, maxWidth: CGFloat = 3200) -> CGRect {
        guard !existing.isEmpty else { return CGRect(origin: .zero, size: size) }
        let gap: CGFloat = 24
        let maxX = existing.map(\.maxX).max()!, minY = existing.map(\.minY).min()!
        var candidate = CGRect(x: maxX + gap, y: minY, width: size.width, height: size.height)
        if candidate.maxX > maxWidth {
            candidate.origin = CGPoint(x: existing.map(\.minX).min()!, y: existing.map(\.maxY).max()! + gap)
        }
        while existing.contains(where: { $0.intersects(candidate) }) { candidate.origin.y += gap }
        return candidate
    }

    /// Maßstab und Offset (Bildschirmposition des Weltursprungs), damit `rect` exakt in den Viewport passt.
    static func fit(_ rect: CGRect, in viewport: CGSize, pad: CGFloat) -> (scale: CGFloat, offset: CGPoint) {
        let s = Swift.min(Swift.max(Swift.min((viewport.width - 2 * pad) / rect.width,
                                              (viewport.height - 2 * pad) / rect.height), minScale), maxScale)
        return (s, CGPoint(x: (viewport.width - rect.width * s) / 2 - rect.minX * s,
                           y: (viewport.height - rect.height * s) / 2 - rect.minY * s))
    }

    static func clamp(_ scale: CGFloat) -> CGFloat { Swift.min(Swift.max(scale, minScale), maxScale) }
}
