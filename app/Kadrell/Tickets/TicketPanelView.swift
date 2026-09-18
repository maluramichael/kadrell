import AppKit

/// Rechte Region: Suchfeld oben, darunter die scrollende Ticket-Liste. Legt seine zwei Unteransichten selbst per
/// `layout()` an, statt sie dem Autoresizing zu überlassen (das verrechnet sich, wenn der Container bei Null startet).
final class RightPanelContainer: NSView {
    let search = NSSearchField(frame: .zero)
    let scroll = NSScrollView(frame: .zero)

    override init(frame: NSRect) {
        super.init(frame: frame)
        addSubview(search)
        addSubview(scroll)
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) not used") }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        let h = (36 * Theme.scale).rounded()
        search.frame = NSRect(x: 8, y: 6, width: max(0, bounds.width - 16), height: h - 14)
        scroll.frame = NSRect(x: 0, y: h, width: bounds.width, height: max(0, bounds.height - h))
    }
}

/// Rechte Ticket-Leiste: offene Tickets als Zeilen, jede mit „implement“-Knopf, der eine neue Session startet.
/// documentView von `rightScroll`, gezeichnet wie der Baum (kein NSTableView, damit Optik und Scrollen zum Rest passen).
/// Tastatur: ↑↓ wählt, ⏎ startet die markierte Session, ⌘R lädt neu. Klick auf den Knopf startet direkt.
final class TicketPanelView: NSView {
    /// Sichtbare (gefilterte) Tickets. Gesetzt wird über `setTickets`, gefiltert über `filter`.
    private(set) var tickets: [Ticket] = []
    /// Alle geladenen Tickets, aus denen `filter` die sichtbaren zieht.
    private var all: [Ticket] = []
    /// Freitextfilter über Titel und Projekt (Suchfeld oben in der Leiste).
    var filter = "" { didSet { applyFilter() } }
    /// Meldung statt Liste: Laden, Fehler, kein Provider, keine Tickets. Nil = Liste zeigen.
    var status: String? { didSet { reload() } }
    var onImplement: ((Ticket) -> Void)?
    var onRefresh: (() -> Void)?

    private var selected = 0

    func setTickets(_ list: [Ticket]) { all = list; applyFilter() }

    private func applyFilter() {
        let q = filter.trimmingCharacters(in: .whitespaces).lowercased()
        tickets = q.isEmpty ? all : all.filter { $0.title.lowercased().contains(q) || $0.project.lowercased().contains(q) }
        selected = min(selected, max(0, tickets.count - 1))
        reload()
    }

    @objc func filterChanged(_ sender: NSSearchField) { filter = sender.stringValue }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { NSApp.currentEvent?.type != .leftMouseDown }
    override func becomeFirstResponder() -> Bool { needsDisplay = true; return true }
    override func resignFirstResponder() -> Bool { needsDisplay = true; return true }

    private var rowHeight: CGFloat { (52 * Theme.scale).rounded() }
    private var pad: CGFloat { (12 * Theme.scale).rounded() }
    private var isActive: Bool { window?.firstResponder === self }
    /// Sichtbare Breite (Clip der Scrollview), maßgeblich fürs Zeichnen: die eigene Frame-Breite kann kurz veraltet sein.
    private var contentWidth: CGFloat { enclosingScrollView?.contentSize.width ?? bounds.width }

    /// Breite an die Clip-Fläche klammern (der Knopf sitzt sonst außerhalb der Region) und die Höhe an die Zeilen anpassen.
    func reload() {
        let clip = enclosingScrollView?.contentSize ?? bounds.size
        let contentH = status != nil ? clip.height : max(clip.height, CGFloat(tickets.count) * rowHeight)
        setFrameSize(NSSize(width: clip.width, height: contentH))
        needsDisplay = true
    }

    override func layout() {
        super.layout()
        if let w = enclosingScrollView?.contentSize.width, abs(w - frame.width) > 0.5 { reload() }
    }

    override func draw(_ dirty: NSRect) {
        Theme.panel.setFill()
        bounds.fill()
        if let status {
            let attrs = Theme.attrs(12 * Theme.scale, Theme.muted, truncate: false)
            let size = NSString(string: status).size(withAttributes: attrs)
            let r = NSRect(x: pad, y: (bounds.height - size.height) / 2, width: contentWidth - 2 * pad, height: size.height)
            NSString(string: status).draw(in: r, withAttributes: attrs)
            return
        }
        for (i, t) in tickets.enumerated() { drawRow(t, at: i) }
    }

    private func drawRow(_ t: Ticket, at i: Int) {
        let r = NSRect(x: 0, y: CGFloat(i) * rowHeight, width: contentWidth, height: rowHeight)
        if i == selected, isActive { Theme.surface.setFill(); r.fill() }
        let btn = implementRect(r)
        let titleWidth = btn.minX - pad - pad
        NSString(string: t.title).draw(in: NSRect(x: pad, y: r.minY + 9 * Theme.scale, width: titleWidth, height: 18 * Theme.scale),
                                       withAttributes: Theme.attrs(13 * Theme.scale, Theme.fg, bold: true))
        NSString(string: t.project).draw(in: NSRect(x: pad, y: r.minY + 28 * Theme.scale, width: titleWidth, height: 15 * Theme.scale),
                                         withAttributes: Theme.attrs(11 * Theme.scale, Theme.muted))
        Theme.running.setFill()
        NSBezierPath(roundedRect: btn, xRadius: 4, yRadius: 4).fill()
        let label = String(localized: "implement", bundle: Bundle.app)
        let la = Theme.attrs(11 * Theme.scale, Theme.pillText(on: Theme.running), bold: true)
        let ls = NSString(string: label).size(withAttributes: la)
        NSString(string: label).draw(at: NSPoint(x: btn.midX - ls.width / 2, y: btn.midY - ls.height / 2), withAttributes: la)
        Theme.line.setStroke()
        let sep = NSBezierPath()
        sep.move(to: NSPoint(x: 0, y: r.maxY - 0.5))
        sep.line(to: NSPoint(x: r.width, y: r.maxY - 0.5))
        sep.stroke()
    }

    private func implementRect(_ row: NSRect) -> NSRect {
        let w = 84 * Theme.scale, h = 22 * Theme.scale
        return NSRect(x: row.maxX - pad - w, y: row.minY + (rowHeight - h) / 2, width: w, height: h)
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        let i = Int(p.y / rowHeight)
        guard tickets.indices.contains(i) else { return }
        selected = i
        window?.makeFirstResponder(self)
        needsDisplay = true
        if implementRect(NSRect(x: 0, y: CGFloat(i) * rowHeight, width: contentWidth, height: rowHeight)).contains(p) {
            onImplement?(tickets[i])
        }
    }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case KeyCode.down: move(1)
        case KeyCode.up: move(-1)
        case KeyCode.returnKey, KeyCode.keypadEnter:
            if tickets.indices.contains(selected) { onImplement?(tickets[selected]) }
        default:
            if event.modifierFlags.contains(.command), event.charactersIgnoringModifiers == "r" { onRefresh?() }
            else { super.keyDown(with: event) }
        }
    }

    private func move(_ d: Int) {
        guard !tickets.isEmpty else { return }
        selected = max(0, min(tickets.count - 1, selected + d))
        scrollToVisible(NSRect(x: 0, y: CGFloat(selected) * rowHeight, width: bounds.width, height: rowHeight))
        needsDisplay = true
    }
}
