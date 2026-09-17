import AppKit

/// Rechte Seite: die ausgewählten Sessions als Grid oder Stack (i3-Akkordeon). Hält Auswahl, Fokus und
/// Layout-Modus, hängt Terminals in die sichtbaren Kacheln ein. Stack-Zeilen zeichnet sie selbst.
@MainActor
final class WorkspaceView: NSView {
    private(set) var groups: [Group] = []
    private(set) var sessions: [String: Session] = [:]
    /// Geordnete Session-Ids, die rechts zu sehen sind.
    private(set) var selected: [String] = []
    private(set) var focused: String? {
        didSet {
            guard oldValue != focused else { return }
            if let o = oldValue { lastFocused = o }
            revealFocus = true
            if let f = focused { onFocusChange?(f) }
        }
    }
    /// Für „zuletzt fokussierte Kachel“ (tmux M-Tab).
    private var lastFocused: String?
    private(set) var mode: LayoutMode = .grid
    /// Weitere Fenster speichern Auswahl, Layout und Auto-Modus unter eigenen Schlüsseln („.2“), Fenster 1 ohne.
    let defaultsSuffix: String
    /// Zoom: nur die Fokus-Kachel, bildschirmfüllend, die Auswahl bleibt.
    private(set) var zen = false
    /// ⌥J/⌥K: eine Session aus dem Baum vorübergehend allein zeigen. Auswahl und Fokus bleiben unangetastet.
    private(set) var preview: String?
    var attach: AttachManager?
    /// Auto-Modus: aus der Auswahl nur wartende bzw. arbeitende Sessions zeigen (Einstellungen).
    private(set) var auto = false
    /// Passt eine Session nicht mehr, bleibt ihre Kachel bis zu diesem Zeitpunkt stehen.
    private var linger: [String: CFTimeInterval] = [:]
    private var lastMatching: Set<String> = []
    private var lastTiles: [String] = []

    private var cells: [String: CellView] = [:]
    /// Ziehbare Grenzen der aktuellen Vorlage und die Felder der Kacheln, beides aus `relayout`.
    private var dividers: [SplitLine] = []
    private var tileFrames: [String: CGRect] = [:]
    private var draggedDivider: SplitLine?
    /// Scrollen: seitlicher Versatz der Spalten; nach Fokuswechsel oder neuer Breite rückt die Fokus-Kachel ins Bild.
    private var scrollX: CGFloat = 0
    private var revealFocus = true
    private var loaded = false
    private var stackRows: [(CGRect, String)] = []
    private var hoveredCell: String?
    private var hoveredRow: String?
    private var pulse: CGFloat = 1
    private var pulseTask: Task<Void, Never>?
    private weak var lastFirstResponder: NSResponder?
    /// Ziehen einer Kachel oder Stack-Zeile auf eine andere sortiert um.
    private var pressed: (point: CGPoint, key: String)?
    private var dragging = false
    private var dropTarget: String?

    var onChange: (() -> Void)?
    /// Eine neue Kachel bekommt den Fokus (Klick, Pfeiltasten, ⌘-Zahlen, …).
    var onFocusChange: ((String) -> Void)?
    /// Jeder Klick oder Fokus auf eine Kachel, auch wenn sie schon fokussiert war (Marke „neu“ quittieren).
    var onActivate: ((String) -> Void)?
    /// Zweiter Parameter: ⌥ gehalten, dann ohne Rückfrage. Nicht ⌘: das kollidiert mit Auswahl im Baum.
    var onCloseSession: ((String, Bool) -> Void)?
    var onRenameSession: ((String) -> Void)?
    /// Ziehen: (gezogen, Ziel). Die Reihenfolge selbst gehört dem Baum, siehe `sortSelected`.
    var onMoveSession: ((String, String) -> Void)?
    /// Baum ist leer: Klick auf den Hinweis startet eine neue Session, wie ⌘N.
    var onEmptyClick: (() -> Void)?
    /// Binary nicht gefunden oder `claude agents` schlägt fehl. AppDelegate hält es aktuell (`SessionRegistry.lastError`).
    var lastError: String?
    /// Ob schon ein Poll durchgelaufen ist, für den Lade-Zustand davor.
    var polled = false
    /// Interaktive Claude-Sessions, die woanders laufen (tmux, iTerm), nicht von diesem Profil verwaltet.
    /// AppDelegate hält es aktuell (`offerAdopt`), nur für den Hinweis im Leerzustand.
    var otherInteractiveCount = 0 { didSet { if otherInteractiveCount != oldValue { needsDisplay = true } } }
    /// Rechtsklick auf Kachel-Header oder Stack-Zeile: liefert das Kontextmenü der Session.
    var onContextMenu: ((String) -> NSMenu?)?
    /// Ein Terminal wurde hier ausgehängt: ein anderes Fenster, das es gerade nur als Hinweis zeigt, kann es einhängen.
    var onReleaseTerminal: (() -> Void)?
    private var released = false

    init(frame: NSRect, defaultsSuffix: String = "") {
        self.defaultsSuffix = defaultsSuffix
        super.init(frame: frame)
        wantsLayer = true
        clipsToBounds = true
        mode = LayoutMode(rawValue: Profile.defaults.string(forKey: "workspace.mode" + defaultsSuffix) ?? "") ?? .grid
        auto = Profile.defaults.bool(forKey: "workspace.auto" + defaultsSuffix)
        selected = Profile.defaults.stringArray(forKey: "workspace.selected" + defaultsSuffix) ?? []
        focused = selected.first
        pulseTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(80))
                guard let self else { return }
                self.tick()
            }
        }
    }
    required init?(coder: NSCoder) { nil }
    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { true }

    // MARK: Daten

    func reload(groups: [Group], sessions: [Session]) {
        self.groups = groups
        self.sessions = Dictionary(uniqueKeysWithValues: sessions.map { ($0.id, $0) })
        selected.removeAll { self.sessions[$0] == nil }
        if let p = preview, self.sessions[p] == nil { preview = nil }
        sortSelected()
        if let f = focused, !selected.contains(f), !(auto && autoPool.contains(f)) { focused = selected.first }
        // Entfernte Kacheln blenden kurz aus, neue ein. Beim allerersten Laden erscheint alles sofort.
        for (k, v) in cells where self.sessions[k] == nil {
            cells[k] = nil
            NSAnimationContext.runAnimationGroup({ $0.duration = 0.15; v.animator().alphaValue = 0 }, completionHandler: { v.removeFromSuperview() })
        }
        for id in selected {
            let v = cells[id] ?? CellView(session: self.sessions[id]!)
            v.session = self.sessions[id]!
            if v.superview == nil {
                addSubview(v)
                if loaded { v.alphaValue = 0; NSAnimationContext.runAnimationGroup { $0.duration = 0.18; v.animator().alphaValue = 1 } }
            }
            cells[id] = v
        }
        loaded = true
        relayout()
    }

    func session(_ key: String) -> Session? { sessions[key] }
    func group(forSession key: String) -> Group? { groups.first { $0.sessionIds.contains(key) } }

    /// Sichtbare Sessions starten ihren Claude-Prozess; einmal gestartet läuft er weiter, auch ausgeblendet.
    func shownSessions() -> [Session] { selected.compactMap { sessions[$0] } }

    // MARK: Auswahl

    /// Ersetzt die Auswahl (Klick) oder toggelt jede Id (⌘-Klick). `takeKeyboard: false`: Pfeiltasten im Baum.
    func select(_ ids: [String], add: Bool, takeKeyboard: Bool = true) {
        let ids = ids.filter { sessions[$0] != nil }
        preview = nil
        if add {
            for id in ids {
                if let i = selected.firstIndex(of: id) {
                    selected.remove(at: i)
                    if focused == id { focused = selected.last }
                } else {
                    selected.append(id)
                    focused = id
                }
            }
        } else {
            selected = ids
            focused = ids.first
        }
        zen = false
        sortSelected()
        let shown = tiles
        for (k, v) in cells where !selected.contains(k) && !shown.contains(k) { v.removeFromSuperview(); cells[k] = nil }
        for id in selected where cells[id] == nil {
            let v = CellView(session: sessions[id]!)
            addSubview(v)
            cells[id] = v
        }
        // Nur die angeklickten starten sofort, auch beendete; andere beendete bleiben stehen.
        for id in ids where selected.contains(id) { attach?.attachNow(sessions[id]!) }
        persist()
        relayout()
        if takeKeyboard { focusTerminal() }
    }

    /// ⇧-Bereich oder Gruppe mit Modifier: fehlende Sessions dazu, nichts weg.
    func addMissing(_ ids: [String]) { select(ids.filter { !selected.contains($0) }, add: true) }

    func setFocus(_ key: String, takeKeyboard: Bool = true) {
        guard selected.contains(key) || tiles.contains(key) else { return }
        focused = key
        onActivate?(key)
        relayout()
        if takeKeyboard { focusTerminal() }
    }

    /// ⌘Esc: Fokus-Kachel aus der Auswahl nehmen (im Zen erst zurück ins Layout).
    func removeFocused() {
        if zen { zen = false; relayout(); return }
        guard let f = focused else { return }
        select([f], add: true)
    }

    /// Nachbar der Fokus-Kachel: im Stack und Zoom nach Reihenfolge, sonst nach Lage der Felder.
    private func neighbor(_ d: Tiling.Direction) -> String? {
        let tiles = tiles
        guard let f = focused, let i = tiles.firstIndex(of: f) else { return nil }
        if mode == .stack || zen { return Tiling.neighbor(of: i, count: tiles.count, mode: .stack, d).map { tiles[$0] } }
        return Tiling.neighbor(of: i, frames: tiles.map { tileFrames[$0] ?? .zero }, d).map { tiles[$0] }
    }

    func moveFocus(_ d: Tiling.Direction) {
        if let n = neighbor(d) { setFocus(n) }
    }

    /// Fokus-Kachel mit ihrem Nachbarn in Richtung `d` tauschen, wie Ziehen: die Reihenfolge gehört dem Baum.
    func swapFocused(_ d: Tiling.Direction) {
        guard let f = focused, let n = neighbor(d) else { return }
        onMoveSession?(f, n)
    }

    /// ⌃⌥-Pfeile wie tmux `resize-pane`: die Trennlinie an der Fokus-Kachel wandert 5 % in Pfeilrichtung.
    /// Bevorzugt die Linie auf der Seite des Pfeils, sonst die gegenüberliegende.
    func resizeFocused(_ d: Tiling.Direction) {
        guard let f = focused, let frame = tileFrames[f], !zen, preview == nil else { NSSound.beep(); return }
        if mode == .scroll {
            // Scrollen: ← / → schaltet die Breite der Fokus-Spalte eine Stufe (⅓ ½ ⅔) schmaler bzw. breiter.
            guard d == .left || d == .right, let i = tiles.firstIndex(of: f) else { NSSound.beep(); return }
            var v = Settings.layoutRatios("scroll.widths", 0) ?? []
            while v.count <= i { v.append(0.5) }
            let w = Tiling.scrollWidths, cur = w.indices.min { abs(w[$0] - v[i]) < abs(w[$1] - v[i]) }!, next = cur + (d == .right ? 1 : -1)
            guard w.indices.contains(next) else { NSSound.beep(); return }
            v[i] = w[next]
            Settings.setLayoutRatios("scroll.widths", v)
            revealFocus = true
            relayout()
            return
        }
        let vertical = d == .left || d == .right, forward = d == .right || d == .down
        let near = dividers.filter { div in
            guard div.vertical == vertical else { return false }
            return vertical ? div.rect.minY < frame.maxY && div.rect.maxY > frame.minY : div.rect.minX < frame.maxX && div.rect.maxX > frame.minX
        }
        let mid = { (div: SplitLine) in vertical ? div.rect.midX : div.rect.midY }
        let far = vertical ? (forward ? frame.maxX : frame.minX) : (forward ? frame.maxY : frame.minY)
        let other = vertical ? (forward ? frame.minX : frame.maxX) : (forward ? frame.minY : frame.maxY)
        let reach = CGFloat(Settings.tileGap) + 10
        guard let div = near.first(where: { abs(mid($0) - far) <= reach }) ?? near.first(where: { abs(mid($0) - other) <= reach }) else { NSSound.beep(); return }
        let step = (vertical ? div.span.width : div.span.height) * 0.05 * (forward ? 1 : -1)
        let p = vertical ? CGPoint(x: mid(div) + step, y: 0) : CGPoint(x: 0, y: mid(div) + step)
        moveDivider(div, to: p)
    }

    private func moveDivider(_ d: SplitLine, to p: CGPoint) {
        Settings.setLayoutRatios(d.key, Tiling.drag(d, to: p, gap: CGFloat(Settings.tileGap), ratios: Settings.layoutRatios(d.key, d.count) ?? []))
        relayout()
    }

    /// Frei (i3 `split h/v`): wo die Kachel nach der fokussierten entsteht, „r“ rechts, „d“ unten, „a“ längere Seite.
    /// Schaltet auf Frei um. An der letzten Kachel gilt es für die nächste, sonst ordnet es sofort neu.
    func setSplit(_ c: Character) {
        guard let f = focused, let i = tiles.firstIndex(of: f), !zen, preview == nil else { NSSound.beep(); return }
        var s = Array(Settings.customSplits)
        while s.count <= i { s.append("a") }
        s[i] = c
        Settings.customSplits = String(s)
        if mode != .custom { setMode(.custom) } else { relayout() }
    }

    /// Teilungsrichtung an der Fokus-Kachel für die Leiste.
    var focusedSplit: Character {
        let s = Array(Settings.customSplits)
        guard let f = focused, let i = tiles.firstIndex(of: f), s.indices.contains(i) else { return "a" }
        return s[i]
    }

    /// Mausrad oder Trackpad seitlich im Layout Scrollen.
    func scrollBy(_ dx: CGFloat) {
        guard mode == .scroll, !zen, preview == nil else { return }
        scrollX += dx
        relayout()
    }

    func setGridColumns(_ c: Int) {
        Settings.gridColumns = max(0, c)
        relayout()
    }

    func resetRatios() {
        Settings.resetLayoutRatios()
        relayout()
    }

    /// Nächste (+1) oder vorige (-1) Kachel, am Ende wieder vorn.
    func cycleFocus(_ step: Int) {
        let tiles = tiles
        guard let f = focused, let i = tiles.firstIndex(of: f) else { return }
        let n = tiles.count
        setFocus(tiles[((i + step) % n + n) % n])
    }

    /// Vorschau ohne Tastatur und ohne Prozessstart: eine beendete Session zeigt ihre letzten Zeilen.
    func setPreview(_ key: String?) {
        let old = preview
        preview = key.flatMap { sessions[$0] == nil ? nil : $0 }
        if let o = old, o != preview, !selected.contains(o), !tiles.contains(o) { cells[o]?.removeFromSuperview(); cells[o] = nil }
        if let p = preview, cells[p] == nil {
            let v = CellView(session: sessions[p]!)
            addSubview(v)
            cells[p] = v
        }
        relayout()
    }

    func focusLast() { if let l = lastFocused { setFocus(l) } }

    func focusTile(_ i: Int) { let t = tiles; if t.indices.contains(i) { setFocus(t[i]) } }

    func toggleAuto() {
        auto.toggle()
        Profile.defaults.set(auto, forKey: "workspace.auto" + defaultsSuffix)
        linger = [:]
        lastMatching = []
        zen = false
        relayout()
        focusTerminal()
    }

    /// Sync (tmux synchronize-panes): Eingaben im Fokus-Terminal gehen an alle Kacheln. Bewusst nicht gespeichert.
    private(set) var sync = false

    func toggleSync() {
        sync.toggle()
        relayout()
        focusTerminal()
    }

    /// Terminals der übrigen Kacheln, an die im Sync-Modus Eingaben mitgehen.
    func syncTargets(except source: KadrellTerminalView) -> [KadrellTerminalView] {
        guard sync else { return [] }
        return tiles.compactMap { attach?.terminal(for: $0) }.filter { $0 !== source }
    }

    /// Kacheln rechts: die Auswahl, im Auto-Modus gefiltert samt Nachlauf.
    private var tiles: [String] { auto ? autoPool.filter { matchesAuto($0) || linger[$0] != nil } : selected }

    /// Woraus der Auto-Modus filtert: je nach Einstellung alle Sessions in Baumreihenfolge oder nur die Auswahl.
    private var autoPool: [String] { Settings.autoAllSessions ? groups.flatMap(\.sessionIds).filter { sessions[$0] != nil } : selected }

    private func matchesAuto(_ key: String) -> Bool {
        switch sessions[key]?.status {
        case .waiting: Settings.autoWaiting
        case .running: Settings.autoRunning
        default: false
        }
    }

    /// Wer eben noch passte und jetzt nicht mehr, läuft 3 s nach.
    private func updateLinger() {
        guard auto else { return }
        let now = CACurrentMediaTime()
        let pool = autoPool
        let matching = Set(pool.filter(matchesAuto))
        for id in lastMatching.subtracting(matching) where linger[id] == nil { linger[id] = now + 3 }
        linger = linger.filter { $0.value > now && !matching.contains($0.key) && pool.contains($0.key) }
        lastMatching = matching
    }

    func setMode(_ m: LayoutMode) {
        mode = m
        revealFocus = true
        Profile.defaults.set(m.rawValue, forKey: "workspace.mode" + defaultsSuffix)
        relayout()
        focusTerminal()
    }

    func toggleZen() {
        guard focused != nil else { return }
        zen.toggle()
        relayout()
        focusTerminal()
    }

    /// Arbeitsfläche zeigt die Sessions in Baumreihenfolge: Gruppen, darin ihre Sessions.
    private func sortSelected() {
        let order = Dictionary(groups.flatMap(\.sessionIds).enumerated().map { ($1, $0) }, uniquingKeysWith: { a, _ in a })
        selected = selected.enumerated().sorted { (order[$0.1] ?? .max, $0.0) < (order[$1.1] ?? .max, $1.0) }.map(\.1)
    }

    private func persist() { Profile.defaults.set(selected, forKey: "workspace.selected" + defaultsSuffix) }

    /// Die Fokus-Kachel bekommt die Tastatur. Hängt ihr Terminal in einem anderen Fenster, holt sie es hierher:
    /// eine NSView kann nur an einer Stelle hängen, dort bleibt der Hinweis stehen.
    private func focusTerminal() {
        guard let f = focused, let t = attach?.terminal(for: f) else { window?.makeFirstResponder(self); return }
        if let other = host(of: t), other !== self {
            other.unmountTerminal(for: f)
            relayout()
            other.relayout()
        }
        guard t.superview != nil else { window?.makeFirstResponder(self); return }
        window?.makeFirstResponder(t)
    }

    /// Fenster geht zu: Terminals freigeben und den Takt anhalten.
    func close() {
        pulseTask?.cancel()
        for key in Array(cells.keys) { unmountTerminal(for: key) }
        onReleaseTerminal?()
    }

    // MARK: Layout

    func relayout() {
        let gap = CGFloat(Settings.tileGap)
        let inset = bounds.insetBy(dx: gap, dy: gap)
        var frames: [String: CGRect] = [:]
        stackRows = []
        dividers = []
        updateLinger()
        let tiles = tiles
        // Auto-Modus: ist die Fokus-Kachel weg, bekommt eine neu aufgetauchte (sonst die erste) den Fokus.
        var refocus = false
        if auto, preview == nil, let first = tiles.first, !(focused.map(tiles.contains) ?? false) {
            focused = tiles.first { !lastTiles.contains($0) } ?? first
            refocus = true
        }
        lastTiles = tiles
        // Auto-Modus über alle Sessions: auch nicht ausgewählte bekommen eine Kachel, sobald sie passen.
        for id in tiles where cells[id] == nil {
            guard let s = sessions[id] else { continue }
            let v = CellView(session: s)
            addSubview(v)
            cells[id] = v
            if loaded { v.alphaValue = 0; NSAnimationContext.runAnimationGroup { $0.duration = 0.18; v.animator().alphaValue = 1 } }
        }
        let visible: [String]
        if let p = preview {
            visible = [p]
            frames[p] = inset
        } else if zen, let f = focused, tiles.contains(f) {
            visible = [f]
            frames[f] = inset
        } else if mode != .stack {
            visible = tiles
            let t = Tiling.layout(mode, count: tiles.count, in: inset, gap: gap, columns: Settings.gridColumns, splits: Settings.customSplits, ratios: Settings.layoutRatios)
            var laid = t.frames
            if mode == .scroll {
                let reveal = revealFocus ? focused.flatMap { tiles.firstIndex(of: $0) }.map { laid[$0] } : nil
                scrollX = Tiling.scrollOffset(scrollX, reveal: reveal, contentMaxX: laid.last?.maxX ?? 0, in: inset)
                laid = laid.map { $0.offsetBy(dx: -scrollX, dy: 0) }
            }
            for (id, r) in zip(tiles, laid) { frames[id] = r }
            dividers = t.dividers
        } else {
            let active = focused.flatMap { tiles.firstIndex(of: $0) } ?? 0
            let (rows, body) = Tiling.stack(count: tiles.count, active: active, in: inset, rowHeight: (Tiling.rowHeight * Theme.scale).rounded())
            stackRows = Array(zip(rows, tiles))
            visible = tiles.isEmpty ? [] : [tiles[active]]
            if !tiles.isEmpty { frames[tiles[active]] = body }
        }
        revealFocus = false
        tileFrames = frames
        // Scrollen: Kacheln ganz außerhalb verstecken und ihre Terminals aushängen, Nachbarn per Pfeil finden sie trotzdem.
        if mode == .scroll { frames = frames.filter { $0.value.intersects(bounds) } }
        for (key, v) in cells {
            guard let f = frames[key], let s = sessions[key] else {
                v.isHidden = true
                unmountTerminal(for: key)
                continue
            }
            v.isHidden = false
            v.headerHidden = mode == .stack && !zen && preview == nil
            if v.frame != f { v.frame = f }
            let g = group(forSession: key)
            v.groupName = g?.name ?? ""
            v.groupColor = Theme.group(g?.color ?? "#6c7086")
            // Sync: alle Kacheln bekommen Eingaben, also sehen auch alle ausgewählt aus.
            v.focused = (focused == key || sync) && visible.count > 1
            v.zoomed = zen && tiles.count > 1
            v.dropTarget = dragging && dropTarget == key
            v.hovered = hoveredCell == key
            v.attached = attach?.isAttached(key) ?? false
            v.ended = attach?.isEnded(key) ?? false
            v.previewing = preview == key
            v.keyboardFocus = attach?.terminal(for: key).map { $0 === window?.firstResponder } ?? false
            v.lines = attach?.lines(for: key) ?? []
            v.pulse = pulse
            mountTerminal(for: key, in: v, session: s)
            v.elsewhere = attach?.terminal(for: key).flatMap(host).map { $0 !== self } ?? false
            v.needsDisplay = true
        }
        // Terminals nicht sichtbarer Sessions dürfen nirgends hängen.
        for (key, t) in attach?.terminals ?? [:] where frames[key] == nil && t.superview != nil { unmountTerminal(for: key) }
        if let w = window, w.firstResponder === w { w.makeFirstResponder(self) }
        needsDisplay = true
        if refocus { focusTerminal() }
        if released { released = false; onReleaseTerminal?() }
        onChange?()
    }

    private func mountTerminal(for key: String, in cell: CellView, session: Session) {
        // Hängt schon in einem anderen Fenster: dort lassen, die Kachel zeigt den Hinweis (siehe `focusTerminal`).
        guard let t = attach?.terminal(for: key), host(of: t) == nil || host(of: t) === self else { return }
        if t.superview !== cell { cell.addSubview(t) }
        let bg = cell.bodyColor
        if t.nativeBackgroundColor != bg { t.nativeBackgroundColor = bg }
        let body = cell.terminalRect
        if t.frame != body { t.frame = body }
        // Erst im Fenster umschalten, so will es SwiftTerm. Scheitert Metal, bleibt CoreGraphics.
        if t.window != nil, t.isUsingMetalRenderer != Settings.terminalMetal { try? t.setUseMetal(Settings.terminalMetal) }
    }

    /// Arbeitsfläche, in der das Terminal hängt. nil: frei, auch wenn es noch in einer schon entfernten Kachel steckt.
    private func host(of t: NSView) -> WorkspaceView? { t.superview?.superview as? WorkspaceView }

    private func unmountTerminal(for key: String) {
        guard let t = attach?.terminal(for: key), t.superview != nil, host(of: t) == nil || host(of: t) === self else { return }
        if window?.firstResponder === t { window?.makeFirstResponder(self) }
        t.removeFromSuperview()
        released = true
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        relayout()
    }

    private func tick() {
        // Klick in ein Terminal macht es still zum First Responder: Fokus und Rahmen nachziehen.
        if window?.firstResponder !== lastFirstResponder {
            lastFirstResponder = window?.firstResponder
            if let hit = cells.first(where: { attach?.terminal(for: $0.key) === window?.firstResponder }), focused != hit.key {
                focused = hit.key
                onActivate?(hit.key)
                relayout()
            } else {
                // Klick ins schon fokussierte Terminal zählt auch als Hinsehen.
                if let hit = cells.first(where: { attach?.terminal(for: $0.key) === window?.firstResponder }) { onActivate?(hit.key) }
                for (key, v) in cells {
                    let has = attach?.terminal(for: key).map { $0 === window?.firstResponder } ?? false
                    if v.keyboardFocus != has { v.keyboardFocus = has; v.needsDisplay = true }
                }
            }
        }
        let t = CACurrentMediaTime().truncatingRemainder(dividingBy: 1.2) / 1.2
        pulse = Feedback.reduceMotion ? 1 : 0.3 + 0.7 * (0.5 + 0.5 * cos(2 * .pi * t))
        for (key, v) in cells where sessions[key]?.status == .running && !v.isHidden {
            v.pulse = pulse
            if !v.headerHidden { v.setNeedsDisplay(v.dotRect) }
        }
        if !stackRows.isEmpty { needsDisplay = true }
        let now = CACurrentMediaTime()
        if linger.values.contains(where: { $0 <= now }) { relayout() }
    }

    // MARK: Zeichnen

    /// Warum die Fläche leer ist und was als Nächstes zu tun ist.
    private enum EmptyReason { case error(String), loading, noSessions, hint }

    private var emptyReason: EmptyReason {
        if let lastError { return .error(lastError) }
        if !polled { return .loading }
        if sessions.isEmpty { return .noSessions }
        return .hint
    }

    /// Hintergrundbild, einmal auf die Größe der Fläche gerechnet: Stack-Zeilen zeichnen oft neu, das Bild soll dabei nur kopiert werden.
    private var background: (path: String, size: CGSize, image: CGImage?)?

    private func drawBackground() {
        let path = Settings.backgroundImage, px = convertToBacking(bounds).size
        guard !path.isEmpty, px.width >= 1, px.height >= 1 else { return }
        if background?.path != path || background?.size != px {
            var scaled: CGImage?
            if let src = NSImage(contentsOfFile: path)?.cgImage(forProposedRect: nil, context: nil, hints: nil),
               let ctx = CGContext(data: nil, width: Int(px.width), height: Int(px.height), bitsPerComponent: 8, bytesPerRow: 0,
                                   space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue) {
                // Füllend wie „aspect fill“: die kürzere Seite passt, der Rest wird mittig abgeschnitten.
                let s = max(px.width / CGFloat(src.width), px.height / CGFloat(src.height))
                let w = CGFloat(src.width) * s, h = CGFloat(src.height) * s
                ctx.interpolationQuality = .high
                ctx.draw(src, in: CGRect(x: (px.width - w) / 2, y: (px.height - h) / 2, width: w, height: h))
                scaled = ctx.makeImage()
            }
            background = (path, px, scaled)
        }
        guard let img = background?.image, let ctx = NSGraphicsContext.current?.cgContext else { return }
        ctx.saveGState()
        ctx.translateBy(x: 0, y: bounds.height)
        ctx.scaleBy(x: 1, y: -1)
        ctx.draw(img, in: bounds)
        ctx.restoreGState()
    }

    override func draw(_ dirtyRect: NSRect) {
        Theme.bg.setFill()
        dirtyRect.fill()
        drawBackground()
        if tiles.isEmpty {
            let a: NSAttributedString, b: NSAttributedString
            var c: NSAttributedString?
            switch emptyReason {
            case .error(let message):
                a = NSAttributedString(string: String(localized: "Sessions können nicht geladen werden"), attributes: Theme.attrs(12, Theme.error))
                b = NSAttributedString(string: message, attributes: Theme.attrs(11, Theme.error.withAlphaComponent(0.7)))
            case .loading:
                a = NSAttributedString(string: String(localized: "Lade Sessions …"), attributes: Theme.attrs(12, Theme.muted))
                b = NSAttributedString(string: "", attributes: Theme.attrs(11, Theme.muted))
            case .noSessions:
                a = NSAttributedString(string: String(localized: "Noch keine Session"), attributes: Theme.attrs(12, Theme.muted))
                b = NSAttributedString(string: String(localized: "⌘N startet eine"), attributes: Theme.attrs(11, Theme.muted.withAlphaComponent(0.7)))
                if otherInteractiveCount > 0 {
                    let text = otherInteractiveCount == 1 ? String(localized: "1 Claude-Session läuft interaktiv in anderen Terminals · Import: tools/tmux-dump.py")
                        : String(localized: "\(otherInteractiveCount) Claude-Sessions laufen interaktiv in anderen Terminals · Import: tools/tmux-dump.py")
                    c = NSAttributedString(string: text,
                                           attributes: Theme.attrs(11, Theme.muted.withAlphaComponent(0.55)))
                }
            case .hint:
                let idle = auto && !autoPool.isEmpty
                a = NSAttributedString(string: idle ? String(localized: "Gerade wartet keine Session") : String(localized: "Session im Baum wählen"), attributes: Theme.attrs(12, Theme.muted))
                b = NSAttributedString(string: idle ? String(localized: "Auto-Modus: Kacheln erscheinen, sobald Claude etwas von dir will") : String(localized: "⌘-Klick für mehrere · ⇧-Klick Bereich · Gruppe = alle · F1 Hilfe"), attributes: Theme.attrs(11, Theme.muted.withAlphaComponent(0.7)))
            }
            Theme.scaled(bounds) { r in
                a.draw(at: CGPoint(x: r.midX - a.size().width / 2, y: r.midY - 16))
                b.draw(at: CGPoint(x: r.midX - b.size().width / 2, y: r.midY + 4))
                c?.draw(at: CGPoint(x: r.midX - c!.size().width / 2, y: r.midY + 24))
            }
            return
        }
        guard !stackRows.isEmpty, !zen, preview == nil else { return }
        for (r, key) in stackRows { Theme.scaled(r) { drawStackRow($0, key: key) } }
        if dragging, let t = dropTarget, let (r, _) = stackRows.first(where: { $0.1 == t }) {
            Theme.fg.setFill()
            r.frame(withWidth: 2)
        }
    }

    private func drawStackRow(_ r: CGRect, key: String) {
        guard let s = sessions[key] else { return }
        let g = group(forSession: key)
        let color = Theme.group(g?.color ?? "#6c7086")
        let on = focused == key || sync, hover = hoveredRow == key
        color.mixed(on ? 0.22 : 0.1, into: on ? Theme.surface : Theme.panel).setFill()
        r.fill()
        Theme.line.setFill()
        CGRect(x: r.minX, y: r.maxY - 1, width: r.width, height: 1).fill()
        (on ? color : color.mixed(0.45, into: Theme.bg)).setFill()
        CGRect(x: r.minX, y: r.minY, width: on ? 3 : 1, height: r.height).fill()
        let attached = attach?.isAttached(key) ?? false
        let c = attached ? Theme.color(for: s.status) : Theme.detached
        let dotColor = s.status == .running && attached ? c.withAlphaComponent(pulse) : c
        Icons.statusDot(in: CGRect(x: r.minX + 12, y: r.midY - 4, width: 8, height: 8), status: s.status, attached: attached, color: dotColor)
        let age = NSAttributedString(string: s.elapsed(), attributes: Theme.attrs(10.5, Theme.muted))
        let grp = NSAttributedString(string: g?.name ?? "", attributes: Theme.attrs(10.5, color))
        var rx = r.maxX - 10
        if hover {
            Icons.x(in: CGRect(x: rx - 16, y: r.midY - 8, width: 16, height: 16), color: Theme.sub)
            Icons.pen(in: CGRect(x: rx - 36, y: r.midY - 8, width: 16, height: 16), color: Theme.sub)
            rx -= 44
        }
        rx -= age.size().width; age.draw(at: CGPoint(x: rx, y: r.midY - 7))
        rx -= 8 + grp.size().width; grp.draw(at: CGPoint(x: rx, y: r.midY - 7))
        if Settings.stackShowPath {
            let pa = Theme.attrs(10.5, Theme.muted)
            let maxW = max(0, (rx - 16 - r.minX - 28) / 2)
            let path = NSAttributedString(string: Theme.fitPath(s.cwd, width: maxW, attrs: pa), attributes: pa)
            let w = min(path.size().width, maxW)
            rx -= 16 + w
            path.draw(with: CGRect(x: rx, y: r.midY - 7, width: w, height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
        }
        let title = NSAttributedString(string: s.title, attributes: Theme.attrs(11.5, on || hover ? Theme.fg : Theme.sub, bold: on))
        title.draw(with: CGRect(x: r.minX + 28, y: r.midY - 8, width: max(0, rx - 10 - r.minX - 28), height: 16), options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }

    /// Klick auf Kachel oder Stack-Zeile: Fokus, eine beendete Session setzt fort.
    func activate(_ k: String) {
        if attach?.isAttached(k) == false, let s = sessions[k] { attach?.attachNow(s) }
        setFocus(k)
    }

    // MARK: Accessibility

    private var a11y: [A11yElement] = []
    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .group }
    override func accessibilityLabel() -> String? { String(localized: "Arbeitsfläche") }

    /// Kacheln (eigene Views) plus die gezeichneten Stack-Zeilen.
    override func accessibilityChildren() -> [Any]? {
        a11y = (stackRows.isEmpty || zen || preview != nil ? [] : stackRows).compactMap { r, key in
            guard let s = sessions[key] else { return nil }
            let label = [s.title, (attach?.isAttached(key) ?? false) ? s.status.spoken : String(localized: "nicht gestartet"), group(forSession: key)?.name]
            return a11y.reuse(key).update(parent: self, role: .button, label: label.compactMap { $0 }.joined(separator: ", "), frame: r,
                                          press: { [weak self] in self?.activate(key) },
                                          actions: [a11yAction(String(localized: "Umbenennen")) { [weak self] in self?.onRenameSession?(key) },
                                                    a11yAction(String(localized: "Schließen")) { [weak self] in self?.onCloseSession?(key, false) }])
        }
        return (super.accessibilityChildren() ?? []) + a11y
    }

    // MARK: Events

    private enum Hit { case cell(String), cellClose(String), cellRename(String), row(String), rowClose(String), rowRename(String), none }

    private func hit(at p: CGPoint) -> Hit {
        for (r, key) in stackRows where r.contains(p) {
            let fromRight = (r.maxX - p.x) / Theme.scale
            return fromRight < 30 ? .rowClose(key) : fromRight < 50 ? .rowRename(key) : .row(key)
        }
        for (key, v) in cells where !v.isHidden && v.frame.contains(p) {
            let local = CGPoint(x: p.x - v.frame.minX, y: p.y - v.frame.minY)
            if !v.headerHidden, v.xRect.insetBy(dx: -4, dy: -4).contains(local) { return .cellClose(key) }
            if !v.headerHidden, v.penRect.insetBy(dx: -2, dy: -4).contains(local) { return .cellRename(key) }
            return .cell(key)
        }
        return .none
    }

    private func divider(at p: CGPoint) -> SplitLine? { dividers.first { $0.rect.contains(p) } }

    /// Über einer Trennlinie gehört die Maus der Arbeitsfläche, nicht dem Terminal darunter (Abstand 0).
    override func hitTest(_ point: NSPoint) -> NSView? {
        if !isHidden, divider(at: convert(point, from: superview)) != nil { return self }
        return super.hitTest(point)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for t in trackingAreas { removeTrackingArea(t) }
        addTrackingArea(NSTrackingArea(rect: bounds, options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect], owner: self))
    }

    override func mouseMoved(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        guard p.x > ThinSplitView.grabWidth / 2 else { return }   // Griffzone des Trenners: Cursor gehört dem Split
        if let d = divider(at: p) { (d.vertical ? NSCursor.resizeLeftRight : NSCursor.resizeUpDown).set(); return }
        var cell: String?, row: String?
        switch hit(at: p) {
        case .cell(let k): cell = k; NSCursor.arrow.set()
        case .cellClose(let k), .cellRename(let k): cell = k; NSCursor.pointingHand.set()
        case .row(let k): row = k; NSCursor.pointingHand.set()
        case .rowClose(let k), .rowRename(let k): row = k; NSCursor.pointingHand.set()
        case .none: NSCursor.arrow.set()
        }
        guard cell != hoveredCell || row != hoveredRow else { return }
        let old = hoveredCell
        hoveredCell = cell; hoveredRow = row
        for k in [old, cell].compactMap({ $0 }) { cells[k]?.hovered = hoveredCell == k; cells[k]?.needsDisplay = true }
        needsDisplay = true
    }

    override func mouseExited(with event: NSEvent) {
        if let c = hoveredCell { cells[c]?.hovered = false; cells[c]?.needsDisplay = true }
        hoveredCell = nil; hoveredRow = nil
        needsDisplay = true
    }

    /// Rechtsklick (bzw. Ctrl-Klick): Kontextmenü der Session unter Kachel-Header oder Stack-Zeile.
    override func menu(for event: NSEvent) -> NSMenu? {
        switch hit(at: convert(event.locationInWindow, from: nil)) {
        case .cell(let k), .cellClose(let k), .cellRename(let k), .row(let k), .rowClose(let k), .rowRename(let k):
            return onContextMenu?(k)
        case .none: return nil
        }
    }

    override func mouseDown(with event: NSEvent) {
        let p = convert(event.locationInWindow, from: nil)
        let force = event.modifierFlags.contains(.option)
        pressed = nil
        // Trennlinie: ziehen ändert die Vorlage, Doppelklick verteilt diese Liste wieder gleich.
        if let d = divider(at: p) {
            if event.clickCount == 2 { Settings.setLayoutRatios(d.key, nil); relayout() } else { draggedDivider = d }
            return
        }
        switch hit(at: p) {
        case .cellClose(let k), .rowClose(let k): onCloseSession?(k, force)
        case .cellRename(let k), .rowRename(let k): onRenameSession?(k)
        case .cell(let k), .row(let k):
            pressed = (p, k)
            activate(k)
        case .none:
            if case .noSessions = emptyReason { onEmptyClick?() }
            window?.makeFirstResponder(self)
        }
    }

    override func mouseDragged(with event: NSEvent) {
        if let d = draggedDivider { moveDivider(d, to: convert(event.locationInWindow, from: nil)); return }
        guard let press = pressed, tiles.count > 1, !zen else { return }
        let p = convert(event.locationInWindow, from: nil)
        if !dragging, hypot(p.x - press.point.x, p.y - press.point.y) > 4 { dragging = true }
        guard dragging else { return }
        NSCursor.closedHand.set()
        var t: String?
        switch hit(at: p) {
        case .cell(let k), .cellClose(let k), .cellRename(let k), .row(let k), .rowClose(let k), .rowRename(let k): t = k == press.key ? nil : k
        case .none: t = nil
        }
        guard t != dropTarget else { return }
        if t != nil { Feedback.snap() }
        dropTarget = t
        relayout()
    }

    override func mouseUp(with event: NSEvent) {
        if draggedDivider != nil { draggedDivider = nil; return }
        let src = pressed?.key, t = dropTarget, wasDragging = dragging
        pressed = nil; dragging = false; dropTarget = nil
        guard wasDragging else { return }
        NSCursor.arrow.set()
        relayout()
        if let src, let t { onMoveSession?(src, t) }
    }
}
