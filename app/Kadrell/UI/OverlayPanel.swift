import AppKit
import SwiftUI

/// Rahmenloses Overlay über dem Hauptfenster für die SwiftUI-Dialoge: 1 px Linie, keine Rundung.
/// Esc und ⌘⏎ werden hier abgefangen, bevor SwiftUI sie sieht.
@MainActor
final class OverlayPanel: NSPanel {
    var onCancel: (() -> Void)?
    var onPrimary: (() -> Void)?
    /// Rückfragen: auch plain ⏎ bestätigt. Dialoge mit Textfeldern brauchen ⏎ selbst und nehmen nur ⌘⏎.
    var primaryOnPlainReturn = false

    private var anchor = CGPoint.zero   // Mitte oben: bleibt beim Wachsen und Schrumpfen fest
    private var resizeObserver: NSObjectProtocol?

    init<V: View>(rootView: V) {
        let host = NSHostingController(rootView: rootView)
        host.sizingOptions = [.preferredContentSize]   // Fenster folgt der Inhaltsgröße, kein Zentrieren
        super.init(contentRect: NSRect(x: 0, y: 0, width: 640, height: 200), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        isOpaque = true
        backgroundColor = Theme.panel
        hasShadow = true
        appearance = NSAppearance(named: .darkAqua)
        contentViewController = host
        host.view.wantsLayer = true
        host.view.layer?.borderColor = Theme.line.cgColor
        host.view.layer?.borderWidth = 1
        resizeObserver = NotificationCenter.default.addObserver(forName: NSWindow.didResizeNotification, object: self, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.reanchor() }
        }
    }

    override var canBecomeKey: Bool { true }

    private func reanchor() {
        let f = frame
        let o = NSPoint(x: anchor.x - f.width / 2, y: anchor.y - f.height)
        if abs(o.x - f.origin.x) > 0.5 || abs(o.y - f.origin.y) > 0.5 { setFrameOrigin(o) }
    }

    func open(over parent: NSWindow) {
        let pf = parent.frame
        anchor = CGPoint(x: pf.midX, y: pf.maxY - 0.12 * pf.height)
        reanchor()
        parent.addChildWindow(self, ordered: .above)
        makeKeyAndOrderFront(nil)
        Backdrop.sync(parent)
    }

    func dismiss() {
        let p = parent
        p?.removeChildWindow(self)
        orderOut(nil)
        Backdrop.sync(p)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.keyCode == 53 { onCancel?(); return true }
        if event.keyCode == 36, event.modifierFlags.contains(.command) || primaryOnPlainReturn { onPrimary?(); return true }
        return super.performKeyEquivalent(with: event)
    }

    override func cancelOperation(_ sender: Any?) { onCancel?() }
}

/// Graut und blurrt das Hauptfenster, solange ein Overlay (Dialog oder ⌘P) als Kindfenster offen ist.
/// Fängt auch die Klicks ab, damit dahinter nichts ungewollt bedient wird.
@MainActor
enum Backdrop {
    private static let id = NSUserInterfaceItemIdentifier("KadrellBackdrop")

    static func sync(_ window: NSWindow?) {
        guard let root = window?.contentView else { return }
        let existing = root.subviews.first { $0.identifier == id }
        let open = window?.childWindows?.contains { $0.isVisible } == true
        if !open { existing?.removeFromSuperview(); return }
        guard existing == nil else { return }
        let backdrop = NSView(frame: root.bounds)
        backdrop.identifier = id
        backdrop.autoresizingMask = [.width, .height]
        // Der Blur-Radius von NSVisualEffectView ist fest, abgeschwächt wird er über die Deckkraft.
        let blur = NSVisualEffectView(frame: backdrop.bounds)
        blur.blendingMode = .withinWindow
        blur.material = .fullScreenUI
        blur.state = .active
        blur.alphaValue = 0.4
        blur.autoresizingMask = [.width, .height]
        let dim = NSView(frame: backdrop.bounds)
        dim.wantsLayer = true
        dim.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.3).cgColor
        dim.autoresizingMask = [.width, .height]
        backdrop.addSubview(blur)
        backdrop.addSubview(dim)
        root.addSubview(backdrop)
    }
}
