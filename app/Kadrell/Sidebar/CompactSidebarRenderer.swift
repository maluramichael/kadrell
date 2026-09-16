import AppKit

/// Dicht: kein Pfad, niedrigere Zeilen, Trennlinie zwischen Gruppen. Rechts im Kopf ein Punkt je Session,
/// so bleibt der Status auch bei eingeklappter Gruppe sichtbar.
struct CompactSidebarRenderer: SidebarRenderer {
    let groupRow: CGFloat = 24
    let sessionRow: CGFloat = 22
    let groupGap: CGFloat = 0
    let topInset: CGFloat = 4

    func drawGroup(_ g: SidebarGroupItem, in r: CGRect) {
        if g.hover { Theme.surface.setFill(); r.fill() }
        if !g.first { Theme.line.setFill(); CGRect(x: 0, y: r.minY, width: r.width, height: 1).fill() }
        if g.selected { g.color.setFill(); CGRect(x: 0, y: r.minY, width: 3, height: r.height).fill() }
        drawChevron(g, head: r)
        var x = drawFavorite(g, head: r)
        for c in g.dots.reversed() {
            x -= 5
            c.setFill()
            NSBezierPath(ovalIn: CGRect(x: x, y: r.midY - 2.5, width: 5, height: 5)).fill()
            x -= 3
        }
        drawName(g.group.name, color: g.color, head: r, right: x - 5)
    }

    func drawSession(_ s: SidebarSessionItem, in r: CGRect) {
        drawSessionBackground(s, in: r, base: nil, highlight: Theme.surface)
        drawSessionContent(s, in: r)
    }
}
