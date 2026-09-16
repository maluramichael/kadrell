import AppKit

/// Jede Gruppe leicht in ihrer Farbe getönt, der Kopf kräftiger. Gruppen stoßen aneinander, der Farbwechsel trennt sie.
struct TintedSidebarRenderer: SidebarRenderer {
    let groupRow: CGFloat = 38
    let sessionRow: CGFloat = 24
    let groupGap: CGFloat = 0
    let topInset: CGFloat = 0

    func drawGroup(_ g: SidebarGroupItem, in r: CGRect) {
        g.color.mixed(g.hover ? 0.16 : 0.11, into: Theme.panel).setFill()
        r.fill()
        if g.selected { g.color.setFill(); CGRect(x: 0, y: r.minY, width: 3, height: r.height).fill() }
        let head = headRect(r)
        drawChevron(g, head: head)
        let right = drawCount(g, head: head, right: drawWaitingBadge(g, head: head, right: drawFavorite(g, head: head)))
        drawName(g.group.name, color: g.color, head: head, right: right)
        drawPath(g, below: head)
    }

    func drawSession(_ s: SidebarSessionItem, in r: CGRect) {
        drawSessionBackground(s, in: r, base: s.color.mixed(0.05, into: Theme.panel), highlight: s.color.mixed(0.10, into: Theme.panel))
        drawSessionContent(s, in: r)
    }
}
