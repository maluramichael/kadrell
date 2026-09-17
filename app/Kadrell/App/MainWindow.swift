import AppKit

/// Ein Hauptfenster: Baum, Arbeitsfläche und Leiste mit eigener Auswahl, eigenem Layout und Fokus. Sessions, Gruppen,
/// Marke „neu“, Hooks und Sounds hält der AppDelegate für alle Fenster gemeinsam. Fenster 0 speichert unter den
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

    static func suffix(_ index: Int) -> String { index == 0 ? "" : ".\(index + 1)" }

    /// `cascade`: Fenster, gegen das ein neues ohne gespeicherte Lage versetzt aufgeht.
    init(index: Int, cascade: NSWindow?) {
        self.index = index
        workspace = WorkspaceView(frame: .zero, defaultsSuffix: Self.suffix(index))
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1400, height: 900),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        super.init()
        window.isReleasedWhenClosed = false
        window.title = "Kadrell" + Profile.label + (index == 0 ? "" : String(localized: " · Fenster \(index + 1)"))
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
        split.isVertical = true
        split.dividerStyle = .thin
        split.addArrangedSubview(sidebarScroll)
        split.addArrangedSubview(workspace)
        split.setHoldingPriority(.defaultLow + 1, forSubviewAt: 0)
        split.autosaveName = "KadrellSplit" + Self.suffix(index)
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
        window.makeFirstResponder(workspace)
    }

    /// Darstellung aus den Einstellungen: Farben nur bei neuem Theme, Leiste und Split folgen der UI-Größe.
    func applyAppearance(themeChanged: Bool) {
        if themeChanged {
            window.appearance = Theme.appearance
            window.backgroundColor = Theme.bg
            sidebarScroll.backgroundColor = Theme.panel
            split.needsDisplay = true
        }
        let root = window.contentView!.bounds
        bar.frame = NSRect(x: 0, y: 0, width: root.width, height: Theme.barHeight)
        split.frame = NSRect(x: 0, y: Theme.barHeight, width: root.width, height: root.height - Theme.barHeight)
        bar.needsDisplay = true
        sidebar.needsDisplay = true
    }
}

/// Der Baum ist höchstens halb so breit wie das Fenster: beim Ziehen und wenn das Fenster schmaler wird.
extension MainWindowController: NSSplitViewDelegate {
    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        min(proposedMaximumPosition, splitView.bounds.width / 2)
    }

    /// Ziehen startet nur im effektiven Rechteck, ohne das hier ist es die 1-px-Linie: so breit wie die Griffzone.
    func splitView(_ splitView: NSSplitView, effectiveRect proposedEffectiveRect: NSRect, forDrawnRect drawnRect: NSRect, ofDividerAt dividerIndex: Int) -> NSRect {
        drawnRect.insetBy(dx: -ThinSplitView.grabWidth / 2, dy: 0)
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        let half = split.bounds.width / 2
        if !sidebarScroll.isHidden, sidebarScroll.frame.width > half + 1 { split.setPosition(half, ofDividerAt: 0) }
        // Der Trenner ist gewandert (Ziehen, UI-Größe, Baum ein/aus): Griffzone für Cursor und Mausbewegung nachziehen.
        split.updateTrackingAreas()
        window.invalidateCursorRects(for: split)
    }
}
