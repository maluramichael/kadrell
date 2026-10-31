import AppKit

/// Statusleiste unten im Fenster, befüllt nur von Extensions (`kadrell.footer.set`). Flache Module im Instrument-Look
/// wie die Nutzungswerte der oberen Leiste, getrennt durch Fugen: links ausgerichtete von links, rechts ausgerichtete
/// von rechts. Ohne Einträge blendet das Fenster sie ganz aus (siehe `MainWindowController.updateFooter`).
@MainActor
final class FooterBarView: NSView, NSViewToolTipOwner {
    typealias Entry = (name: String, item: FooterItem)

    /// Gezeichnet wird nur bei geändertem Stand, der Manager meldet bis zu 50-mal pro Sekunde.
    var entries: [Entry] = [] {
        didSet { if !Self.same(entries, oldValue) { needsDisplay = true } }
    }
    /// Klick auf einen Eintrag mit Aktion: Name der Extension und Aktions-ID.
    var onAction: ((_ name: String, _ id: String) -> Void)?

    /// Höhe in unskalierten Punkten, `height` rechnet die UI-Größe ein.
    static let baseHeight: CGFloat = 22
    static var height: CGFloat { (baseHeight * Theme.scale).rounded() }

    private var hitRects: [HitRegion] = []
    private var tips: [(rect: CGRect, text: String)] = []
    private var tipsKey = ""
    private var a11y: [A11yElement] = []
    /// Einträge, die beim letzten Zeichnen Platz hatten, mit Rahmen in unskalierten Punkten.
    private(set) var drawn: [(entry: Entry, rect: CGRect)] = []

    private static let pad: CGFloat = 10, partGap: CGFloat = 5, sideGap: CGFloat = 16
    private static let barWidth: CGFloat = 3, barGap: CGFloat = 1, barHeight: CGFloat = 10
    private static let gaugeWidth: CGFloat = 36, gaugeHeight: CGFloat = 6

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

    static func same(_ a: [Entry], _ b: [Entry]) -> Bool {
        a.count == b.count && zip(a, b).allSatisfy { $0.name == $1.name && $0.item == $1.item }
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.scaled(bounds) { drawBar($0) }
        updateToolTips()
    }

    // MARK: Platz

    /// Wie viele Einträge Platz haben: rechts zählt vor links, links fallen die hinteren weg, rechts die vorderen.
    /// Zwischen beiden Seiten bleiben mindestens `sideGap` frei. Liefert die Anzahl links (vorn) und rechts (hinten).
    static func fit(left: [CGFloat], right: [CGFloat], width: CGFloat) -> (left: Int, right: Int) {
        var rightCount = right.count
        while rightCount > 0, right.suffix(rightCount).reduce(0, +) > width { rightCount -= 1 }
        let room = width - right.suffix(rightCount).reduce(0, +) - (rightCount > 0 ? sideGap : 0)
        var leftCount = 0, used: CGFloat = 0
        while leftCount < left.count, used + left[leftCount] <= room {
            used += left[leftCount]
            leftCount += 1
        }
        return (leftCount, rightCount)
    }

    // MARK: Zeichnen

    private func drawBar(_ b: CGRect) {
        hitRects = []
        tips = []
        drawn = []
        Theme.panel.setFill()
        b.fill()
        Instrument.hSeam(x: 0, y: 0, width: b.width)
        let body = CGRect(x: 0, y: 2 * Instrument.hairline, width: b.width, height: b.height - 2 * Instrument.hairline)
        let left = entries.filter { $0.item.align == .left }, right = entries.filter { $0.item.align == .right }
        let seam = 2 * Instrument.hairline
        let fit = Self.fit(left: left.map { Self.width($0.item) + seam }, right: right.map { Self.width($0.item) + seam }, width: b.width)
        var x: CGFloat = 0
        for e in left.prefix(fit.left) {
            let r = CGRect(x: x, y: body.minY, width: Self.width(e.item), height: body.height)
            drawModule(e, in: r)
            Instrument.vSeam(x: r.maxX, y: body.minY, height: body.height)
            x = r.maxX + seam
        }
        var rx = b.width
        for e in right.suffix(fit.right).reversed() {
            let r = CGRect(x: rx - seam - Self.width(e.item), y: body.minY, width: Self.width(e.item), height: body.height)
            Instrument.vSeam(x: rx - seam, y: body.minY, height: body.height)
            drawModule(e, in: r)
            rx = r.minX
        }
    }

    /// Ein Modul: Label, Balken, Pegel und Text nebeneinander, senkrecht mittig. Mit Aktion klickbar, immer mit Tooltip.
    private func drawModule(_ e: Entry, in r: CGRect) {
        let item = e.item
        var x = r.minX + Self.pad
        for part in Self.parts(item) {
            switch part {
            case .text(let s): Self.drawCentered(s, x: x, midY: r.midY)
            case .bars(let values): Self.drawBars(values, x: x, midY: r.midY, color: Self.ink(item))
            case .gauge(let v): Self.drawGauge(v, x: x, midY: r.midY, color: Self.ink(item))
            }
            x += Self.width(part) + Self.partGap
        }
        drawn.append((e, r))
        tips.append((r, item.tip ?? "\(e.name): \(item.spoken)"))
        if let action = item.action {
            let name = e.name
            hitRects.append(HitRegion(rect: r, label: item.spoken, value: item.text) { [weak self] in self?.onAction?(name, action) })
        }
    }

    private enum Part {
        case text(NSAttributedString), bars([Double]), gauge(Double)
    }

    private static func parts(_ item: FooterItem) -> [Part] {
        var parts: [Part] = []
        if let label = item.label, !label.isEmpty { parts.append(.text(NSAttributedString(string: label, attributes: Theme.attrs(11, Theme.muted)))) }
        if !item.bars.isEmpty { parts.append(.bars(item.bars)) }
        if let g = item.gauge { parts.append(.gauge(g)) }
        if let text = item.text, !text.isEmpty {
            parts.append(.text(NSAttributedString(string: text, attributes: Theme.attrs(11, item.color?.textColor ?? Theme.fg))))
        }
        return parts
    }

    /// Breite eines Moduls ohne Fuge: `pad` je Seite, `partGap` zwischen den Teilen.
    static func width(_ item: FooterItem) -> CGFloat {
        let parts = parts(item)
        return parts.reduce(2 * pad) { $0 + width($1) } + CGFloat(max(0, parts.count - 1)) * partGap
    }

    private static func width(_ part: Part) -> CGFloat {
        switch part {
        case .text(let s): ceil(s.size().width)
        case .bars(let values): CGFloat(values.count) * (barWidth + barGap) - barGap
        case .gauge: gaugeWidth
        }
    }

    /// Farbe von Balken und Pegel: die des Eintrags, sonst die Textfarbe des Schemas.
    private static func ink(_ item: FooterItem) -> NSColor { item.color?.nsColor ?? Theme.sub }

    /// Senkrecht mittig wie in der oberen Leiste: zentriert wird die Höhe der Großbuchstaben, nicht die Schriftbox.
    private static func drawCentered(_ s: NSAttributedString, x: CGFloat, midY: CGFloat) {
        let f = (s.length > 0 ? s.attribute(.font, at: 0, effectiveRange: nil) as? NSFont : nil) ?? Theme.font(11)
        s.draw(at: CGPoint(x: x, y: midY - f.ascender + f.capHeight / 2))
    }

    /// Je Wert ein Balken auf gedämpfter Spur, gefüllt von unten. Kanten auf Gerätepixeln.
    private static func drawBars(_ values: [Double], x: CGFloat, midY: CGFloat, color: NSColor) {
        let top = Instrument.snapped(midY - barHeight / 2), bottom = Instrument.snapped(midY + barHeight / 2)
        let track = color.withAlphaComponent(color.alphaComponent * 0.22)
        for (i, v) in values.enumerated() {
            let left = Instrument.snapped(x + CGFloat(i) * (barWidth + barGap))
            let right = Instrument.snapped(x + CGFloat(i) * (barWidth + barGap) + barWidth)
            fill(CGRect(x: left, y: top, width: right - left, height: bottom - top), track)
            let fillTop = Instrument.snapped(bottom - (bottom - top) * CGFloat(v))
            if fillTop < bottom { fill(CGRect(x: left, y: fillTop, width: right - left, height: bottom - fillTop), color) }
        }
    }

    /// Waagerechter Pegel: gedämpfte Spur, von links gefüllt.
    private static func drawGauge(_ v: Double, x: CGFloat, midY: CGFloat, color: NSColor) {
        let left = Instrument.snapped(x), right = Instrument.snapped(x + gaugeWidth)
        let top = Instrument.snapped(midY - gaugeHeight / 2), bottom = Instrument.snapped(midY + gaugeHeight / 2)
        fill(CGRect(x: left, y: top, width: right - left, height: bottom - top), color.withAlphaComponent(color.alphaComponent * 0.22))
        let end = Instrument.snapped(x + gaugeWidth * CGFloat(v))
        if end > left { fill(CGRect(x: left, y: top, width: end - left, height: bottom - top), color) }
    }

    private static func fill(_ r: CGRect, _ color: NSColor) {
        color.setFill()
        NSBezierPath(rect: r).fill()
    }

    // MARK: Tooltips, Maus, VoiceOver

    /// Nur bei geändertem Stand neu registrieren, sonst reißt jedes Neuzeichnen den Hover-Timer ab.
    private func updateToolTips() {
        let key = tips.map { "\($0.rect)|\($0.text)" }.joined(separator: "\n")
        guard key != tipsKey else { return }
        tipsKey = key
        removeAllToolTips()
        for t in tips { addToolTip(t.rect.scaled(Theme.scale), owner: self, userData: nil) }
    }

    func view(_ view: NSView, stringForToolTip tag: NSView.ToolTipTag, point: NSPoint, userData data: UnsafeMutableRawPointer?) -> String {
        tips.first { $0.rect.scaled(Theme.scale).contains(point) }?.text ?? ""
    }

    override func mouseDown(with event: NSEvent) {
        let v = convert(event.locationInWindow, from: nil)
        hitRects.first(at: CGPoint(x: v.x / Theme.scale, y: v.y / Theme.scale))?.action()
    }

    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .group }
    override func accessibilityLabel() -> String? { String(localized: "Statusleiste der Extensions", bundle: Bundle.app) }

    /// Einträge mit Aktion als Knöpfe, die übrigen als Text, in der Reihenfolge der Leiste.
    override func accessibilityChildren() -> [Any]? {
        let texts = drawn.filter { $0.entry.item.action == nil }.map { d in
            a11y.reuse("text-\(d.entry.name)-\(d.entry.item.id)").update(parent: self, role: .staticText, label: d.entry.item.spoken,
                                                                         value: d.entry.item.tip, frame: d.rect.scaled(Theme.scale))
        }
        a11y = hitRects.accessibilityElements(parent: self, reusing: a11y) + texts
        return a11y
    }
}
