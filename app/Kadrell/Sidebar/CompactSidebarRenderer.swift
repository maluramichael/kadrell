import AppKit

/// Dicht: kein Pfad, niedrigere Zeilen, Fuge zwischen Gruppen. Eingeklappt steht rechts im Kopf ein Punkt
/// je Session, so bleibt der Status sichtbar. Aufgeklappt zeigen ihn die Zeilen selbst.
struct CompactSidebarRenderer: SidebarRenderer {
    let groupRow: CGFloat = 24
    let sessionRow: CGFloat = 22
    let groupGap: CGFloat = 0
    let topInset: CGFloat = 4

    func drawGroup(_ g: SidebarGroupItem, in r: CGRect) {
        drawGroupBackground(g, in: r)
        drawChevron(g, head: r)
        drawGroupAccent(g, head: r)
        var x = drawWaitingBadge(g, head: r, right: drawFavorite(g, head: r))
        for c in g.open ? [] : g.dots.reversed() {
            x -= 5
            c.setFill()
            NSBezierPath(ovalIn: CGRect(x: x, y: r.midY - 2.5, width: 5, height: 5)).fill()
            x -= 3
        }
        drawGroupName(g, head: r, right: x - 5)
        if !g.first { Instrument.hSeam(x: 0, y: r.minY, width: r.width) }
    }
}
