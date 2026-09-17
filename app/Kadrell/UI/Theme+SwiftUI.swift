import SwiftUI

extension Theme {
    /// Schrift der SwiftUI-Dialoge, mit der UI-Größe skaliert.
    static func ui(_ size: CGFloat, bold: Bool = false) -> Font {
        .custom(bold ? "JetBrainsMonoNF-Bold" : "JetBrainsMonoNF-Regular", size: size * scale)
    }

    static var bgColor: Color { Color(nsColor: bg) }
    static var panelColor: Color { Color(nsColor: panel) }
    static var surfaceColor: Color { Color(nsColor: surface) }
    static var lineColor: Color { Color(nsColor: line) }
    static var fgColor: Color { Color(nsColor: fg) }
    static var mutedColor: Color { Color(nsColor: muted) }
    static var runningColor: Color { Color(nsColor: running) }
    static let errorColor = Color(nsColor: error)
}

/// Sichtbarer Tastaturfokus für `.buttonStyle(.plain)`-Elemente (Menüs, Icon-Knöpfe, Dialog-Fuß, Farbfelder):
/// die verstecken sonst den nativen Fokusring, Tab-Nutzer sehen dann nirgends, wo der Fokus gerade steht.
private struct KeyboardFocusRing: ViewModifier {
    @FocusState private var focused: Bool
    func body(content: Content) -> some View {
        content.focused($focused)
            .overlay(RoundedRectangle(cornerRadius: 4).stroke(Theme.runningColor, lineWidth: focused ? 2 : 0).padding(-2))
    }
}

extension View {
    func kbdFocusRing() -> some View { modifier(KeyboardFocusRing()) }

    /// Rahmen der schmalen Eingabe-Dialoge (⌘N, Gruppe bearbeiten, Umbenennen).
    func dialogFrame() -> some View {
        font(Theme.ui(12)).foregroundStyle(Theme.fgColor).frame(width: 640 * Theme.scale).background(Theme.panelColor)
    }
}

extension Text {
    /// Überschrift oben links im Dialog: klein, gesperrt, gedämpft.
    func dialogTitle() -> some View {
        font(Theme.ui(11)).kerning(0.6).foregroundStyle(Theme.mutedColor)
            .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.top, 12)
    }
}
