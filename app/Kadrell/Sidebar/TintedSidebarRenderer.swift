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
        drawGroupWithPath(g, in: r)
    }

    func drawSession(_ s: SidebarSessionItem, in r: CGRect) {
        drawSessionBackground(s, in: r, base: s.color.mixed(0.05, into: Theme.panel), highlight: s.color.mixed(0.10, into: Theme.panel))
        drawSessionContent(s, in: r)
    }
}
