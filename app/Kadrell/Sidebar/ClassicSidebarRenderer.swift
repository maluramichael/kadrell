import AppKit

/// Ursprüngliches Design: Gruppen-Label mit Farbakzent, Pfad darunter.
struct ClassicSidebarRenderer: SidebarRenderer {
    let groupRow: CGFloat = 38
    let sessionRow: CGFloat = 24
    let groupGap: CGFloat = 6
    let topInset: CGFloat = 6

    func drawGroup(_ g: SidebarGroupItem, in r: CGRect) {
        drawGroupWithPath(g, in: r)
    }
}
