import AppKit

/// Zeichnet die Zeilen des Baums. `SidebarView` kümmert sich um Layout, Events und die schwebende Toolbar,
/// ein Renderer nur um das Aussehen einer Zeile. Neues Design = neuer Renderer plus Eintrag in `SidebarStyle`.
@MainActor
protocol SidebarRenderer {
    var groupRow: CGFloat { get }
    /// Kopfzeile einer Session, die Nachrichtenzeile kommt bei Bedarf dazu.
    var sessionRow: CGFloat { get }
    /// Abstand vor jeder Gruppe außer der ersten.
    var groupGap: CGFloat { get }
    /// Abstand über der ersten Gruppe.
    var topInset: CGFloat { get }
    func drawGroup(_ g: SidebarGroupItem, in r: CGRect)
    func drawSession(_ s: SidebarSessionItem, in r: CGRect)
}

struct SidebarGroupItem {
    let group: Group
    let color: NSColor
    /// Statusfarben der Sessions in Baumreihenfolge, auch bei eingeklappter Gruppe.
    let dots: [NSColor]
    let open: Bool
    let selected: Bool
    let hover: Bool
    let first: Bool
    /// Sessions dieser Gruppe, die auf dich warten, auch bei eingeklappter Gruppe.
    let waitingCount: Int
}

struct SidebarSessionItem {
    let session: Session
    let color: NSColor
    let dot: NSColor
    let selected: Bool
    let focused: Bool
    /// Der Baum hat die Tastatur (⌘1): fokussierte Zeile bekommt einen Rahmen.
    let keyFocus: Bool
    let hover: Bool
    let message: String?
    let showAge: Bool
    /// Antwort seit dem letzten Fokus: Titel fett.
    let unread: Bool
}

enum SidebarStyle: String, CaseIterable {
    case classic, tinted, compact

    var title: String {
        switch self {
        case .classic: "Klassisch"
        case .tinted: "Getönte Gruppen"
        case .compact: "Kompakt"
        }
    }

    @MainActor var renderer: any SidebarRenderer {
        switch self {
        case .classic: ClassicSidebarRenderer()
        case .tinted: TintedSidebarRenderer()
        case .compact: CompactSidebarRenderer()
        }
    }
}

/// Bausteine, die sich alle Designs teilen: Punkt, Titel, Laufzeit, Nachricht, Favorit.
extension SidebarRenderer {
    func headRect(_ row: CGRect) -> CGRect { CGRect(x: row.minX, y: row.minY, width: row.width, height: sessionRow) }
    func dotRect(_ row: CGRect) -> CGRect { CGRect(x: 27, y: headRect(row).midY - 4, width: 8, height: 8) }

    /// Hintergrund einer Session: Fokus getönt, mit Tastatur zusätzlich gerahmt, Auswahl mit Balken links.
    func drawSessionBackground(_ s: SidebarSessionItem, in row: CGRect, base: NSColor?, highlight: NSColor) {
        if let base { base.setFill(); row.fill() }
        if s.focused { s.color.mixed(0.14, into: Theme.surface).setFill(); row.fill() }
        if s.focused, s.keyFocus { s.color.setStroke(); NSBezierPath(rect: row.insetBy(dx: 0.5, dy: 0.5)).stroke() }
        else if s.selected || s.hover { highlight.setFill(); row.fill() }
        if s.selected { s.color.setFill(); CGRect(x: 0, y: row.minY, width: 3, height: row.height).fill() }
    }

    /// Punkt, Titel und rechtsbündige Laufzeit. Die Laufzeit sitzt immer an derselben Stelle, auch beim Hover:
    /// Aktionen schweben als Toolbar darüber.
    func drawSessionContent(_ s: SidebarSessionItem, in row: CGRect) {
        let r = headRect(row)
        if let m = s.message {
            // Ende der Antwort: dort steht meist, worauf die Session wartet. Vorn abgeschnitten.
            let style = NSMutableParagraphStyle()
            style.lineBreakMode = .byTruncatingHead
            var attrs = Theme.attrs(10.5, Theme.muted)
            attrs[.paragraphStyle] = style
            NSAttributedString(string: String(m.suffix(300)), attributes: attrs)
                .draw(in: CGRect(x: 42, y: r.maxY - 4, width: max(0, row.maxX - 10 - 42), height: 14))
        }
        s.dot.setFill()
        NSBezierPath(ovalIn: dotRect(row)).fill()
        if s.session.isShell { Icons.computer(in: CGRect(x: 12, y: r.midY - 6, width: 12, height: 12), color: s.selected || s.hover ? Theme.fg : Theme.muted) }
        var right = r.maxX - 10
        if s.showAge {
            let age = NSAttributedString(string: s.session.elapsed(), attributes: Theme.attrs(10.5, Theme.muted))
            right -= age.size().width
            age.draw(at: CGPoint(x: right, y: r.midY - 7))
            right -= 8
        }
        // Die fokussierte Zeile zeigt gerade selbst an, was neu ist: dort bleibt der Marker aus.
        let unread = s.unread && !s.focused
        let title = NSAttributedString(string: s.session.title, attributes: Theme.attrs(12, s.selected || s.hover ? Theme.fg : Theme.sub, bold: unread))
        title.draw(with: CGRect(x: 42, y: r.midY - 8, width: max(0, right - 42), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    /// Favoritenherz ganz rechts in der Kopfzeile, liefert die linke Kante des belegten Platzes.
    func drawFavorite(_ g: SidebarGroupItem, head: CGRect) -> CGFloat {
        guard g.group.isFavorite else { return head.maxX - 10 }
        let r = CGRect(x: head.maxX - 10 - 12, y: head.midY - 6, width: 12, height: 12)
        Icons.heart(in: r, color: g.color, filled: true)
        return r.minX - 6
    }

    /// Anzahl der Sessions rechts in der Kopfzeile, links neben dem Herz.
    func drawCount(_ g: SidebarGroupItem, head: CGRect, right: CGFloat) -> CGFloat {
        let n = NSAttributedString(string: "\(g.dots.count)", attributes: Theme.attrs(10.5, Theme.muted))
        let x = right - n.size().width
        n.draw(at: CGPoint(x: x, y: head.midY - 7))
        return x - 8
    }

    /// „2 ⏳“ links vom Zähler, nur wenn etwas in der Gruppe wartet, auch bei eingeklappter Gruppe.
    func drawWaitingBadge(_ g: SidebarGroupItem, head: CGRect, right: CGFloat) -> CGFloat {
        guard g.waitingCount > 0 else { return right }
        let t = NSAttributedString(string: "\(g.waitingCount) ⏳", attributes: Theme.attrs(10.5, Theme.waiting, bold: true))
        let x = right - t.size().width
        t.draw(at: CGPoint(x: x, y: head.midY - 7))
        return x - 8
    }

    func drawName(_ text: String, color: NSColor, head: CGRect, right: CGFloat) {
        NSAttributedString(string: text, attributes: Theme.attrs(12, color, bold: true))
            .draw(with: CGRect(x: 26, y: head.midY - 8, width: max(0, right - 26), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    func drawPath(_ g: SidebarGroupItem, below head: CGRect) {
        let pa = Theme.attrs(10.5, Theme.muted)
        let pw = max(0, head.maxX - 10 - 42)
        NSAttributedString(string: Theme.fitPath(g.group.cwd, width: pw, attrs: pa), attributes: pa)
            .draw(with: CGRect(x: 42, y: head.maxY - 4, width: pw, height: 14), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    func drawChevron(_ g: SidebarGroupItem, head: CGRect) {
        Icons.chevron(in: CGRect(x: 6, y: head.midY - 8, width: 16, height: 16), open: g.open, color: Theme.muted)
    }
}
