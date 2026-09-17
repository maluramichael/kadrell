import AppKit

/// Ursprüngliches Design: Name mit ▪ in Gruppenfarbe, Pfad darunter, Hover grau hinterlegt.
struct ClassicSidebarRenderer: SidebarRenderer {
    let groupRow: CGFloat = 38
    let sessionRow: CGFloat = 24
    let groupGap: CGFloat = 6
    let topInset: CGFloat = 6

    func drawGroup(_ g: SidebarGroupItem, in r: CGRect) {
        if g.hover { Theme.surface.setFill(); r.fill() }
        drawGroupWithPath(g, in: r, prefix: "▪ ")
    }
}
