import AppKit

/// Sessions je Status für die Leiste. `detached` = kein laufender Claude-Prozess, egal welcher Status zuletzt galt.
struct StatusCounts: Equatable {
    var running = 0, waiting = 0, idle = 0, error = 0, detached = 0
    var total: Int { running + waiting + idle + error + detached }
}

/// 30 pt Leiste im Instrument-Look: links Tasten für Layout und Schalter, rechts das Status-Display mit Uhr,
/// daneben Tasten und flache Module, getrennt durch Fugen. Die Mitte bleibt frei für Einträge der Extensions.
/// Alles gezeichnet, Hit-Rects von Hand.
@MainActor
final class StatusBarView: NSView, NSViewToolTipOwner {
    /// Sessions können nicht geladen werden: steht statt des Status im Display, rot.
    var errorText: String?
    /// claude läuft, ist aber älter als von Kadrell getestet: blockiert nichts, nur ein Hinweis rechts.
    var versionWarning: String?
    /// Einmaliger Tipp (zweite Session, Bedeutung von Gelb), verschwindet mit einem Klick darauf.
    var tip: String?
    var onDismissTip: (() -> Void)?
    /// Sessions je Status für das Display; alles außer `detached` hat einen laufenden Claude-Prozess.
    var counts = StatusCounts()
    /// Fertige Sessions, die sich trennen lassen (ohne Terminals ohne Claude und Remote-Kacheln): Zahl auf der Taste.
    var detachableCount = 0
    /// Klick auf die grüne Taste: alle fertigen Sessions trennen (spart Speicher und CPU).
    var onDetachIdle: (() -> Void)?
    /// Sessions, die gerade auf dich warten (Status waiting, angehängt). Modul „N warten“ nur bei > 0.
    var waitingCount = 0
    var onSelectWaiting: (() -> Void)?
    /// Neuere Version als die laufende gefunden (`UpdateChecker`), Klick zeigt Changelog und Download-Link.
    var updateAvailable: UpdateManifest?
    var onShowUpdate: (() -> Void)?
    /// Hinterlegte Claude-Accounts für die Konto-Taste (Global-Swap). Leer = keine Taste. `detail` = gecachte Nutzung.
    var accounts: [(id: String, title: String, active: Bool, detail: String)] = []
    /// Auto-Wechsel aktiv, Häkchen im Konto-Menü.
    var autoswitch = false
    var onSwitchAccount: ((String) -> Void)?
    var onAddAccount: (() -> Void)?
    var onManageAccounts: (() -> Void)?
    var onToggleAutoswitch: (() -> Void)?
    /// Fläche der Konto-Taste beim letzten Zeichnen, damit das Menü darunter aufklappt.
    private var accountRect: CGRect = .zero
    /// Neue Werte zählen vom alten Stand hoch bzw. herunter.
    var usage = Usage.empty {
        didSet { if usage != oldValue { usageFrom = oldValue; usageAt = CACurrentMediaTime(); animate(0.5) } }
    }
    private var usageFrom = Usage.empty
    private var usageAt: CFTimeInterval = 0
    /// Wann `usage` zuletzt erfolgreich abgefragt wurde. Ist das länger her als `usageStaleAfter` (etwa weil der
    /// Endpunkt mit 429 ablehnt), stehen die Werte grau da und der Tooltip nennt das Alter.
    var usageMeasured: Date?
    static let usageStaleAfter: TimeInterval = 600
    var layoutMode: LayoutMode = .grid
    /// Klick aufs Layout-Symbol: Auswahl aller Layouts.
    var onPickLayout: ((LayoutMode) -> Void)?
    /// Spalten im Grid, 0 = automatisch. Nur im Grid sichtbar: ‹ weniger, › mehr.
    var gridColumns = 0
    var onGridColumns: ((Int) -> Void)?
    /// Sichtbare Spalten beim Scrollen, mindestens 1. Nur beim Scrollen sichtbar.
    var scrollColumns = 2
    var onScrollColumns: ((Int) -> Void)?
    /// Frei: Teilung an der Fokus-Kachel, „r“ → , „d“ ↓, „a“ längere Seite. Klick schaltet reihum.
    var split: Character = "a"
    var onSplit: ((Character) -> Void)?
    /// Auto-Modus: nur Sessions, die etwas wollen. An = eingesunkene, gefüllte Taste.
    var auto = false
    var onToggleAuto: (() -> Void)?
    /// Sync: Eingaben gehen an alle Kacheln. An = rote Taste, damit es niemand vergisst.
    var sync = false
    var onToggleSync: (() -> Void)?
    /// Sortierung des Baums: Klick schaltet aus → A–Z → Status weiter. Aktiv = eingesunkene, gefüllte Taste.
    var sort: SidebarSort = .off
    var onCycleSort: (() -> Void)?
    /// Gruppierung nach Projekt im Baum. Aus = flache Liste, dann zeigt die Taste „FLACH“ eingesunken und gefüllt.
    var grouped = true
    var onToggleGrouping: (() -> Void)?
    /// Zoom aktiv: Taste „ZOOM“ am Ende der linken Seite, Klick hebt den Zoom auf.
    var zoomed = false
    var onToggleZoom: (() -> Void)?

    /// Statusanzeigen der Extensions, rechts neben den Nutzungswerten. Der Manager meldet bis zu 50-mal pro Sekunde;
    /// gezeichnet wird nur bei geändertem Stand. Passt nicht alles in die Leiste, fallen die linken Einträge weg.
    var extensionItems: [(name: String, text: String, color: ThemeColor?, action: String?)] = [] {
        didSet { if !Self.sameItems(extensionItems, oldValue) { needsDisplay = true } }
    }
    static func sameItems(_ a: [(name: String, text: String, color: ThemeColor?, action: String?)],
                          _ b: [(name: String, text: String, color: ThemeColor?, action: String?)]) -> Bool {
        a.count == b.count && zip(a, b).allSatisfy { $0 == $1 }
    }
    /// Klick auf einen Eintrag mit Aktion: Name der Extension und Aktions-ID.
    var onExtensionAction: ((_ name: String, _ id: String) -> Void)?

    /// Trefferflächen mit Label und Wert, dieselbe Liste liefert die Knöpfe für VoiceOver.
    private var hitRects: [HitRegion] = []
    /// Tooltip-Text nur für Module ohne eigenen Klick (Nutzungszahlen, Kürzel „läuft x/y“); die anklickbaren
    /// Module bekommen ihren Tooltip aus `hitRects` (Label + Wert).
    private var moduleTips: [(rect: CGRect, text: String)] = []
    /// Registrierte Tooltip-Rects fürs Nachschlagen in `view(_:stringForToolTip:point:userData:)`, nur neu
    /// gesetzt, wenn sich seit dem letzten Zeichnen etwas geändert hat: sonst reißt `removeAllToolTips()` bei
    /// jedem Uhr-Tick (jede Sekunde) den Hover-Timer ab, und der Tooltip erscheint nie.
    private var toolTipEntries: [(rect: CGRect, text: String)] = []
    private var toolTipsKey = ""
    private var a11y: [A11yElement] = []
    /// Display beim letzten Zeichnen: Rahmen und vorgelesener Text (Langform bzw. Fehlermeldung), auch als Tooltip.
    private var status: (rect: CGRect, text: String) = (.zero, "")
    private let statusA11y = A11yElement(key: "status-display")
    /// Welche Form das Display zuletzt gezeichnet hat: kurz erst, wenn rechts sonst nichts mehr wegfallen kann.
    private(set) var displayCompact = false
    /// Rahmen aller Tasten, flachen Module und des Displays beim letzten Zeichnen, in unskalierten Punkten.
    private(set) var drawnFrames: [CGRect] = []
    /// Fortschritt des Hochzählens der Nutzung, einmal je Zeichnen festgehalten: Messen und Zeichnen sehen dieselben Zahlen.
    private var usageProgress: CGFloat?
    private var clockTask: Task<Void, Never>?
    private static let clock: DateFormatter = { let df = DateFormatter(); df.dateFormat = "HH:mm"; return df }()
    /// Relative Reset-Zeit im Tooltip („in 2 Std.“), Sprache folgt der eingestellten Oberfläche.
    private static let relative: RelativeDateTimeFormatter = { let f = RelativeDateTimeFormatter(); f.unitsStyle = .abbreviated; return f }()
    /// Zuletzt geklickte Trefferfläche: ihre Taste sitzt danach kurz eingesunken.
    private var pressedLabel: String?
    private var pressedAt: CFTimeInterval = 0
    private var animUntil: CFTimeInterval = 0
    private var animTask: Task<Void, Never>?

    /// Maße der Tasten und des Displays: 20 pt hoch, 4 pt Abstand untereinander, 6 pt Rand zu den Kanten.
    private static let keyHeight: CGFloat = 20, keyGap: CGFloat = 4, edge: CGFloat = 6
    private static let layoutIconSize: CGFloat = 10

    /// Kurz flüssig neu zeichnen, die Uhr allein tickt nur jede Sekunde.
    private func animate(_ duration: CFTimeInterval) {
        animUntil = max(animUntil, CACurrentMediaTime() + duration)
        guard animTask == nil else { return }
        animTask = Task { [weak self] in
            while let self, CACurrentMediaTime() < self.animUntil {
                self.needsDisplay = true
                try? await Task.sleep(for: .milliseconds(16))
            }
            self?.needsDisplay = true
            self?.animTask = nil
        }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        clockTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self else { return }
                // Fenster verdeckt/versteckt: die Uhr darf ruhig eine Sekunde nachhängen, sie zeichnet niemand.
                guard self.window?.occlusionState.contains(.visible) == true else { continue }
                self.needsDisplay = true
            }
        }
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }

    /// Gezeichnet in unskalierten Punkten, die Hit-Rects auch.
    override func draw(_ dirtyRect: NSRect) {
        Theme.scaled(bounds) { drawBar($0) }
        updateToolTips()
    }

    private func updateToolTips() {
        toolTipEntries = moduleTips + hitRects.map { h in (h.rect, h.value.map { "\(h.label): \($0)" } ?? h.label) }
        let key = toolTipEntries.map { "\($0.rect)|\($0.text)" }.joined(separator: "\n")
        guard key != toolTipsKey else { return }
        toolTipsKey = key
        removeAllToolTips()
        for t in toolTipEntries { addToolTip(t.rect.scaled(Theme.scale), owner: self, userData: nil) }
    }

    func view(_ view: NSView, stringForToolTip tag: NSView.ToolTipTag, point: NSPoint, userData data: UnsafeMutableRawPointer?) -> String {
        toolTipEntries.first { $0.rect.scaled(Theme.scale).contains(point) }?.text ?? ""
    }

    /// Misst erst und zeichnet dann einmal: die rechte Seite verliert so lange Teile (Reihenfolge in `BarPlan`), bis
    /// sie mit 16 pt Abstand neben die linken Tasten passt.
    private func drawBar(_ b: CGRect) {
        hitRects = []
        moduleTips = []
        drawnFrames = []
        usageProgress = Feedback.progress(since: usageAt, duration: 0.5)
        Theme.panel.setFill(); b.fill()
        let content = CGRect(x: 0, y: 0, width: b.width, height: b.height - 2 * Instrument.hairline)
        Instrument.hSeam(x: 0, y: content.maxY, width: b.width)
        let limit = drawLeft(content) + 16
        var rx = b.width - Self.edge
        let plan = BarPlan.fit(extensions: extensionItems.count, room: rx - limit) { rightParts(content, $0).reduce(0) { $0 + $1.width } }
        displayCompact = plan.compact
        for part in rightParts(content, plan) { part.draw(&rx) }
    }

    /// Zeichnet die Zeile senkrecht mittig auf `midY`, für jede Schriftgröße gleich. Die Leiste ist flipped, der
    /// Zeichenpunkt ist also die Oberkante der Textbox. Zentriert werden die Großbuchstaben/Ziffern (capHeight über
    /// der Grundlinie), nicht die volle JetBrains-NF-Box mit ihrem großen Unterlängen-Rand, sonst säße alles zu hoch.
    /// Ist `width` schmaler als der Text, endet er mit „…“.
    private func drawCentered(_ s: NSAttributedString, x: CGFloat, midY: CGFloat, width: CGFloat = .greatestFiniteMagnitude) {
        let f = (s.length > 0 ? s.attribute(.font, at: 0, effectiveRange: nil) as? NSFont : nil) ?? Theme.font(11)
        let origin = CGPoint(x: x, y: midY - f.ascender + f.capHeight / 2)
        guard width < s.size().width else { s.draw(at: origin); return }
        s.draw(with: CGRect(origin: origin, size: CGSize(width: width, height: ceil(s.size().height))),
               options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    // MARK: Tasten

    /// Rechteck einer Taste ab `x`, senkrecht mittig in der Leiste, oben aufs Gerätepixel eingerastet.
    private func keyRect(x: CGFloat, width: CGFloat, _ b: CGRect) -> CGRect {
        CGRect(x: x, y: Instrument.snapped(b.midY - Self.keyHeight / 2), width: width, height: Self.keyHeight)
    }

    /// Trefferfläche einer Taste: so breit wie die Taste, aber über die ganze Höhe der Leiste.
    private static func hitRect(_ r: CGRect, _ b: CGRect) -> CGRect {
        CGRect(x: r.minX, y: b.minY, width: r.width, height: b.height)
    }

    /// Zeichnet die Taste der Trefferfläche `label`: eingeschaltet (`on`) sinkt sie ein, nach einem Klick kurz auch.
    /// Gibt die Kappe zurück, dort sitzt die Beschriftung.
    @discardableResult
    private func key(_ r: CGRect, _ label: String, on: Bool = false, fill: NSColor? = nil) -> CGRect {
        drawnFrames.append(r)
        return Instrument.key(r, pressed: on || isFlashing(label), fill: fill)
    }

    private func isFlashing(_ label: String) -> Bool {
        pressedLabel == label && CACurrentMediaTime() - pressedAt < 0.08
    }

    /// Nach einem Klick sinkt die getroffene Taste 80 ms ein, bei „Bewegung reduzieren“ nicht.
    private func flash(_ label: String) {
        guard !Feedback.reduceMotion else { return }
        pressedLabel = label
        pressedAt = CACurrentMediaTime()
        animate(0.08)
    }

    // MARK: Links

    /// Layout-Taste, Spalten bzw. Teilrichtung, AUTO/SYNC/GRP/SORT und ZOOM. Gibt das Ende der linken Seite zurück.
    private func drawLeft(_ b: CGRect) -> CGFloat {
        // Layout-Taste: zeigt den aktuellen Modus, Klick öffnet die Auswahl.
        let label = String(localized: "Layout", bundle: Bundle.app)
        let toggle = keyRect(x: Self.edge, width: 26, b)
        let cap = key(toggle, label)
        // Symbol so hoch wie die Großbuchstaben der Nachbartasten, nicht kappenfüllend.
        let icon = Self.layoutIconSize
        Icons.layout(layoutMode, in: CGRect(x: Instrument.snapped(cap.midX - icon / 2), y: Instrument.snapped(cap.midY - icon / 2), width: icon, height: icon), color: Theme.sub)
        hitRects.append(HitRegion(rect: Self.hitRect(toggle, b), label: label, value: layoutMode.title) { [weak self] in self?.showLayoutMenu(at: CGPoint(x: toggle.minX, y: b.maxY)) })
        var x = toggle.maxX + Self.keyGap
        if layoutMode == .grid { drawColumns(b, &x, value: gridColumns, min: 0) { [weak self] c in self?.onGridColumns?(c) } }
        if layoutMode == .scroll { drawColumns(b, &x, value: scrollColumns, min: 1) { [weak self] c in self?.onScrollColumns?(c) } }
        if layoutMode == .custom { drawSplit(b, &x) }
        let onOff = { (on: Bool) in on ? String(localized: "an", bundle: Bundle.app) : String(localized: "aus", bundle: Bundle.app) }
        segment(b, &x, "AUTO", fill: auto ? Theme.waiting : nil, String(localized: "Auto-Modus", bundle: Bundle.app), onOff(auto)) { [weak self] in self?.onToggleAuto?() }
        segment(b, &x, "SYNC", fill: sync ? Theme.error : nil, "Sync", onOff(sync)) { [weak self] in self?.onToggleSync?() }
        segment(b, &x, grouped ? "GRP" : String(localized: "FLACH", bundle: Bundle.app), fill: grouped ? nil : Theme.sub,
                String(localized: "Gruppierung nach Projekt", bundle: Bundle.app), onOff(grouped)) { [weak self] in self?.onToggleGrouping?() }
        let sortLabel = switch sort { case .off: "SORT"; case .alpha: "A–Z"; case .status: "STATUS" }
        segment(b, &x, sortLabel, fill: sort == .off ? nil : Theme.sub, String(localized: "Sortierung", bundle: Bundle.app), sort == .off ? String(localized: "aus", bundle: Bundle.app) : sortLabel) { [weak self] in self?.onCycleSort?() }
        drawZoom(b, &x)
        return x - Self.keyGap
    }

    /// Schalter als Taste. `fill` = an: eingesunken in dieser Farbe, sonst erhaben mit gedämpftem Text.
    private func segment(_ b: CGRect, _ x: inout CGFloat, _ text: String, color: NSColor? = nil, fill: NSColor? = nil,
                         _ label: String, _ value: String?, action: @escaping @MainActor () -> Void) {
        let t = NSAttributedString(string: text, attributes: Theme.attrs(10, color ?? fill.map { Theme.pillText(on: $0) } ?? Theme.muted, bold: true))
        let r = keyRect(x: x, width: t.size().width + 16, b)
        let cap = key(r, label, on: fill != nil, fill: fill)
        drawCentered(t, x: cap.minX + 8, midY: cap.midY)
        hitRects.append(HitRegion(rect: Self.hitRect(r, b), label: label, value: value, action: action))
        x = r.maxX + Self.keyGap
    }

    /// ‹ AUTO SP › bzw. ‹ 3 SP ›: die Pfeiltasten ändern die Spaltenzahl. `min` 0 lässt sie unter 1 wieder automatisch werden.
    private func drawColumns(_ b: CGRect, _ x: inout CGFloat, value cols: Int, min lo: Int, onChange: @escaping (Int) -> Void) {
        let value = NSAttributedString(string: cols == 0 ? String(localized: "AUTO SP", bundle: Bundle.app) : String(localized: "\(cols) SP", bundle: Bundle.app), attributes: Theme.attrs(10, cols == 0 ? Theme.muted : Theme.fg, bold: true))
        let spoken = cols == 0 ? String(localized: "automatisch", bundle: Bundle.app) : "\(cols)"
        let less = arrowKey(b, x: x, "‹", String(localized: "Weniger Spalten", bundle: Bundle.app), value: spoken) { onChange(Swift.max(lo, cols - 1)) }
        drawCentered(value, x: less.maxX + 6, midY: less.midY - 1)
        let more = arrowKey(b, x: less.maxX + 12 + value.size().width, "›", String(localized: "Mehr Spalten", bundle: Bundle.app), value: spoken) { onChange(Swift.min(12, cols + 1)) }
        x = more.maxX + Self.keyGap
    }

    /// Kleine Taste mit Pfeil für die Spaltenzahl, gibt ihr Rechteck zurück.
    private func arrowKey(_ b: CGRect, x: CGFloat, _ arrow: String, _ label: String, value: String, action: @escaping @MainActor () -> Void) -> CGRect {
        let t = NSAttributedString(string: arrow, attributes: Theme.attrs(12, Theme.sub, bold: true))
        let r = keyRect(x: x, width: t.size().width + 12, b)
        let cap = key(r, label)
        drawCentered(t, x: cap.minX + 6, midY: cap.midY)
        hitRects.append(HitRegion(rect: Self.hitRect(r, b), label: label, value: value, action: action))
        return r
    }

    /// Teilrichtung der nächsten Kachel im Layout „Frei“, Klick schaltet auto → rechts → unten weiter.
    private func drawSplit(_ b: CGRect, _ x: inout CGFloat) {
        let next: Character = split == "a" ? "r" : split == "r" ? "d" : "a"
        let (text, spoken) = switch split {
        case "r": (String(localized: "TEILT →", bundle: Bundle.app), String(localized: "rechts", bundle: Bundle.app))
        case "d": (String(localized: "TEILT ↓", bundle: Bundle.app), String(localized: "unten", bundle: Bundle.app))
        default: (String(localized: "TEILT AUTO", bundle: Bundle.app), String(localized: "automatisch", bundle: Bundle.app))
        }
        segment(b, &x, text, color: split == "a" ? Theme.muted : Theme.fg, String(localized: "Nächste Kachel teilt", bundle: Bundle.app), spoken) { [weak self] in self?.onSplit?(next) }
    }

    /// ZOOM als eingesunkene Taste, solange der Zoom aktiv ist.
    private func drawZoom(_ b: CGRect, _ x: inout CGFloat) {
        guard zoomed else { return }
        segment(b, &x, "ZOOM", fill: Theme.waiting, String(localized: "Zoom aufheben", bundle: Bundle.app), nil) { [weak self] in self?.onToggleZoom?() }
    }

    // MARK: Rechts, von rechts nach links

    /// Ein Teil der rechten Seite: Breite samt Abstand zum linken Nachbarn, gezeichnet ab `rx` nach links. Messen und
    /// Zeichnen nutzen dieselben Teile, deshalb kann nichts breiter ausfallen als gemessen.
    private struct BarPart {
        let width: CGFloat
        let draw: (inout CGFloat) -> Void
    }

    /// Alle Teile der rechten Seite nach `plan`, von rechts: Display, Tasten, Tipp und Versionswarnung, Nutzung,
    /// Einträge der Extensions.
    private func rightParts(_ b: CGRect, _ plan: BarPlan) -> [BarPart] {
        var parts = [displayPart(b, plan)]
        let keys = noticeKeyParts(b, plan)
        parts += keys
        // Nach der letzten Taste derselbe Rand wie nach dem Display, dann folgt die Fuge des ersten flachen Moduls.
        if !keys.isEmpty { parts.append(BarPart(width: Self.edge - Self.keyGap) { $0 -= Self.edge - Self.keyGap }) }
        return parts + noticeModuleParts(b, plan) + usageParts(b, plan) + extensionParts(b, plan)
    }

    /// Schwarzes Display ganz rechts: Lampe, Status in Worten (bzw. die Fehlermeldung) und die Uhr. Tooltip und
    /// VoiceOver bekommen die Langform aller Zustände plus die Nutzungswerte, die aus Platzmangel fehlen. Der Text
    /// wird um `plan.shrink` gekürzt, damit er nicht über die linken Tasten wächst; Tooltip und VoiceOver behalten ihn ganz.
    private func displayPart(_ b: CGRect, _ plan: BarPlan) -> BarPart {
        let text = NSAttributedString(string: errorText ?? StatusDisplay.text(counts, compact: plan.compact),
                                      attributes: Theme.attrs(10.5, errorText == nil ? Instrument.screenInk : Self.screenError, bold: true))
        let time = NSAttributedString(string: Self.clock.string(from: Date()), attributes: Theme.attrs(10.5, Instrument.screenMuted))
        let textWidth = max(0, text.size().width - plan.shrink)
        let w = 10 + 7 + 6 + textWidth + (textWidth > 0 ? 8 : 0) + time.size().width + 10
        let spoken = (errorText ?? StatusDisplay.spoken(counts)) + droppedUsage(plan)
        return BarPart(width: w + Self.edge) { [self] rx in
            let r = keyRect(x: rx - w, width: w, b)
            Instrument.screen(r)
            drawnFrames.append(r)
            lampColor.setFill()
            NSBezierPath(ovalIn: CGRect(x: r.minX + 10, y: r.midY - 3.5, width: 7, height: 7)).fill()
            drawCentered(text, x: r.minX + 23, midY: r.midY, width: textWidth)
            drawCentered(time, x: r.maxX - 10 - time.size().width, midY: r.midY)
            status = (r, spoken)
            moduleTips.append(status)
            rx = r.minX - Self.edge
        }
    }

    /// Lampe im Display: bei einer Fehlermeldung rot, sonst nach `StatusDisplay.lamp`.
    private var lampColor: NSColor {
        if errorText != nil { return Theme.error }
        switch StatusDisplay.lamp(counts) {
        case .waiting: return Theme.waiting
        case .error: return Theme.error
        case .ok: return Theme.idle
        case .off: return Instrument.screenMuted
        }
    }

    /// Fehlerrot, so weit Richtung Weiß gemischt, dass es auf dem schwarzen Display lesbar ist (mindestens 4,5:1).
    private static var screenError: NSColor {
        Theme.error.ensuringContrast(4.5, against: [Instrument.screenFace], toward: .white)
    }

    /// Taste links von `rx`, danach 4 pt Abstand. Gibt ihr Rechteck zurück.
    @discardableResult
    private func keyModule(_ b: CGRect, _ rx: inout CGFloat, _ text: NSAttributedString, fill: NSColor? = nil,
                           _ label: String, value: String?, action: @escaping @MainActor () -> Void) -> CGRect {
        let r = keyRect(x: rx - text.size().width - 16, width: text.size().width + 16, b)
        let cap = key(r, label, fill: fill)
        drawCentered(text, x: cap.minX + 8, midY: cap.midY)
        hitRects.append(HitRegion(rect: Self.hitRect(r, b), label: label, value: value, action: action))
        rx = r.minX - Self.keyGap
        return r
    }

    /// `keyModule` als Teil der rechten Seite.
    private func keyPart(_ b: CGRect, _ text: NSAttributedString, fill: NSColor? = nil, _ label: String, value: String?,
                         action: @escaping @MainActor () -> Void) -> BarPart {
        BarPart(width: text.size().width + 16 + Self.keyGap) { [self] rx in keyModule(b, &rx, text, fill: fill, label, value: value, action: action) }
    }

    /// Flaches Modul links von `rx`, rechts davon eine Fuge zum Nachbarn.
    private func module(_ b: CGRect, _ rx: inout CGFloat, _ parts: [NSAttributedString], tip: String? = nil,
                        _ label: String? = nil, value: String? = nil, action: (@MainActor () -> Void)? = nil) {
        let seam = 2 * Instrument.hairline
        Instrument.vSeam(x: rx - seam, y: 0, height: b.height)
        let rect = CGRect(x: rx - seam - Self.moduleWidth(parts), y: 0, width: Self.moduleWidth(parts), height: b.height)
        drawnFrames.append(rect)
        var px = rect.minX + 10
        for p in parts {
            drawCentered(p, x: px, midY: b.midY); px += p.size().width + 5
        }
        if let tip { moduleTips.append((rect, tip)) }
        if let label, let action { hitRects.append(HitRegion(rect: rect, label: label, value: value, action: action)) }
        rx = rect.minX
    }

    /// `module` als Teil der rechten Seite: Fuge plus Modul.
    private func modulePart(_ b: CGRect, _ parts: [NSAttributedString], tip: String? = nil,
                            _ label: String? = nil, value: String? = nil, action: (@MainActor () -> Void)? = nil) -> BarPart {
        BarPart(width: 2 * Instrument.hairline + Self.moduleWidth(parts)) { [self] rx in
            module(b, &rx, parts, tip: tip, label, value: value, action: action)
        }
    }

    /// Breite eines flachen Moduls ohne Fuge: 10 pt Rand je Seite, 5 pt zwischen den Teilen.
    private static func moduleWidth(_ parts: [NSAttributedString]) -> CGFloat {
        parts.reduce(20) { $0 + $1.size().width } + CGFloat(max(0, parts.count - 1)) * 5
    }

    /// Konto-Taste: aktiver Account, Klick öffnet das Menü zum Wechseln, Hinzufügen und Verwalten. Ohne Accounts
    /// keine Taste. Ist es eng (`short`), zeigt sie nur die ersten zehn Zeichen.
    private func accountPart(_ b: CGRect, short: Bool) -> BarPart? {
        guard !accounts.isEmpty else { return nil }
        let title = accounts.first { $0.active }?.title ?? String(localized: "Konto wählen", bundle: Bundle.app)
        let limit = short ? 10 : 26
        let shown = title.count > limit ? String(title.prefix(limit - 1)) + "…" : title
        let text = NSAttributedString(string: shown, attributes: Theme.attrs(11, Theme.fg))
        return BarPart(width: text.size().width + 16 + Self.keyGap) { [self] rx in
            accountRect = keyModule(b, &rx, text, String(localized: "Aktives Konto", bundle: Bundle.app), value: title) { [weak self] in self?.showAccountMenu() }
        }
    }

    private func showAccountMenu() {
        let menu = NSMenu()
        for a in accounts {
            let item = NSMenuItem(title: a.detail.isEmpty ? a.title : "\(a.title)   \(a.detail)", action: #selector(pickAccount(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = a.id
            item.state = a.active ? .on : .off
            menu.addItem(item)
        }
        menu.addItem(.separator())
        let auto = NSMenuItem(title: String(localized: "Automatisch wechseln bei Limit", bundle: Bundle.app), action: #selector(toggleAutoswitch), keyEquivalent: "")
        auto.target = self
        auto.state = autoswitch ? .on : .off
        menu.addItem(auto)
        for (title, sel) in [(String(localized: "Konto hinzufügen …", bundle: Bundle.app), #selector(addAccountItem)),
                             (String(localized: "Konten verwalten …", bundle: Bundle.app), #selector(manageAccounts))] {
            let item = NSMenuItem(title: title, action: sel, keyEquivalent: "")
            item.target = self
            menu.addItem(item)
        }
        menu.popUp(positioning: nil, at: CGPoint(x: accountRect.minX * Theme.scale, y: accountRect.maxY * Theme.scale), in: self)
    }

    @objc private func pickAccount(_ item: NSMenuItem) { if let id = item.representedObject as? String { onSwitchAccount?(id) } }
    @objc private func toggleAutoswitch() { onToggleAutoswitch?() }
    @objc private func addAccountItem() { onAddAccount?() }
    @objc private func manageAccounts() { onManageAccounts?() }

    /// Von rechts: „N warten“, „N trennen“, Update und Konto, jeweils nur wenn es etwas zu tun gibt.
    private func noticeKeyParts(_ b: CGRect, _ plan: BarPlan) -> [BarPart] {
        var parts: [BarPart] = []
        if waitingCount > 0 {
            parts.append(keyPart(b, NSAttributedString(string: String(localized: "\(waitingCount) warten", bundle: Bundle.app), attributes: Theme.attrs(11, Theme.pillText(on: Theme.waiting), bold: true)), fill: Theme.waiting,
                                 String(localized: "Wartende Sessions", bundle: Bundle.app), value: "\(waitingCount)") { [weak self] in self?.onSelectWaiting?() })
        }
        if detachableCount > 0 {
            parts.append(keyPart(b, NSAttributedString(string: String(localized: "\(detachableCount) trennen", bundle: Bundle.app), attributes: Theme.attrs(11, Theme.pillText(on: Theme.idle), bold: true)), fill: Theme.idle,
                                 String(localized: "Fertige Sessions trennen", bundle: Bundle.app), value: "\(detachableCount)") { [weak self] in self?.onDetachIdle?() })
        }
        if let update = updateAvailable, plan.update {
            // Dezent statt der waiting-Farbe: kein Alarm, nur ein Hinweis.
            parts.append(keyPart(b, NSAttributedString(string: String(localized: "\(update.version) verfügbar", bundle: Bundle.app), attributes: Theme.attrs(11, Theme.fg)),
                                 String(localized: "Update verfügbar", bundle: Bundle.app), value: update.version) { [weak self] in self?.onShowUpdate?() })
        }
        if plan.account, let account = accountPart(b, short: plan.shortAccount) { parts.append(account) }
        return parts
    }

    /// Links der Tasten: Tipp, dann die Versionswarnung.
    private func noticeModuleParts(_ b: CGRect, _ plan: BarPlan) -> [BarPart] {
        var parts: [BarPart] = []
        if let tip, plan.tip {
            parts.append(modulePart(b, [NSAttributedString(string: tip + "  ×", attributes: Theme.attrs(10.5, Theme.waiting))],
                                    String(localized: "Tipp", bundle: Bundle.app), value: tip) { [weak self] in self?.onDismissTip?() })
        }
        if versionWarning != nil, plan.version {
            parts.append(modulePart(b, [NSAttributedString(string: String(localized: "claude alt · claude update", bundle: Bundle.app), attributes: Theme.attrs(10.5, Theme.waiting, bold: true))],
                                    String(localized: "Ältere claude-Version", bundle: Bundle.app), value: versionWarning) { NSPasteboard.general.copy(ClaudeCLI.updateCommand) })
        }
        return parts
    }

    /// Farbe eines Nutzungswerts. Mit Plan (7 Tage) zählt der Vorsprung: über dem Plan rot, dicht darunter gelb,
    /// sonst grün. Ohne Plan bleibt es beim absoluten Stand.
    static func usageColor(_ v: Int, plan: Int?) -> NSColor {
        guard let plan else { return v >= 90 ? Theme.error : v >= 70 ? Theme.waiting : Theme.idle }
        return v > plan ? Theme.error : v >= plan - Self.usagePlanWarning ? Theme.waiting : Theme.idle
    }

    /// So viele Punkte vor dem Plan wird aus Grün Gelb (bei 7 Tagen rund 8 Stunden Vorlauf).
    static let usagePlanWarning = 5

    /// Minuten seit der letzten Messung, wenn der Stand veraltet ist, sonst nil.
    static func staleMinutes(_ measured: Date?, now: Date = Date()) -> Int? {
        guard let measured else { return nil }
        let age = now.timeIntervalSince(measured)
        return age > usageStaleAfter ? Int(age / 60) : nil
    }

    /// Ein Nutzungswert: Kürzel, Stand, Ausgangswert fürs Hochzählen, Bezeichnung im Tooltip, Plan-Stand, Reset.
    private struct UsageRow {
        let name: String, pct: Int?, from: Int?, label: String, plan: Int?, reset: Date?
    }

    /// Nutzungswerte von rechts: Fable-Woche, 7 Tage, 5 Stunden.
    private var usageRows: [UsageRow] {
        [
            UsageRow(name: "fable", pct: usage.fable, from: usageFrom.fable, label: String(localized: "Fable-Kontingent", bundle: Bundle.app), plan: nil, reset: usage.weeklyResets),
            UsageRow(name: "7d", pct: usage.weekly, from: usageFrom.weekly, label: String(localized: "Claude-Nutzung der letzten 7 Tage", bundle: Bundle.app), plan: usage.weeklyPlan(), reset: usage.weeklyResets),
            UsageRow(name: "5h", pct: usage.session, from: usageFrom.session, label: String(localized: "Claude-Nutzung der letzten 5 Stunden", bundle: Bundle.app), plan: nil, reset: usage.sessionResets),
        ]
    }

    /// Claude-Nutzung als flache Module. Fehlt ein Wert, steht „–%“ statt nichts. Vor dem 7-Tage-Wert steht der
    /// Plan-Stand („25%/32%“): so viel dürfte bei gleichmäßiger Woche jetzt weg sein.
    private func usageParts(_ b: CGRect, _ plan: BarPlan) -> [BarPart] {
        let fMuted = Theme.attrs(11.5, Theme.muted)
        let stale = Self.staleMinutes(usageMeasured)
        let staleTip = stale.map { " · " + String(localized: "Stand von vor \($0) min", bundle: Bundle.app) } ?? ""
        return usageRows.filter { plan.usage.contains($0.name) }.map { row in
            let value = NSMutableAttributedString()
            if let p = row.plan, row.pct != nil { value.append(NSAttributedString(string: "\(p)%/", attributes: fMuted)) }
            value.append(pctString(row, stale: stale != nil))
            return modulePart(b, [NSAttributedString(string: row.name, attributes: fMuted), value], tip: usageTip(row) + resetTip(row.reset) + staleTip)
        }
    }

    /// Prozentwert, beim Hochzählen der Zwischenstand. Veraltet oder ohne Wert gedämpft.
    private func pctString(_ row: UsageRow, stale: Bool) -> NSAttributedString {
        let fMuted = Theme.attrs(11.5, Theme.muted)
        guard let target = row.pct else { return NSAttributedString(string: "–%", attributes: fMuted) }
        var v = target
        if let usageProgress, let from = row.from { v = from + Int((CGFloat(target - from) * usageProgress).rounded()) }
        return NSAttributedString(string: "\(v)%", attributes: stale ? fMuted : Theme.attrs(11.5, Self.usageColor(v, plan: row.plan)))
    }

    /// Tooltip: Bezeichnung plus Prozentwert, beim 7-Tage-Wert zusätzlich der Plan-Stand.
    private func usageTip(_ row: UsageRow) -> String {
        guard let pct = row.pct else { return String(localized: "\(row.label): Nutzung nicht abrufbar", bundle: Bundle.app) }
        guard let plan = row.plan else { return String(localized: "\(row.label): \(pct) % verbraucht", bundle: Bundle.app) }
        return String(localized: "\(row.label): \(pct) % verbraucht, \(plan) % nach Plan", bundle: Bundle.app)
    }

    /// Reset-Zeit ans Tooltip: absolute Uhrzeit plus relativ, leer wenn kein Reset-Zeitpunkt bekannt ist.
    private func resetTip(_ resets: Date?) -> String {
        guard let resets, resets > Date() else { return "" }
        Self.relative.locale = Localization.locale
        let rel = Self.relative.localizedString(for: resets, relativeTo: Date())
        return " · " + String(localized: "Reset \(Self.clock.string(from: resets)) (\(rel))", bundle: Bundle.app)
    }

    /// Nutzungswerte, die aus Platzmangel nicht in der Leiste stehen, für Tooltip und VoiceOver des Displays
    /// („ · 5h 40% · 7d 25%“), in der Reihenfolge der Leiste.
    private func droppedUsage(_ plan: BarPlan) -> String {
        usageRows.reversed().filter { !plan.usage.contains($0.name) }
            .map { " · \($0.name) " + ($0.pct.map { "\($0)%" } ?? "–%") }.joined()
    }

    /// Einträge der Extensions, der erste ganz links. Es bleiben die `plan.extensions` rechten, die vorhandenen
    /// Module rechts bleiben unberührt.
    private func extensionParts(_ b: CGRect, _ plan: BarPlan) -> [BarPart] {
        extensionItems.suffix(plan.extensions).reversed().map { item in
            let text = NSAttributedString(string: item.text, attributes: Theme.attrs(11, item.color?.nsColor ?? Theme.fg))
            guard let action = item.action else { return modulePart(b, [text], tip: "\(item.name): \(item.text)") }
            return modulePart(b, [text], item.name, value: item.text) { [weak self] in self?.onExtensionAction?(item.name, action) }
        }
    }

    private func showLayoutMenu(at p: CGPoint) {
        let menu = NSMenu()
        for m in LayoutMode.allCases {
            let item = NSMenuItem(title: m.title, action: #selector(pickLayout(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = m.rawValue
            item.state = m == layoutMode ? .on : .off
            item.image = Icons.layoutImage(m)
            menu.addItem(item)
        }
        menu.popUp(positioning: nil, at: CGPoint(x: p.x * Theme.scale, y: p.y * Theme.scale), in: self)
    }

    @objc private func pickLayout(_ item: NSMenuItem) {
        if let m = (item.representedObject as? String).flatMap(LayoutMode.init) { onPickLayout?(m) }
    }

    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .toolbar }
    override func accessibilityLabel() -> String? { String(localized: "Statusleiste", bundle: Bundle.app) }

    /// Knöpfe aus den Trefferflächen des letzten Zeichnens, dahinter das Display als Text.
    override func accessibilityChildren() -> [Any]? {
        a11y = hitRects.accessibilityElements(parent: self, reusing: a11y)
        statusA11y.update(parent: self, role: .staticText, label: String(localized: "Status", bundle: Bundle.app),
                          value: status.text, frame: status.rect.scaled(Theme.scale))
        return a11y + [statusA11y]
    }

    /// Die Aktion feuert sofort beim Drücken, die Taste sinkt dazu kurz ein.
    override func mouseDown(with event: NSEvent) {
        let v = convert(event.locationInWindow, from: nil)
        guard let hit = hitRects.first(at: CGPoint(x: v.x / Theme.scale, y: v.y / Theme.scale)) else { return }
        flash(hit.label)
        hit.action()
    }
}
