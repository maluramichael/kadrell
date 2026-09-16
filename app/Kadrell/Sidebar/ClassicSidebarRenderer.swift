import AppKit

/// Ursprüngliches Design: Name mit ▪ in Gruppenfarbe, Pfad darunter, Hover grau hinterlegt.
struct ClassicSidebarRenderer: SidebarRenderer {
    let groupRow: CGFloat = 38
    let sessionRow: CGFloat = 24
    let groupGap: CGFloat = 6

    func drawGroup(_ g: SidebarGroupItem, in r: CGRect) {
        if g.hover { Theme.surface.setFill(); r.fill() }
        if g.selected { g.color.setFill(); CGRect(x: 0, y: r.minY, width: 3, height: r.height).fill() }
        let head = headRect(r)
        drawChevron(g, head: head)
        let right = drawCount(g, head: head, right: drawFavorite(g, head: head))
        drawName("▪ " + g.group.name, color: g.color, head: head, right: right)
        drawPath(g, below: head)
    }

    func drawSession(_ s: SidebarSessionItem, in r: CGRect) {
        drawSessionBackground(s, in: r, base: nil, highlight: Theme.surface)
        drawSessionContent(s, in: r)
    }
}
