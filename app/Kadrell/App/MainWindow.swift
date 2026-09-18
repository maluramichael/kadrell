import AppKit

/// Ein Hauptfenster: Baum, Arbeitsfläche und Leiste mit eigener Auswahl, eigenem Layout und Fokus. Sessions und Gruppen
/// hält der AppDelegate, Marke „neu“, Hooks und Sounds der `AttentionTracker`, beide für alle Fenster gemeinsam. Fenster 0 speichert unter den
/// bisherigen Schlüsseln, weitere mit Suffix („workspace.selected.2“).
@MainActor
final class MainWindowController: NSObject {
    let index: Int
    let window: NSWindow
    let bar = StatusBarView(frame: .zero)
    let split = ThinSplitView(frame: .zero)
    let sidebarScroll = NSScrollView(frame: .zero)
    let sidebar = SidebarView(frame: .zero)
    let workspace: WorkspaceView
    /// Dritte Region rechts: die Ticket-Leiste (Suchfeld oben, darunter die Liste). Ohne Provider bleibt sie versteckt.
    let rightContainer = RightPanelContainer(frame: .zero)
    let ticketPanel = TicketPanelView(frame: .zero)

    static func suffix(_ index: Int) -> String { index == 0 ? "" : ".\(index + 1)" }

    /// `cascade`: Fenster, gegen das ein neues ohne gespeicherte Lage versetzt aufgeht.
    init(index: Int, cascade: NSWindow?) {
        self.index = index
        workspace = WorkspaceView(frame: .zero, defaultsSuffix: Self.suffix(index))
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
        sidebarScroll.drawsBackground = true
        sidebarScroll.backgroundColor = Theme.panel
        sidebar.autoresizingMask = [.width]
        sidebar.frame = NSRect(x: 0, y: 0, width: 260, height: 10)
        rightContainer.scroll.documentView = ticketPanel
        rightContainer.scroll.hasVerticalScroller = true
        rightContainer.scroll.autohidesScrollers = true
        rightContainer.scroll.drawsBackground = true
        rightContainer.scroll.backgroundColor = Theme.panel
        ticketPanel.autoresizingMask = [.width]
        ticketPanel.frame = NSRect(x: 0, y: 0, width: 300, height: 10)
        rightContainer.search.placeholderString = String(localized: "Tickets filtern", bundle: Bundle.app)
        rightContainer.search.font = Theme.font(12)
        rightContainer.search.target = ticketPanel
        rightContainer.search.action = #selector(TicketPanelView.filterChanged(_:))
        split.isVertical = true
        split.dividerStyle = .thin
        split.addArrangedSubview(sidebarScroll)
        split.addArrangedSubview(workspace)
        split.addArrangedSubview(rightContainer)
        split.setHoldingPriority(.defaultLow + 1, forSubviewAt: 0)
        split.setHoldingPriority(.defaultLow + 1, forSubviewAt: 2)
        split.autosaveName = "KadrellSplit" + Self.suffix(index)
        // Nach der Autosave-Wiederherstellung: die Einstellung entscheidet über die Sichtbarkeit der rechten Region.
        rightContainer.isHidden = !Settings.rightPanelVisible
        split.delegate = self
        split.frame = NSRect(x: 0, y: Theme.barHeight, width: root.bounds.width, height: root.bounds.height - Theme.barHeight)
        split.autoresizingMask = [.width, .height]
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
        if split.arrangedSubviews[0].frame.width < 120 { split.setPosition(260, ofDividerAt: 0) }
        if !rightContainer.isHidden, rightContainer.frame.width < 120 { ensureRightWidth() }
        window.makeFirstResponder(workspace)
    }

    /// Divider vor der rechten Region (Reihenfolge Baum, Arbeitsfläche, rechte Region ist fix).
    static let rightDivider = 1
    /// Setzt die rechte Region auf eine sinnvolle Startbreite, wenn sie zu schmal (frisch eingeblendet) ist.
    func ensureRightWidth() { split.setPosition(split.bounds.width - 300, ofDividerAt: Self.rightDivider) }

    /// Darstellung aus den Einstellungen: Farben nur bei neuem Theme, Leiste und Split folgen der UI-Größe.
    func applyAppearance(themeChanged: Bool) {
        if themeChanged {
            window.appearance = Theme.appearance
            window.backgroundColor = Theme.bg
            sidebarScroll.backgroundColor = Theme.panel
            rightContainer.scroll.backgroundColor = Theme.panel
            split.needsDisplay = true
        }
        let root = window.contentView!.bounds
        bar.frame = NSRect(x: 0, y: 0, width: root.width, height: Theme.barHeight)
        split.frame = NSRect(x: 0, y: Theme.barHeight, width: root.width, height: root.height - Theme.barHeight)
        bar.needsDisplay = true
        sidebar.needsDisplay = true
    }
}

/// Baum und rechte Region sind je höchstens halb so breit wie das Fenster: beim Ziehen und wenn das Fenster schmaler wird.
extension MainWindowController: NSSplitViewDelegate {
    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        dividerIndex == 0 ? min(proposedMaximumPosition, splitView.bounds.width / 2) : proposedMaximumPosition
    }

    /// Der Divider vor der rechten Region darf nicht über die Mitte nach links, sonst wäre die Region breiter als halb.
    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        dividerIndex == Self.rightDivider ? max(proposedMinimumPosition, splitView.bounds.width / 2) : proposedMinimumPosition
    }

    /// Ziehen startet nur im effektiven Rechteck, ohne das hier ist es die 1-px-Linie: so breit wie die Griffzone.
    func splitView(_ splitView: NSSplitView, effectiveRect proposedEffectiveRect: NSRect, forDrawnRect drawnRect: NSRect, ofDividerAt dividerIndex: Int) -> NSRect {
        drawnRect.insetBy(dx: -ThinSplitView.grabWidth / 2, dy: 0)
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        let half = split.bounds.width / 2
        if !sidebarScroll.isHidden, sidebarScroll.frame.width > half + 1 { split.setPosition(half, ofDividerAt: 0) }
        if !rightContainer.isHidden, rightContainer.frame.width > half + 1 { split.setPosition(split.bounds.width - half, ofDividerAt: Self.rightDivider) }
        // Der Trenner ist gewandert (Ziehen, UI-Größe, Baum ein/aus): Griffzone für Cursor und Mausbewegung nachziehen.
        split.updateTrackingAreas()
        window.invalidateCursorRects(for: split)
    }
}
