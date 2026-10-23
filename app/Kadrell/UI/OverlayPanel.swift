import AppKit
import SwiftUI

/// Rahmenloses Overlay über dem Hauptfenster für die SwiftUI-Dialoge. Das Fenster selbst ist durchsichtig, die
/// erhabene Fläche mit Fugenrand, Rundung und festem Schatten legt `OverlayScroll` an.
/// Esc und ⏎ werden hier abgefangen, bevor SwiftUI sie sieht.
@MainActor
final class OverlayPanel: ChildPanel {
    var onCancel: (() -> Void)?
    var onPrimary: (() -> Void)?
    /// ⏎ bestätigt, ⌘⏎ immer. Aus nur bei destruktiven Rückfragen: dort zählt allein ⌘⏎.
    var primaryOnPlainReturn = true

    private var anchor = CGPoint.zero   // Mitte des Hauptfensters: Dialog wächst nach oben und unten gleich
    private var resizeObserver: NSObjectProtocol?

    /// Maximalhöhe und Schema-Zähler für `OverlayScroll`; intern, damit Tests den Zähler sehen.
    let limit = OverlayLimit()

    init<V: View>(rootView: V) {
        let host = NSHostingController(rootView: OverlayScroll(limit: limit, content: rootView).environment(\.locale, Localization.locale))
        host.sizingOptions = [.preferredContentSize]   // Fenster folgt der Inhaltsgröße, kein Zentrieren
        super.init(size: NSSize(width: 640, height: 200))
        isOpaque = false
        backgroundColor = .clear
        contentViewController = host
        resizeObserver = NotificationCenter.default.addObserver(forName: NSWindow.didResizeNotification, object: self, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.reanchor() }
        }
    }

    /// Fläche, Rand, Felder und Tasten ziehen bei einem Wechsel von Schema oder UI-Größe nach, solange der Dialog offen
    /// ist (Einstellungen): `OverlayScroll` reicht den Zähler als Umgebungswert an jede Instrument-Fläche weiter und
    /// rechnet Schattenpolster und Scrollhöhe mit dem neuen Maßstab.
    func applyTheme() {
        appearance = Theme.appearance
        limit.themeGeneration += 1
    }

    private func reanchor() {
        let f = frame
        let o = NSPoint(x: anchor.x - f.width / 2, y: anchor.y - f.height / 2)
        if abs(o.x - f.origin.x) > 0.5 || abs(o.y - f.origin.y) > 0.5 { setFrameOrigin(o) }
    }

    func open(over parent: NSWindow) {
        let pf = parent.frame
        anchor = CGPoint(x: pf.midX, y: pf.midY)
        limit.maxHeight = pf.height - 2 * 24
        reanchor()
        attach(to: parent)
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.keyCode == KeyCode.escape { onCancel?(); return true }
        if event.keyCode == KeyCode.returnKey, event.modifierFlags.contains(.command) || primaryOnPlainReturn { onPrimary?(); return true }
        return super.performKeyEquivalent(with: event)
    }

    override func cancelOperation(_ sender: Any?) { onCancel?() }
}

@Observable
@MainActor
final class OverlayLimit {
    var maxHeight = CGFloat.infinity
    /// Zählt Wechsel von Schema oder UI-Größe bei offenem Dialog. `OverlayScroll` setzt ihn als
    /// `instrumentThemeGeneration` in die Umgebung.
    var themeGeneration = 0
}

/// Gemeinsame Hülle aller SwiftUI-Dialoge: erhabene Fläche mit Fugenrand und festem Schatten, die Dialoge malen
/// keinen eigenen Grund. Höher als bis je 24 pt an den oberen und unteren Fensterrand wird ein Dialog samt Schatten
/// nicht, der Rest scrollt. Eine `DialogFoot` im Inhalt steht fest darunter und scrollt nicht mit.
struct OverlayScroll<Content: View>: View {
    let limit: OverlayLimit
    let content: Content
    @State private var height: CGFloat = 0
    @State private var width: CGFloat = 0
    @State private var footHeight: CGFloat = 0
    @State private var foot: DialogFootData?

    var body: some View {
        VStack(spacing: 0) {
            ScrollView(.vertical) {
                content.onGeometryChange(for: CGSize.self) { $0.size } action: { height = $0.height; width = $0.width }
            }
            .scrollBounceBehavior(.basedOnSize)
            .frame(height: height > 0 ? min(height, scrollLimit) : nil)
            if let foot {
                DialogFootBar(foot: foot).frame(width: width > 0 ? width : nil)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { footHeight = $0 }
            }
        }
        .raisedPanel()
        .onPreferenceChange(DialogFootKey.self) { foot = $0 }
        .environment(\.instrumentThemeGeneration, limit.themeGeneration)   // Schemawechsel erreicht jede Fläche im Dialog
    }

    /// Platz für den scrollenden Inhalt: Maximalhöhe ohne Fußleiste und ohne den Schatten unter der Fläche.
    private var scrollLimit: CGFloat { limit.maxHeight - footHeight - Instrument.shadowDepth * Theme.scale }
}

/// Rahmenloses Kindfenster über dem Hauptfenster (Dialoge, ⌘P): wird Key-Fenster und graut das Hauptfenster ab.
@MainActor
class ChildPanel: NSPanel {
    init(size: NSSize) {
        super.init(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .floating
        hasShadow = false   // fester Schatten kommt aus dem Instrument-Kit, kein weicher Systemschatten
        appearance = Theme.appearance
    }

    override var canBecomeKey: Bool { true }

    func attach(to parent: NSWindow) {
        host = parent
        parent.addChildWindow(self, ordered: .above)
        makeKeyAndOrderFront(nil)
        Backdrop.sync(parent)
    }

    func dismiss() {
        if let h = host { host?.removeChildWindow(self); Backdrop.sync(h); orderOut(nil) }
    }

    /// `parent` ist nach orderOut/close schon nil. Deshalb das Hauptfenster selbst merken, sonst bleibt der Blur liegen.
    private weak var host: NSWindow?
    override func orderOut(_ sender: Any?) { super.orderOut(sender); Backdrop.sync(host) }
    override func close() { super.close(); Backdrop.sync(host) }
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
