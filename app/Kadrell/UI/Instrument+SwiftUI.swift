import SwiftUI

/// SwiftUI-Hülle um `InstrumentSurfaceView`: gezeichnet wird ausschließlich im AppKit-Kit. `themeID` und `scale`
/// stehen nur hier, damit SwiftUI bei Schema- oder Größenwechsel `updateNSView` ruft und neu gezeichnet wird.
/// Das greift nur, wenn die umgebende View neu rendert; deshalb hängt sie zusätzlich am Schema-Zähler der Umgebung.
struct InstrumentSurface: NSViewRepresentable {
    let part: InstrumentSurfaceView.Part
    var themeID = Theme.current.id
    var scale = Theme.scale
    /// Wird nie gelesen und muss trotzdem bleiben: erst die Eigenschaft meldet die Abhängigkeit vom Umgebungswert an,
    /// ohne sie ruft SwiftUI bei einem Wechsel von Schema oder UI-Größe kein `updateNSView`.
    @Environment(\.instrumentThemeGeneration) private var themeGeneration

    func makeNSView(context: Context) -> InstrumentSurfaceView { InstrumentSurfaceView(part: part) }

    func updateNSView(_ view: InstrumentSurfaceView, context: Context) {
        view.part = part
        view.needsDisplay = true
    }
}

extension EnvironmentValues {
    /// Zählt Wechsel von Schema oder UI-Größe bei offenem Dialog (setzt `OverlayScroll`). Jede `InstrumentSurface` liest ihn: so erreicht
    /// ein Wechsel auch Flächen in Views, deren Eingaben gleich bleiben und die SwiftUI sonst überspringt (Fuge, Taste).
    @Entry var instrumentThemeGeneration = 0
}

/// Waagrechte Fuge: 1 pt dunkel, 1 pt hell.
struct SeamLine: View {
    var body: some View {
        InstrumentSurface(part: .hSeam).frame(height: 2).accessibilityHidden(true)
    }
}

extension View {
    /// Erhabene Fläche mit Fugenring und festem Schatten. Unten wächst die View um die Schattentiefe. Bewusst ohne
    /// `clipShape`: Fokusringe am Rand bleiben sichtbar.
    func raisedPanel() -> some View {
        padding(.bottom, Instrument.shadowDepth * Theme.scale)
            .background(InstrumentSurface(part: .raised).accessibilityHidden(true))
    }

    /// Gruppe (Einstellungstabelle, Formular): eingesetzte Fläche mit Fugenring.
    func instrumentGroup() -> some View {
        background(InstrumentSurface(part: .group).accessibilityHidden(true))
    }

    /// Eingelassenes Feld für Werte, mit Fokus im Akzentring.
    func instrumentWell(focused: Bool = false) -> some View {
        padding(.horizontal, 8).padding(.vertical, 3)
            .background(InstrumentSurface(part: .well(focused: focused)).accessibilityHidden(true))
    }

    /// Auswahl in Dialog-Listen: Fläche mit Umriss in `fg`, ohne Akzentring (die Tastatur steht im Suchfeld).
    /// Die Bedingung sitzt im Hintergrund, damit die Zeile beim Auswählen ihre Identität behält.
    func instrumentRow(selected: Bool) -> some View {
        background {
            if selected { InstrumentSurface(part: .row(outline: true, ring: false)).accessibilityHidden(true) }
        }
    }
}

/// Taste: Kappe mit Dicke darunter, gedrückt sinkt sie 1 pt ein. Primär in `accent`, destruktiv in `error`, sonst neutral.
struct InstrumentKeyStyle: ButtonStyle {
    let role: KeyRole
    let compact: Bool

    init(_ role: KeyRole = .neutral, compact: Bool = false) {
        self.role = role
        self.compact = compact
    }

    func makeBody(configuration: Configuration) -> some View {
        KeyLabel(label: configuration.label, role: role, compact: compact)
            .offset(y: configuration.isPressed ? Theme.scale : 0)
            .padding(.bottom, Instrument.keyDepth * Theme.scale)
            .background(InstrumentSurface(part: .key(pressed: configuration.isPressed, role: role)).accessibilityHidden(true))
            // Die gezeichnete Kappe nimmt keine Klicks an (`hitTest` = nil); ohne Form träfe nur die Schrift.
            .contentShape(RoundedRectangle(cornerRadius: Instrument.keyRadius * Theme.scale))
    }

    /// Beschriftung der Kappe; liest `isEnabled`, damit eine gesperrte Taste gedämpft erscheint.
    private struct KeyLabel: View {
        let label: ButtonStyleConfiguration.Label
        let role: KeyRole
        let compact: Bool
        @Environment(\.isEnabled) private var enabled

        var body: some View {
            label.font(compact ? Theme.ui(11) : Theme.ui(12, bold: role != .neutral))
                .foregroundStyle(enabled ? Color(nsColor: role.label) : Theme.mutedColor)
                .padding(.horizontal, compact ? 6 : 12).padding(.vertical, compact ? 2 : 5)
        }
    }
}

extension Text {
    /// Gesperrtes Versal-Label: klein, gesperrt, standardmäßig gedämpft.
    func capsLabel(_ color: Color = Theme.mutedColor, bold: Bool = false) -> some View {
        font(Theme.ui(11, bold: bold)).kerning(Instrument.labelKern * Theme.scale).foregroundStyle(color)
    }

    /// Abschnittstitel: gesperrtes Versal-Label in Akzent-Textfarbe.
    func sectionTitle() -> some View { capsLabel(Theme.accentTextColor, bold: true) }
}
