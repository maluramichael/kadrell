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
    /// Ohne Gruppierung steht der Projektname vor dem Titel, sonst trägt ihn die Gruppenzeile darüber.
    var project: String? = nil
    /// Strich-Anzeige statt Punkt: gesetzt, wenn angehängt und Claude arbeitet oder (untätig) noch ein Kommando läuft.
    /// Warten und Fehler behalten den Punkt. Farbe ohne Puls-Alpha, `lit` = leuchtende Striche (nil = alle, bei
    /// „Bewegung reduzieren“).
    var meter: (color: NSColor, lit: Int?)? = nil
    /// Claude-Prozess läuft (angehängt): steuert die Punktform bei „Farben nicht unterscheiden“ (Ring = nicht gestartet).
    let attached: Bool
    let selected: Bool
    let focused: Bool
    /// Der Baum hat die Tastatur (⌘1): fokussierte Zeile bekommt einen Rahmen.
    let keyFocus: Bool
    let hover: Bool
    let message: String?
    let showAge: Bool
    /// Fertig oder wartet, ungesehen: fetter heller Titel und Marke „neu“, auch an der fokussierten Zeile.
    let unread: Bool
}

extension SidebarSessionItem {
    static func meter(for s: Session, attached: Bool, lit: Int?) -> (color: NSColor, lit: Int?)? {
        // Warten und Fehler gehen vor: eine laufende Shell darf sie nicht hinter den Strichen verstecken.
        guard attached, s.status == .running || (s.hasRunningShell && s.status == .idle) else { return nil }
        return (Theme.statusColor(s.status, attached: true, shellRunning: s.hasRunningShell), lit)
    }
}

enum SidebarStyle: String, CaseIterable {
    case classic, compact

    var title: String {
        switch self {
        case .classic: String(localized: "Klassisch", bundle: Bundle.app)
        case .compact: String(localized: "Kompakt", bundle: Bundle.app)
        }
    }

    @MainActor var renderer: any SidebarRenderer {
        switch self {
        case .classic: ClassicSidebarRenderer()
        case .compact: CompactSidebarRenderer()
        }
    }
}

/// Bausteine, die sich alle Designs teilen: Zeilenfläche, Punkt, Titel, Laufzeit, Nachricht, Favorit, Gruppen-Label.
extension SidebarRenderer {
    func headRect(_ row: CGRect) -> CGRect { CGRect(x: row.minX, y: row.minY, width: row.width, height: sessionRow) }
    /// Höhe der Nachrichtenzeile unter der Kopfzeile einer Session (Einstellung „letzte Nachricht zeigen“). 2 pt mehr
    /// als die Schrift braucht: mit Tastaturring rückt der Umriss unten 2 pt ein und bliebe sonst an den Unterlängen.
    var messageLine: CGFloat { 16 }
    func dotRect(_ row: CGRect) -> CGRect { CGRect(x: 27, y: headRect(row).midY - 4, width: 8, height: 8) }
    /// Strich-Anzeige arbeitender Sessions, sitzt anstelle des Punkts.
    func meterRect(_ row: CGRect) -> CGRect { CGRect(x: 26, y: headRect(row).midY - 5, width: 12, height: 10) }
    /// Alles, was der Status der Zeile je belegt: Puls und Striche zeichnen nur diese Fläche neu.
    func statusRect(_ row: CGRect) -> CGRect { dotRect(row).union(meterRect(row)) }

    /// Fläche einer hervorgehobenen Zeile: eingerückt und rund, in der Modulfarbe wie die Kachelfläche.
    func rowShape(_ row: CGRect) -> CGRect { row.insetBy(dx: 6, dy: 1) }

    func fillRowShape(_ shape: CGRect) {
        Theme.bg.setFill()
        NSBezierPath(roundedRect: shape, xRadius: 6, yRadius: 6).fill()
    }

    func drawSession(_ s: SidebarSessionItem, in r: CGRect) {
        drawSessionBackground(s, in: r)
        drawSessionContent(s, in: r)
    }

    /// Balken links in Gruppenfarbe an ausgewählten Zeilen.
    func drawSelectionBar(_ on: Bool, color: NSColor, row: CGRect) {
        if on { color.setFill(); CGRect(x: 0, y: row.minY, width: 3, height: row.height).fill() }
    }

    /// Hohe Gruppenzeile: Auswahlbalken, Chevron, Farbakzent, Label, rechts Favorit und Warte-Marke, Pfad darunter.
    func drawGroupWithPath(_ g: SidebarGroupItem, in r: CGRect) {
        drawGroupBackground(g, in: r)
        let head = headRect(r)
        drawChevron(g, head: head)
        drawGroupAccent(g, head: head)
        drawGroupName(g, head: head, right: drawWaitingBadge(g, head: head, right: drawFavorite(g, head: head)))
        drawPath(g, below: head)
    }

    /// Hover als Fläche in Modulfarbe, Auswahl (eine Session der Gruppe ausgewählt) als Balken links.
    func drawGroupBackground(_ g: SidebarGroupItem, in r: CGRect) {
        if g.hover { fillRowShape(rowShape(r)) }
        drawSelectionBar(g.selected, color: g.color, row: r)
    }

    /// Hintergrund einer Session: Hover, Auswahl und Fokus als Fläche in Modulfarbe. Die gezeigte Session trägt
    /// zusätzlich einen Umriss in `fg`, mit Tastatur außen einen Ring in der Akzentfarbe, die Auswahl den Balken links.
    func drawSessionBackground(_ s: SidebarSessionItem, in row: CGRect) {
        let shape = sessionShape(s, row: row)
        if s.focused, s.keyFocus { drawKeyFocusRing(keyFocusRing(row)) }
        if s.focused || s.selected || s.hover { fillRowShape(shape) }
        if s.focused { drawFocusOutline(shape) }
        drawSelectionBar(s.selected, color: s.color, row: row)
    }

    /// 1,5 pt Umriss innen an der Fläche.
    func drawFocusOutline(_ shape: CGRect) {
        let path = NSBezierPath(roundedRect: shape.insetBy(dx: 0.75, dy: 0.75), xRadius: 5.25, yRadius: 5.25)
        path.lineWidth = 1.5
        Theme.fg.setStroke()
        path.stroke()
    }

    /// Fläche einer Session. Mit Tastaturring rückt sie oben und unten 2 pt ein: in der Zeile ist darüber nur 1 pt
    /// frei, so liegen Ring, Luft und Umriss ringsum gleich breit und der Ring bleibt in der Zeile.
    func sessionShape(_ s: SidebarSessionItem, row: CGRect) -> CGRect {
        s.focused && s.keyFocus ? keyFocusRing(row).insetBy(dx: 2.5, dy: 2.5) : rowShape(row)
    }

    /// Außenkante des Tastaturrings: um die Zeilenfläche, innerhalb der Zeile.
    func keyFocusRing(_ row: CGRect) -> CGRect { rowShape(row).insetBy(dx: -2.5, dy: -0.5) }

    /// 2 pt Ring in der Akzentfarbe.
    func drawKeyFocusRing(_ outer: CGRect) {
        let ring = NSBezierPath(roundedRect: outer, xRadius: 8, yRadius: 8)
        ring.append(NSBezierPath(roundedRect: outer.insetBy(dx: 2, dy: 2), xRadius: 6, yRadius: 6))
        ring.windingRule = .evenOdd
        Theme.accent.setFill()
        ring.fill()
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
        drawStatus(s, row: row)
        let iconRect = CGRect(x: 12, y: r.midY - 6, width: 12, height: 12), iconColor = s.selected || s.hover ? Theme.fg : Theme.muted
        if s.session.isRemote { Icons.server(in: iconRect, color: iconColor) }
        else if s.session.isShell { Icons.computer(in: iconRect, color: iconColor) }
        var right = r.maxX - 10
        if s.showAge {
            let age = NSAttributedString(string: s.session.elapsed(), attributes: Theme.attrs(10.5, Theme.muted))
            right -= age.size().width
            age.draw(at: CGPoint(x: right, y: r.midY - 7))
            right -= 8
        }
        if s.unread { right = drawUnreadPill(s, head: r, right: right, shape: sessionShape(s, row: row)) - 8 }
        sessionTitle(s).draw(with: CGRect(x: 42, y: r.midY - 8, width: max(0, right - 42), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    /// Strich-Anzeige für arbeitende Sessions, sonst der Statuspunkt.
    func drawStatus(_ s: SidebarSessionItem, row: CGRect) {
        if let m = s.meter { Instrument.meter(meterRect(row), lit: m.lit, color: m.color) }
        else { Icons.statusDot(in: dotRect(row), status: s.session.status, attached: s.attached, color: s.dot) }
    }

    /// Marke „neu“ links von `right`, liefert ihre linke Kante. Sie bleibt innerhalb des Umrisses der Zeilenfläche,
    /// mit Tastaturring wird sie dafür flacher.
    func drawUnreadPill(_ s: SidebarSessionItem, head r: CGRect, right: CGFloat, shape: CGRect) -> CGFloat {
        let pillColor = Theme.color(for: s.session.status == .waiting ? .waiting : .running)
        let tag = NSAttributedString(string: String(localized: "neu", bundle: Bundle.app), attributes: Theme.attrs(10, Theme.pillText(on: pillColor), bold: true))
        let w = tag.size().width + 10, h = min(16, 2 * (r.midY - shape.minY - 2))
        let pill = CGRect(x: right - w, y: r.midY - h / 2, width: w, height: h)
        pillColor.setFill()
        NSBezierPath(roundedRect: pill, xRadius: 3, yRadius: 3).fill()
        tag.draw(at: CGPoint(x: pill.minX + 5, y: r.midY - 7))
        return pill.minX
    }

    /// Titel der Session, ohne Gruppierung mit „projekt › “ in Gruppenfarbe davor.
    func sessionTitle(_ s: SidebarSessionItem) -> NSAttributedString {
        let title = NSAttributedString(string: s.session.title, attributes: Theme.attrs(12, s.selected || s.hover || s.focused || s.unread ? Theme.fg : Theme.sub, bold: s.unread))
        guard let project = s.project else { return title }
        let line = NSMutableAttributedString(string: project + " › ", attributes: Theme.attrs(12, s.color, bold: true))
        line.append(title)
        return line
    }

    /// Favoritenherz ganz rechts in der Kopfzeile, liefert die linke Kante des belegten Platzes.
    func drawFavorite(_ g: SidebarGroupItem, head: CGRect) -> CGFloat {
        guard g.group.isFavorite else { return head.maxX - 10 }
        let r = CGRect(x: head.maxX - 10 - 12, y: head.midY - 6, width: 12, height: 12)
        Icons.heart(in: r, color: g.color, filled: true)
        return r.minX - 6
    }

    /// „2 ⏳“ links vom Favoriten, nur wenn etwas in der Gruppe wartet, auch bei eingeklappter Gruppe.
    func drawWaitingBadge(_ g: SidebarGroupItem, head: CGRect, right: CGFloat) -> CGFloat {
        guard g.waitingCount > 0 else { return right }
        let t = NSAttributedString(string: "\(g.waitingCount) ⏳", attributes: Theme.attrs(10.5, Theme.waitingText, bold: true))
        let x = right - t.size().width
        t.draw(at: CGPoint(x: x, y: head.midY - 7))
        return x - 8
    }

    /// Gesperrtes Versal-Label mit der Zahl der Sessions: `KADRELL · 2`.
    static func groupLabel(_ g: SidebarGroupItem) -> NSAttributedString {
        let label = NSMutableAttributedString(attributedString: groupName(g))
        label.append(groupCount(g))
        return label
    }

    /// Name allein, gesperrt in Versalien; wird bei Platzmangel gekürzt.
    static func groupName(_ g: SidebarGroupItem) -> NSAttributedString {
        Instrument.groupLabel(g.group.name)
    }

    /// Zahl der Sessions ` · 2`, steht immer ganz.
    static func groupCount(_ g: SidebarGroupItem) -> NSAttributedString {
        Instrument.groupLabel(" · \(g.dots.count)")
    }

    /// Kleiner Farbakzent der Gruppe zwischen Chevron und Label.
    func drawGroupAccent(_ g: SidebarGroupItem, head: CGRect) {
        Instrument.groupAccent(center: CGPoint(x: 29, y: head.midY), color: g.color)
    }

    /// Gruppen-Label ab x = 37 bis `right`; liefert die belegte Fläche. Gekürzt wird nur der Name, die Zahl steht direkt
    /// dahinter und bleibt sichtbar. Das Server-Symbol tragen nur die Sessions einer Host-Gruppe.
    @discardableResult
    func drawGroupName(_ g: SidebarGroupItem, head: CGRect, right: CGFloat) -> CGRect {
        let name = Self.groupName(g), count = Self.groupCount(g), y = head.midY - 7
        let avail = max(0, right - 37), countW = min(count.size().width.rounded(.up), avail)
        let nameW = min(name.size().width, avail - countW)
        let options: NSString.DrawingOptions = [.usesLineFragmentOrigin, .truncatesLastVisibleLine]
        if nameW > 0 { name.draw(with: CGRect(x: 37, y: y, width: avail - countW, height: 14), options: options) }
        count.draw(with: CGRect(x: 37 + nameW, y: y, width: countW, height: 14), options: options)
        return CGRect(x: 37, y: y, width: nameW + countW, height: 14)
    }

    func drawPath(_ g: SidebarGroupItem, below head: CGRect) {
        let pa = Theme.attrs(10.5, Theme.muted)
        let pw = max(0, head.maxX - 10 - 42)
        NSAttributedString(string: g.group.host != nil ? "ssh · tmux" : Theme.fitPath(g.group.cwd, width: pw, attrs: pa), attributes: pa)
            .draw(with: CGRect(x: 42, y: head.maxY - 4, width: pw, height: 14), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    func drawChevron(_ g: SidebarGroupItem, head: CGRect) {
        Icons.chevron(in: CGRect(x: 6, y: head.midY - 8, width: 16, height: 16), open: g.open, color: Theme.muted)
    }
}
