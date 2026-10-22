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
    static var accentColor: Color { Color(nsColor: accent) }
    static var errorColor: Color { Color(nsColor: error) }
    static var raisedColor: Color { Color(nsColor: raised) }
    static var wellColor: Color { Color(nsColor: well) }
    static var accentTextColor: Color { Color(nsColor: accentText) }
    static var errorTextColor: Color { Color(nsColor: errorText) }
}

/// Sichtbarer Tastaturfokus für `.buttonStyle(.plain)`-Elemente (Menüs, Icon-Knöpfe, Dialog-Fuß, Farbfelder):
/// die verstecken sonst den nativen Fokusring, Tab-Nutzer sehen dann nirgends, wo der Fokus gerade steht.
private struct KeyboardFocusRing: ViewModifier {
    @FocusState private var focused: Bool
    func body(content: Content) -> some View {
        content.focused($focused)
            .overlay(RoundedRectangle(cornerRadius: (Instrument.keyRadius + 2) * Theme.scale).stroke(Theme.accentColor, lineWidth: focused ? 2 : 0).padding(-2))
    }
}

extension View {
    func kbdFocusRing() -> some View { modifier(KeyboardFocusRing()) }

    /// Rahmen der schmalen Eingabe-Dialoge (⌘N, Gruppe bearbeiten, Umbenennen). Ohne eigenen Grund: die erhabene
    /// Fläche legt die gemeinsame Hülle der Overlays an.
    func dialogFrame() -> some View {
        font(Theme.ui(12)).foregroundStyle(Theme.fgColor).frame(width: 640 * Theme.scale)
    }
}

extension Text {
    /// Überschrift oben links im Dialog: gesperrtes Versal-Label, gedämpft.
    func dialogTitle() -> some View {
        capsLabel().frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 16).padding(.top, 12)
    }
}
