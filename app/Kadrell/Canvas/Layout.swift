import Foundation

/// Grid-Layout in Weltkoordinaten. Chrome (Gruppen-Kopfzeile) ist bildschirmkonstant und geht
/// deshalb mit `1/scale` in die Welt ein: das Layout hängt vom Maßstab ab.
struct Layout {
    static let worldPad: CGFloat = 24
    static let groupGap: CGFloat = 24
    static let groupMin: CGFloat = 720
    static let groupPad: CGFloat = 16
    static let cellGap: CGFloat = 12
    static let cellMin: CGFloat = 320
    static let cellAspect: CGFloat = 16.0 / 10.0
    /// Kopfzeile der Gruppe: 18 px Schrift (Bildschirm) plus 12 Welt-Punkte Abstand.
    static let groupHeaderScreen: CGFloat = 18
    static let groupHeaderGapWorld: CGFloat = 12
    static let cellHeaderScreen: CGFloat = 26
    static let minScale: CGFloat = 0.03
    static let maxScale: CGFloat = 8

    struct GroupInput { let id: String; let cellKeys: [String] }

    var groups: [String: CGRect] = [:]
    var headers: [String: CGRect] = [:]
    var cells: [String: CGRect] = [:]
    var plus: [String: CGRect] = [:]
    var content: CGRect = .zero
    var groupOrder: [String] = []

    static func columns(available: CGFloat, min: CGFloat, gap: CGFloat) -> Int {
        Swift.max(1, Int(((available + gap) / (min + gap)).rounded(.down)))
    }

    /// Weltbreite: mindestens die Fensterbreite; bei vielen Gruppen so viele Spalten, dass die Karte
    /// ungefähr das Seitenverhältnis des Fensters bekommt (Gruppenhöhe grob 0,45 der Breite).
    /// ponytail: Faustformel statt echtem Packing; ein Bin-Packing kommt, wenn die Karte spürbar schief ist.
    static func worldWidth(viewport: CGSize, groupCount: Int) -> CGFloat {
        let byWidth = columns(available: viewport.width - 2 * worldPad, min: groupMin, gap: groupGap)
        let aspect = viewport.height > 0 ? viewport.width / viewport.height : 1.6
        let wanted = Swift.max(byWidth, Int(ceil(sqrt(Double(groupCount) * 0.45 * Double(aspect)))))
        let needed = CGFloat(wanted) * (groupMin + groupGap) - groupGap + 2 * worldPad
        return Swift.max(viewport.width, needed)
    }

    /// LOD nach Kachelbreite auf dem Bildschirm: 0 Farbe, 1 Titel, 2 Kopf + Zeilen, 3 Terminal.
    static func lod(cellScreenWidth w: CGFloat) -> Int {
        w < 60 ? 0 : w < 140 ? 1 : w < 320 ? 2 : 3
    }

    static func compute(groups: [GroupInput], worldWidth: CGFloat, scale: CGFloat,
                        focused: String? = nil, viewportAspect: CGFloat = cellAspect) -> Layout {
        var l = Layout()
        let s = Swift.max(scale, 0.0001)
        let avail = Swift.max(worldWidth - 2 * worldPad, groupMin)
        let cols = columns(available: avail, min: groupMin, gap: groupGap)
        let gw = (avail - CGFloat(cols - 1) * groupGap) / CGFloat(cols)
        let headerH = groupHeaderScreen / s + groupHeaderGapWorld
        var y = worldPad
        var rowH: CGFloat = 0
        for (i, g) in groups.enumerated() {
            let col = i % cols
            if col == 0, i > 0 { y += rowH + groupGap; rowH = 0 }
            let x = worldPad + CGFloat(col) * (gw + groupGap)
            let inner = gw - 2 * groupPad
            let ccols = columns(available: inner, min: cellMin, gap: cellGap)
            let cw = (inner - CGFloat(ccols - 1) * cellGap) / CGFloat(ccols)
            var cy = y + groupPad + headerH
            var crowH: CGFloat = 0
            let keys = g.cellKeys + ["+"]
            for (j, k) in keys.enumerated() {
                let cc = j % ccols
                if cc == 0, j > 0 { cy += crowH + cellGap; crowH = 0 }
                let h = (k == focused) ? cw / viewportAspect : cw / cellAspect
                let r = CGRect(x: x + groupPad + CGFloat(cc) * (cw + cellGap), y: cy, width: cw, height: h)
                if k == "+" { l.plus[g.id] = r } else { l.cells[k] = r }
                crowH = Swift.max(crowH, h)
            }
            let gh = cy + crowH + groupPad - y
            l.groups[g.id] = CGRect(x: x, y: y, width: gw, height: gh)
            l.headers[g.id] = CGRect(x: x + groupPad, y: y + groupPad, width: inner, height: groupHeaderScreen / s)
            l.groupOrder.append(g.id)
            rowH = Swift.max(rowH, gh)
        }
        l.content = CGRect(x: 0, y: 0, width: worldWidth, height: groups.isEmpty ? worldPad * 2 : y + rowH + worldPad)
        return l
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
