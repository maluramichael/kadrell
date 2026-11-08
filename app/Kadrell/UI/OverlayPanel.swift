import AppKit
import SwiftUI

/// Rahmenloses Overlay über dem Hauptfenster für die SwiftUI-Dialoge. Das Fenster selbst ist durchsichtig, die
/// erhabene Fläche mit Fugenrand, Rundung und festem Schatten legt `OverlayScroll` an.
/// Esc und ⏎ werden hier abgefangen, bevor SwiftUI sie sieht.
@MainActor
final class OverlayPanel: ChildPanel {
    /// `centered`: so groß wie der Inhalt, mittig im Fenster (Rückfragen, ⌘N, F1 …). `fullWindow`: füllt das Fenster
    /// bis auf einen schmalen Rand und fährt von oben herein (Einstellungen).
    enum Placement { case centered, fullWindow }

    var onCancel: (() -> Void)?
    var onPrimary: (() -> Void)?
    /// Sieht jede Taste vor Esc und ⏎ (Listenfilter mit `/`). `true` heißt verbraucht.
    var keyHook: ((NSEvent) -> Bool)?
    /// ⏎ bestätigt, ⌘⏎ immer. Aus nur bei destruktiven Rückfragen: dort zählt allein ⌘⏎.
    var primaryOnPlainReturn = true
    let placement: Placement

    private var anchor = CGPoint.zero   // Mitte des Fensterinhalts: Dialog wächst nach oben und unten gleich
    private var resizeObserver: NSObjectProtocol?

    /// Grenzen, Schema-Zähler und Einfahrt für `OverlayScroll`; intern, damit Tests sie sehen.
    let limit: OverlayLimit

    init<V: View>(rootView: V, placement: Placement = .centered) {
        self.placement = placement
        limit = OverlayLimit(fill: placement == .fullWindow)
        let host = NSHostingController(rootView: OverlayScroll(limit: limit, content: rootView).environment(\.locale, Localization.locale))
        // Mittig folgt das Fenster der Inhaltsgröße; fensterfüllend setzt `follow` den Rahmen und der Inhalt füllt ihn.
        host.sizingOptions = placement == .centered ? [.preferredContentSize] : []
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
        guard placement == .centered else { return }
        let f = frame
        let o = NSPoint(x: anchor.x - f.width / 2, y: anchor.y - f.height / 2)
        if abs(o.x - f.origin.x) > 0.5 || abs(o.y - f.origin.y) > 0.5 { setFrameOrigin(o) }
    }

    func open(over parent: NSWindow) { attach(to: parent) }

    /// Grundregel: Größe und Lage folgen dem Fenster, beim Ändern der Größe (auch live während des Ziehens) und beim
    /// Verschieben.
    override func follow(_ parent: NSWindow) {
        let area = OverlayGeometry.area(of: parent)
        switch placement {
        case .centered:
            anchor = CGPoint(x: area.midX, y: area.midY)
            limit.setBounds(width: area.width - 2 * OverlayGeometry.margin, height: area.height - 2 * OverlayGeometry.margin)
            reanchor()
        case .fullWindow:
            let f = OverlayGeometry.fullWindow(in: area)
            let inset = OverlayGeometry.inset(of: area)
            if limit.topInset != inset { limit.topInset = inset }
            limit.setBounds(width: f.width, height: f.height - inset)
            if frame != f { setFrame(f, display: true) }
        }
    }

    /// Fensterfüllend fährt der Dialog erst hinaus und verschwindet dann. Die Tastatur geht sofort ans Hauptfenster
    /// zurück, Klicks laufen während der Ausfahrt nicht mehr in den Dialog.
    override func dismiss() {
        guard placement == .fullWindow, isVisible, limit.shown else { return super.dismiss() }
        ignoresMouseEvents = true
        parent?.makeKey()
        limit.slideOut { self.finishDismiss() }
    }

    private func finishDismiss() { super.dismiss() }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if keyHook?(event) == true { return true }
        if event.keyCode == KeyCode.escape { onCancel?(); return true }
        if event.keyCode == KeyCode.returnKey, event.modifierFlags.contains(.command) || primaryOnPlainReturn { onPrimary?(); return true }
        return super.performKeyEquivalent(with: event)
    }

    override func cancelOperation(_ sender: Any?) { onCancel?() }
}

/// Lage der Overlays über ihrem Fenster, rein rechnerisch: Tests prüfen die Regel ohne Fenster.
@MainActor
enum OverlayGeometry {
    /// Abstand zentrierter Dialoge und der Palette zum Rand des Fensterinhalts.
    static let margin: CGFloat = 24

    /// Fensterinhalt ohne Titelleiste, in Bildschirmkoordinaten.
    static func area(of parent: NSWindow) -> NSRect { parent.contentRect(forFrameRect: parent.frame) }

    /// Schmaler Rand des fensterfüllenden Dialogs: ein halbes Prozent der Breite je Seite, so ist er rund 99 % breit.
    static func inset(of area: NSRect) -> CGFloat { max(6, (area.width * 0.005).rounded()) }

    /// Rahmen des fensterfüllenden Dialogs: seitlich und unten der Rand, oben bündig unter der Titelleiste. Den oberen
    /// Rand legt `OverlayScroll` innen an, damit die Fläche beim Hereinfahren unter der Titelleiste hervorkommt.
    static func fullWindow(in area: NSRect) -> NSRect {
        let inset = inset(of: area)
        return NSRect(x: area.minX + inset, y: area.minY + inset, width: area.width - 2 * inset, height: area.height - inset).integral
    }
}

/// Zustand der Hülle, den das Panel setzt und `OverlayScroll` liest.
@Observable
@MainActor
final class OverlayLimit {
    var maxHeight = CGFloat.infinity
    var maxWidth = CGFloat.infinity
    /// Zählt Wechsel von Schema oder UI-Größe bei offenem Dialog. `OverlayScroll` setzt ihn als
    /// `instrumentThemeGeneration` in die Umgebung.
    var themeGeneration = 0
    /// Fensterfüllend: die Hülle füllt das Panel, statt der Größe des Inhalts zu folgen.
    let fill: Bool
    /// Luft über der Fläche, nur fensterfüllend.
    var topInset: CGFloat = 0
    /// Fensterfüllend: die Fläche ist eingefahren. Zentrierte Dialoge stehen sofort.
    private(set) var shown: Bool

    init(fill: Bool = false) {
        self.fill = fill
        shown = !fill
    }

    /// Nur bei echter Änderung schreiben: jeder Schreibzugriff ließe SwiftUI neu layouten, auch beim Verschieben.
    func setBounds(width: CGFloat, height: CGFloat) {
        if maxWidth != width { maxWidth = width }
        if maxHeight != height { maxHeight = height }
    }

    /// Zügig und weich von oben herein, ohne Nachfedern. Mit „Bewegung reduzieren“ nur kurz eingeblendet.
    static var slideInAnimation: Animation {
        Feedback.reduceMotion ? .easeOut(duration: 0.12) : .timingCurve(0.2, 0.85, 0.3, 1, duration: 0.24)
    }
    static var slideOutAnimation: Animation {
        Feedback.reduceMotion ? .easeIn(duration: 0.1) : .timingCurve(0.5, 0, 0.8, 0.35, duration: 0.18)
    }
    static var slideOutDuration: TimeInterval { Feedback.reduceMotion ? 0.1 : 0.18 }

    func slideIn() {
        guard !shown else { return }
        withAnimation(Self.slideInAnimation) { shown = true }
    }

    /// Fährt hinaus und ruft danach `done`. Über die Uhr statt über den Abschluss der Animation: der kommt in einem
    /// Fenster, das gerade nicht zeichnet, womöglich nie.
    func slideOut(_ done: @escaping @MainActor () -> Void) {
        withAnimation(Self.slideOutAnimation) { shown = false }
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.slideOutDuration) { MainActor.assumeIsolated { done() } }
    }
}

extension EnvironmentValues {
    /// Höchstbreite eines Dialogs: Fensterinhalt abzüglich Rand. Setzt `OverlayScroll`.
    @Entry var overlayMaxWidth = CGFloat.infinity
}

extension View {
    /// Dialogbreite in logischen Punkten, mit der UI-Größe skaliert, aber nie breiter als das Fenster abzüglich Rand
    /// (Grundregel: Overlays folgen der Fenstergröße).
    func overlayWidth(_ logical: CGFloat, alignment: Alignment = .center) -> some View {
        modifier(OverlayWidth(logical: logical, alignment: alignment))
    }
}

private struct OverlayWidth: ViewModifier {
    let logical: CGFloat
    let alignment: Alignment
    @Environment(\.overlayMaxWidth) private var maxWidth

    func body(content: Content) -> some View {
        content.frame(width: max(0, min(logical * Theme.scale, maxWidth)), alignment: alignment)
    }
}

/// Gemeinsame Hülle aller SwiftUI-Dialoge: erhabene Fläche mit Fugenrand und festem Schatten, die Dialoge malen
/// keinen eigenen Grund. Mittig wird ein Dialog samt Schatten nicht höher als bis je 24 pt an den oberen und unteren
/// Rand des Fensterinhalts, der Rest scrollt; fensterfüllend nimmt er das ganze Panel. Eine `DialogFoot` im Inhalt
/// steht fest darunter und scrollt nicht mit.
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
            .frame(height: scrollHeight)
            if let foot {
                DialogFootBar(foot: foot).frame(width: footWidth)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { footHeight = $0 }
            }
        }
        .frame(maxWidth: limit.fill ? .infinity : nil, maxHeight: limit.fill ? .infinity : nil, alignment: .top)
        .raisedPanel()
        .modifier(OverlaySlide(limit: limit))
        .onPreferenceChange(DialogFootKey.self) { foot = $0 }
        .environment(\.instrumentThemeGeneration, limit.themeGeneration)   // Schemawechsel erreicht jede Fläche im Dialog
        .environment(\.overlayMaxWidth, limit.maxWidth)
    }

    /// Mittig so breit wie der Inhalt. Fensterfüllend frei: an die gemessene Breite gebunden hielte die Fußleiste die
    /// Hülle beim Verkleinern auf der alten Breite fest.
    private var footWidth: CGFloat? { limit.fill || width <= 0 ? nil : width }

    /// Mittig so hoch wie der Inhalt, höchstens bis zur Grenze; fensterfüllend nimmt der Inhalt die ganze Höhe.
    private var scrollHeight: CGFloat? { limit.fill || height <= 0 ? nil : min(height, scrollLimit) }

    /// Platz für den scrollenden Inhalt: Maximalhöhe ohne Fußleiste und ohne den Schatten unter der Fläche.
    private var scrollLimit: CGFloat { limit.maxHeight - footHeight - Instrument.shadowDepth * Theme.scale }
}

/// Fensterfüllend: Luft oben und Einfahrt von oben. Das Panel schneidet ab, was über seiner Oberkante steht, so kommt
/// die Fläche unter der Titelleiste hervor. Mit „Bewegung reduzieren“ blendet sie nur ein.
private struct OverlaySlide: ViewModifier {
    let limit: OverlayLimit

    func body(content: Content) -> some View {
        if limit.fill {
            content.padding(.top, limit.topInset)
                .offset(y: limit.shown || Feedback.reduceMotion ? 0 : -(limit.maxHeight + 2 * limit.topInset))
                .opacity(limit.shown || !Feedback.reduceMotion ? 1 : 0)
                .onAppear { limit.slideIn() }
        } else {
            content
        }
    }
}

/// Rahmenloses Kindfenster über dem Hauptfenster (Dialoge, ⌘P): wird Key-Fenster und graut das Hauptfenster ab.
/// Grundregel aller Overlays: Größe und Lage folgen dem Fenster. Solange das Panel angehängt ist, ruft jede Größen-
/// oder Lageänderung des Fensters `follow`; Unterklassen setzen dort ihren Rahmen.
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
        ignoresMouseEvents = false
        follow(parent)
        observe(parent)
        parent.addChildWindow(self, ordered: .above)
        makeKeyAndOrderFront(nil)
        Backdrop.sync(parent)
    }

    /// Rahmen über `parent` setzen. Läuft beim Anhängen und bei jeder Größen- oder Lageänderung des Fensters.
    func follow(_ parent: NSWindow) {}

    func dismiss() {
        stopObserving()
        if let h = host { host?.removeChildWindow(self); Backdrop.sync(h); orderOut(nil) }
    }

    /// `parent` ist nach orderOut/close schon nil. Deshalb das Hauptfenster selbst merken, sonst bleibt der Blur liegen.
    private weak var host: NSWindow?
    private var hostObservers: [NSObjectProtocol] = []
    override func orderOut(_ sender: Any?) { super.orderOut(sender); Backdrop.sync(host) }
    override func close() { stopObserving(); super.close(); Backdrop.sync(host) }

    /// Größe live (auch während des Ziehens und beim Vollbild) und Lage (Verschieben, anderer Bildschirm).
    private func observe(_ parent: NSWindow) {
        stopObserving()
        let names = [NSWindow.didResizeNotification, NSWindow.didMoveNotification, NSWindow.didChangeScreenNotification]
        hostObservers = names.map { name in
            NotificationCenter.default.addObserver(forName: name, object: parent, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.followHost() }
            }
        }
    }

    private func followHost() { if let host { follow(host) } }

    private func stopObserving() {
        hostObservers.forEach { NotificationCenter.default.removeObserver($0) }
        hostObservers = []
    }
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
