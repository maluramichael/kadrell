import AppKit

/// Dicht: kein Pfad, niedrigere Zeilen, Trennlinie zwischen Gruppen. Eingeklappt steht rechts im Kopf ein Punkt
/// je Session, so bleibt der Status sichtbar. Aufgeklappt zeigen ihn die Zeilen selbst.
struct CompactSidebarRenderer: SidebarRenderer {
    let groupRow: CGFloat = 24
    let sessionRow: CGFloat = 22
    let groupGap: CGFloat = 0
    let topInset: CGFloat = 4

    func drawGroup(_ g: SidebarGroupItem, in r: CGRect) {
        if g.hover { Theme.surface.setFill(); r.fill() }
        if !g.first { Theme.line.setFill(); CGRect(x: 0, y: r.minY, width: r.width, height: 1).fill() }
        drawSelectionBar(g.selected, color: g.color, row: r)
        drawChevron(g, head: r)
        var x = drawWaitingBadge(g, head: r, right: drawFavorite(g, head: r))
        for c in g.open ? [] : g.dots.reversed() {
            x -= 5
            c.setFill()
            NSBezierPath(ovalIn: CGRect(x: x, y: r.midY - 2.5, width: 5, height: 5)).fill()
            x -= 3
        }
        drawGroupName(g, head: r, right: x - 5)
    }
}
