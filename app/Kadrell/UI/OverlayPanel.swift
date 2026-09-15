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
    }

    func dismiss() {
        parent?.removeChildWindow(self)
        orderOut(nil)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.keyCode == 53 { onCancel?(); return true }
        if event.keyCode == 36, event.modifierFlags.contains(.command) || primaryOnPlainReturn { onPrimary?(); return true }
        return super.performKeyEquivalent(with: event)
    }

    override func cancelOperation(_ sender: Any?) { onCancel?() }
}
