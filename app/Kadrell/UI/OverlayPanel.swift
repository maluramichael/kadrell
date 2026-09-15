import AppKit
import SwiftUI

/// Rahmenloses Overlay über dem Hauptfenster für die SwiftUI-Dialoge: 1 px Linie, keine Rundung.
/// Esc und ⌘⏎ werden hier abgefangen, bevor SwiftUI sie sieht.
@MainActor
final class OverlayPanel: NSPanel {
    var onCancel: (() -> Void)?
    var onPrimary: (() -> Void)?

    init<V: View>(rootView: V) {
        let host = NSHostingView(rootView: rootView)
        host.sizingOptions = [.intrinsicContentSize]
        let size = host.fittingSize
        super.init(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        isOpaque = true
        backgroundColor = Theme.panel
        hasShadow = true
        appearance = NSAppearance(named: .darkAqua)
        host.frame = NSRect(origin: .zero, size: size)
        host.wantsLayer = true
        host.layer?.borderColor = Theme.line.cgColor
        host.layer?.borderWidth = 1
        contentView = host
    }

    override var canBecomeKey: Bool { true }

    func open(over parent: NSWindow) {
        let pf = parent.frame
        setFrameOrigin(NSPoint(x: pf.midX - frame.width / 2, y: pf.maxY - 0.12 * pf.height - frame.height))
        parent.addChildWindow(self, ordered: .above)
        makeKeyAndOrderFront(nil)
    }

    func dismiss() {
        parent?.removeChildWindow(self)
        orderOut(nil)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.keyCode == 53 { onCancel?(); return true }
        if event.keyCode == 36, event.modifierFlags.contains(.command) { onPrimary?(); return true }
        return super.performKeyEquivalent(with: event)
    }

    override func cancelOperation(_ sender: Any?) { onCancel?() }
}
