import AppKit

/// Zugänglichkeits-Element für handgezeichnete Flächen (Baum, Leiste, Titelzeilen): Rolle, Label, Wert, Rahmen
/// in View-Koordinaten des Elternteils und optional die Klick-Aktion.
final class A11yElement: NSAccessibilityElement {
    let key: String
    var press: (@MainActor () -> Void)?
    private weak var view: NSView?
    private var rect = CGRect.zero
    /// Letzter `value`: ändert er sich (Session wechselt von „wartet“ zu „fertig“ ...), meldet das VoiceOver auch,
    /// ohne dass die Zeile neu angesteuert wird.
    private var lastValue: String?

    init(key: String) { self.key = key; super.init() }

    /// `frame` in Punkten des Elternteils, also nach `Theme.scale`.
    @discardableResult
    func update(parent: NSView, role: NSAccessibility.Role, label: String, value: String? = nil, frame: CGRect,
                press: (@MainActor () -> Void)? = nil, actions: [NSAccessibilityCustomAction] = []) -> A11yElement {
        setAccessibilityParent(parent)
        setAccessibilityRole(role)
        setAccessibilityLabel(label)
        setAccessibilityValue(value)
        if value != lastValue { lastValue = value; NSAccessibility.post(element: self, notification: .valueChanged) }
        view = parent
        rect = frame
        setAccessibilityCustomActions(actions.isEmpty ? nil : actions)
        self.press = press
        return self
    }

    /// Nicht `accessibilityFrameInParentSpace`: das ignoriert `isFlipped` (im Test gemessen), alle Views hier sind gespiegelt.
    override func accessibilityFrame() -> NSRect {
        let v = view
        let r = rect
        return MainActor.assumeIsolated { v.map { NSAccessibility.screenRect(fromView: $0, rect: r) } ?? .zero }
    }

    override func accessibilityPerformPress() -> Bool {
        guard let press else { return false }
        MainActor.assumeIsolated { press() }
        return true
    }

    override func isAccessibilitySelectorAllowed(_ selector: Selector) -> Bool {
        selector == #selector(accessibilityPerformPress) ? press != nil : super.isAccessibilitySelectorAllowed(selector)
    }
}

extension Array where Element == A11yElement {
    /// Element zum Schlüssel wiederverwenden: VoiceOver verliert sonst bei jedem Neuaufbau (Poll, Uhr) den Cursor.
    func reuse(_ key: String) -> A11yElement { first { $0.key == key } ?? A11yElement(key: key) }
}

/// Gezeichnete Trefferfläche in unskalierten Punkten: Klick, Tooltip und VoiceOver-Knopf aus einem Eintrag.
struct HitRegion {
    let rect: CGRect
    let label: String
    var value: String? = nil
    let action: @MainActor () -> Void
}

extension Array where Element == HitRegion {
    func first(at p: CGPoint) -> HitRegion? { first { $0.rect.contains(p) } }

    /// VoiceOver-Knöpfe, je Label wiederverwendet aus `old`.
    @MainActor func accessibilityElements(parent: NSView, reusing old: [A11yElement]) -> [A11yElement] {
        map { h in old.reuse(h.label).update(parent: parent, role: .button, label: h.label, value: h.value, frame: h.rect.scaled(Theme.scale), press: h.action) }
    }
}

extension SessionStatus {
    /// Vorgelesener Status.
    var spoken: String {
        switch self { case .running: String(localized: "arbeitet", bundle: Bundle.app); case .waiting: String(localized: "wartet", bundle: Bundle.app); case .idle: String(localized: "fertig", bundle: Bundle.app); case .error: String(localized: "Fehler", bundle: Bundle.app) }
    }

    /// Vorgelesener Status einer Session: ohne laufenden Prozess „beendet“ (`ended`) oder „nicht gestartet“.
    func spoken(attached: Bool, ended: Bool = false) -> String {
        attached ? spoken : ended ? String(localized: "beendet", bundle: Bundle.app) : String(localized: "nicht gestartet", bundle: Bundle.app)
    }
}

/// Knopf im Aktionsmenü von VoiceOver (VO-⌘-Leertaste) für Hover-Knöpfe, die sonst nur die Maus erreicht.
@MainActor
func a11yAction(_ name: String, _ run: @escaping @MainActor () -> Void) -> NSAccessibilityCustomAction {
    NSAccessibilityCustomAction(name: name) { MainActor.assumeIsolated { run() }; return true }
}
