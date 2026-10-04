import AppKit

/// Ein Hauptfenster: Baum, Arbeitsfläche und Leiste mit eigener Auswahl, eigenem Layout und Fokus. Sessions und Gruppen
/// hält der AppDelegate, Marke „neu“, Hooks und Sounds der `AttentionTracker`, beide für alle Fenster gemeinsam. Fenster 0 speichert unter den
/// bisherigen Schlüsseln, weitere mit Suffix („workspace.selected.2“).
@MainActor
final class MainWindowController: NSObject {
    let index: Int
    let window: NSWindow
    let bar = StatusBarView(frame: .zero)
    let wallpaper = WallpaperView(frame: .zero)
    let split = ThinSplitView(frame: .zero)
    let sidebarScroll = NSScrollView(frame: .zero)
    let sidebar = SidebarView(frame: .zero)
    let workspace: WorkspaceView
    /// Rechte Sidebar mit den Panels der Extensions, dritter Bereich im Split.
    let rightScroll = NSScrollView(frame: .zero)
    let extensionPanel = ExtensionPanelView(frame: .zero)
    /// Breite, auf die der Baum beim nächsten Einblenden zurückkommt.
    private var lastSidebarWidth: CGFloat = 260
    /// Vom Nutzer ausgeblendet (⌘⌥B), gespeichert pro Fenster. Ohne Panels fehlt der Bereich unabhängig davon.
    var isRightSidebarHidden: Bool {
        didSet {
            Profile.defaults.set(!isRightSidebarHidden, forKey: "rightSidebar.visible" + Self.suffix(index))
            applyRightSidebar()
        }
    }

    /// Ausgeblendet heißt: der Baum ist nicht mehr Teil des Splits (kein Trenner übrig).
    var isSidebarHidden: Bool { sidebarScroll.superview == nil }

    static func suffix(_ index: Int) -> String { index == 0 ? "" : ".\(index + 1)" }

    /// `cascade`: Fenster, gegen das ein neues ohne gespeicherte Lage versetzt aufgeht.
    init(index: Int, cascade: NSWindow?) {
        self.index = index
        workspace = WorkspaceView(frame: .zero, defaultsSuffix: Self.suffix(index))
        isRightSidebarHidden = !(Profile.defaults.object(forKey: "rightSidebar.visible" + Self.suffix(index)) as? Bool ?? true)
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1400, height: 900),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        super.init()
        window.isReleasedWhenClosed = false
        window.title = "Kadrell" + Profile.label + (index == 0 ? "" : String(localized: " · Fenster \(index + 1)", bundle: Bundle.app))
        window.appearance = Theme.appearance
        window.backgroundColor = Theme.bg
        window.minSize = NSSize(width: 800, height: 500)
        let frameName = "KadrellMain" + Self.suffix(index)
        if index == 0 { window.setFrameAutosaveName(frameName) }
        let root = FlippedView(frame: window.contentRect(forFrameRect: window.frame))
        root.autoresizingMask = [.width, .height]
        bar.frame = NSRect(x: 0, y: 0, width: root.bounds.width, height: Theme.barHeight)
        bar.autoresizingMask = [.width, .maxYMargin]

        sidebarScroll.documentView = sidebar
        sidebarScroll.hasVerticalScroller = true
        sidebarScroll.autohidesScrollers = true
        sidebarScroll.drawsBackground = false   // der Baum malt sein Panel selbst, mit der Deckkraft der Kacheln
        sidebar.autoresizingMask = [.width]
        sidebar.frame = NSRect(x: 0, y: 0, width: 260, height: 10)
        rightScroll.documentView = extensionPanel
        rightScroll.hasVerticalScroller = true
        rightScroll.autohidesScrollers = true
        rightScroll.drawsBackground = false
        extensionPanel.frame = NSRect(x: 0, y: 0, width: 260, height: 10)
        extensionPanel.onLeave = { [weak self] in self?.leaveRightSidebar() }
        split.isVertical = true
        split.dividerStyle = .thin
        split.addArrangedSubview(sidebarScroll)
        split.addArrangedSubview(workspace)
        split.setHoldingPriority(.defaultLow + 1, forSubviewAt: 0)
        split.autosaveName = "KadrellSplit" + Self.suffix(index)
        split.delegate = self
        split.frame = NSRect(x: 0, y: Theme.barHeight, width: root.bounds.width, height: root.bounds.height - Theme.barHeight)
        split.autoresizingMask = [.width, .height]
        wallpaper.frame = split.frame
        wallpaper.autoresizingMask = [.width, .height]
        root.addSubview(wallpaper)
        root.addSubview(split)
        root.addSubview(bar)
        window.contentView = root
        if index == 0 {
            window.center()
        } else {
            if !window.setFrameUsingName(frameName) {
                window.center()
                if let cascade { window.setFrameTopLeftPoint(cascade.cascadeTopLeft(from: .zero)) }
            }
            window.setFrameAutosaveName(frameName)
        }
    }

    func show() {
        window.makeKeyAndOrderFront(nil)
        restoreSidebarWidth()
        window.makeFirstResponder(workspace)
    }

    private var sidebarWidthKey: String { "sidebar.width" + Self.suffix(index) }
    /// Erst nach `restoreSidebarWidth` gilt die Baumbreite als gewollt und wird gespeichert, nicht schon die Startaufteilung.
    private var sidebarWidthRestored = false

    /// Baumbreite vom letzten Mal. Die Autosave-Daten des Splits taugen dafür nicht mehr: mit rechter Sidebar sichern sie
    /// drei Bereiche, beim Start sind es zwei (Panels kommen erst später), dann verwirft der Split alles.
    func restoreSidebarWidth() {
        if let w = Profile.defaults.object(forKey: sidebarWidthKey) as? Double { split.setPosition(CGFloat(w), ofDividerAt: 0) }
        else if split.arrangedSubviews[0].frame.width < 120 { split.setPosition(260, ofDividerAt: 0) }
        sidebarWidthRestored = true
        // Einmal sichern: kamen die Panels schon vorher (⌘⇧T), gab es danach womöglich kein Verschieben mehr, das speichert.
        if !isSidebarHidden { Profile.defaults.set(Double(sidebarScroll.frame.width), forKey: sidebarWidthKey) }
    }

    /// Baum ein/aus (Cmd+B): ausgeblendet nehmen wir ihn ganz aus dem Split, dann bleibt kein Trenner übrig –
    /// weder als Linie noch zum Herausziehen. Zurück kommt er nur hierüber, auf der zuletzt genutzten Breite.
    func toggleSidebar() {
        if isSidebarHidden {
            split.insertArrangedSubview(sidebarScroll, at: 0)
            split.setHoldingPriority(.defaultLow + 1, forSubviewAt: 0)
            split.setPosition(lastSidebarWidth, ofDividerAt: 0)
        } else {
            lastSidebarWidth = max(sidebarScroll.frame.width, 120)
            sidebarScroll.removeFromSuperview()
        }
        window.invalidateCursorRects(for: split)
    }

    /// Darstellung aus den Einstellungen: Farben nur bei neuem Theme, Leiste und Split folgen der UI-Größe.
    func applyAppearance(themeChanged: Bool) {
        if themeChanged {
            window.appearance = Theme.appearance
            window.backgroundColor = Theme.bg
            split.needsDisplay = true
        }
        let root = window.contentView!.bounds
        bar.frame = NSRect(x: 0, y: 0, width: root.width, height: Theme.barHeight)
        split.frame = NSRect(x: 0, y: Theme.barHeight, width: root.width, height: root.height - Theme.barHeight)
        wallpaper.frame = split.frame
        wallpaper.needsDisplay = true
        bar.needsDisplay = true
        sidebar.needsDisplay = true
        extensionPanel.layoutRows()
    }

    // MARK: Rechte Sidebar

    private var rightWidthKey: String { "rightSidebar.width" + Self.suffix(index) }

    /// Neue Panels aller Extensions. Gleich wie bisher (eine Logzeile hat `onChange` ausgelöst): nichts anfassen.
    /// Verschwindet der Tab, der die Tastatur hat, bekommt sie die Arbeitsfläche, bevor der Bereich womöglich wegfällt.
    func updateExtensionPanels(_ panels: [(name: String, tree: PanelTree)]) {
        guard !ExtensionPanelView.same(extensionPanel.panels, panels) else { return }
        let tab = extensionPanel.selectedTab
        if window.firstResponder === extensionPanel, !panels.contains(where: { $0.name == tab }) { leaveRightSidebar() }
        extensionPanel.panels = panels
        applyRightSidebar()
    }

    func toggleRightSidebar() { isRightSidebarHidden.toggle() }

    /// ⌘3: Tastatur in die rechte Sidebar, ausgeblendet erst einblenden. false, wenn es keine Panels gibt.
    func focusRightSidebar() -> Bool {
        guard !extensionPanel.panels.isEmpty else { return false }
        if isRightSidebarHidden { isRightSidebarHidden = false }
        return window.makeFirstResponder(extensionPanel)
    }

    /// Esc in der rechten Sidebar: Tastatur an die fokussierte Kachel, ohne Kachel an die Arbeitsfläche.
    func leaveRightSidebar() {
        if let f = workspace.focused { workspace.setFocus(f) } else { window.makeFirstResponder(workspace) }
    }

    /// Bereich rein oder raus aus dem Split, wie der Baum ohne Trenner, wenn er fehlt. Kommt auf der gespeicherten Breite zurück.
    private func applyRightSidebar() {
        let show = !isRightSidebarHidden && !extensionPanel.panels.isEmpty
        guard show != (rightScroll.superview != nil) else { return }
        if show {
            let width = CGFloat(max(Profile.defaults.object(forKey: rightWidthKey) as? Double ?? 260, 120))
            // Beim Einfügen verteilt der Split die Breite auf alle Bereiche: den Baum danach auf seine Breite zurück.
            let tree = isSidebarHidden ? nil : sidebarScroll.frame.width
            rightScroll.frame.size = NSSize(width: width, height: split.bounds.height)
            split.addArrangedSubview(rightScroll)
            split.layoutSubtreeIfNeeded()
            let last = split.arrangedSubviews.count - 1
            split.setHoldingPriority(.defaultLow + 1, forSubviewAt: last)
            split.setPosition(split.bounds.width - width - split.dividerThickness, ofDividerAt: last - 1)
            if let tree { split.setPosition(tree, ofDividerAt: 0) }
        } else {
            if window.firstResponder === extensionPanel { leaveRightSidebar() }
            rightScroll.removeFromSuperview()
        }
        window.invalidateCursorRects(for: split)
    }

    /// Trenner `i` liegt links vor der rechten Sidebar (Index hängt davon ab, ob der Baum sichtbar ist).
    fileprivate func isRightDivider(_ i: Int) -> Bool {
        let views = split.arrangedSubviews
        return views.indices.contains(i + 1) && views[i + 1] === rightScroll
    }
}

/// Baum und rechte Sidebar sind je höchstens halb so breit wie das Fenster: beim Ziehen und wenn das Fenster schmaler wird.
extension MainWindowController: NSSplitViewDelegate {
    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        isRightDivider(dividerIndex) ? proposedMaximumPosition : min(proposedMaximumPosition, splitView.bounds.width / 2)
    }

    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        isRightDivider(dividerIndex) ? max(proposedMinimumPosition, splitView.bounds.width / 2) : proposedMinimumPosition
    }

    /// Ziehen startet nur im effektiven Rechteck, so breit wie die Griffzone.
    func splitView(_ splitView: NSSplitView, effectiveRect proposedEffectiveRect: NSRect, forDrawnRect drawnRect: NSRect, ofDividerAt dividerIndex: Int) -> NSRect {
        drawnRect.insetBy(dx: -ThinSplitView.grabWidth / 2, dy: 0)
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        let half = split.bounds.width / 2
        if !isSidebarHidden, sidebarScroll.frame.width > half + 1 { split.setPosition(half, ofDividerAt: 0) }
        if !isSidebarHidden, sidebarWidthRestored { Profile.defaults.set(Double(sidebarScroll.frame.width), forKey: sidebarWidthKey) }
        if rightScroll.superview != nil {
            if rightScroll.frame.width > half + 1 { split.setPosition(split.bounds.width - half, ofDividerAt: split.arrangedSubviews.count - 2) }
            if rightScroll.frame.width > 0 { Profile.defaults.set(Double(rightScroll.frame.width), forKey: rightWidthKey) }
        }
        // Der Trenner ist gewandert (Ziehen, UI-Größe, Baum ein/aus): Griffzone für Cursor und Mausbewegung nachziehen.
        split.updateTrackingAreas()
        window.invalidateCursorRects(for: split)
    }
}
