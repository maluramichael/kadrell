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
    /// Zählt Wechsel von Schema oder UI-Größe bei offenem Dialog (setzt `OverlayScroll`). Jede `InstrumentSurface` liest ihn, ebenso
    /// Taste, Feld, Versal-Label und Fokusring: so erreicht ein Wechsel auch Views, deren Eingaben gleich bleiben und die
    /// SwiftUI sonst überspringt (Fuge, Taste in einer Tabellenzeile).
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

    /// Eingelassenes Feld für Werte, mit Fokus im Akzentring. Die gezeichnete Fläche nimmt keine Klicks an
    /// (`hitTest` = nil); erst die Form macht das ganze Feld klickbar, nicht nur die Schrift (Kürzel-Feld, Dropdown).
    func instrumentWell(focused: Bool = false) -> some View { modifier(InstrumentWell(focused: focused)) }

    /// Auswahl in Dialog-Listen: Fläche mit Umriss in `fg`, ohne Akzentring (die Tastatur steht im Suchfeld).
    /// Die Bedingung sitzt im Hintergrund, damit die Zeile beim Auswählen ihre Identität behält. Ohne Auswahl hat
    /// die Zeile keinen Grund; die Form hält sie trotzdem auf ganzer Breite klickbar.
    func instrumentRow(selected: Bool) -> some View {
        background {
            if selected { InstrumentSurface(part: .row(outline: true, ring: false)).accessibilityHidden(true) }
        }
        .contentShape(Rectangle())
    }
}

/// Feld zu `instrumentWell`: liest den Schema-Zähler, damit die Form der Klickfläche nach einem Größenwechsel stimmt.
private struct InstrumentWell: ViewModifier {
    let focused: Bool
    @Environment(\.instrumentThemeGeneration) private var themeGeneration

    func body(content: Content) -> some View {
        content.padding(.horizontal, 8).padding(.vertical, 3)
            .background(InstrumentSurface(part: .well(focused: focused)).accessibilityHidden(true))
            .contentShape(RoundedRectangle(cornerRadius: Instrument.keyRadius * Theme.scale))
    }
}

/// Taste: Kappe mit Dicke darunter, gedrückt sinkt sie 1 pt ein. Primär in `accent`, destruktiv in `error`, sonst neutral.
/// Alles, was Schema oder UI-Größe liest, sitzt in `KeyBody`: SwiftUI ruft `makeBody` bei gleicher Rolle nicht neu auf,
/// erst der Schema-Zähler der Umgebung erreicht bei offenem Dialog Beschriftung, Schrift, Abstände und Dicke.
struct InstrumentKeyStyle: ButtonStyle {
    let role: KeyRole
    let compact: Bool

    init(_ role: KeyRole = .neutral, compact: Bool = false) {
        self.role = role
        self.compact = compact
    }

    func makeBody(configuration: Configuration) -> some View {
        KeyBody(label: configuration.label, pressed: configuration.isPressed, role: role, compact: compact)
    }

    /// Kappe samt Beschriftung; liest `isEnabled`, damit eine gesperrte Taste gedämpft erscheint.
    private struct KeyBody: View {
        let label: ButtonStyleConfiguration.Label
        let pressed: Bool
        let role: KeyRole
        let compact: Bool
        @Environment(\.isEnabled) private var enabled
        @Environment(\.instrumentThemeGeneration) private var themeGeneration

        var body: some View {
            label.font(compact ? Theme.ui(11) : Theme.ui(12, bold: role != .neutral))
                .foregroundStyle(enabled ? Color(nsColor: role.label) : Theme.mutedColor)
                .padding(.horizontal, compact ? 6 : 12).padding(.vertical, compact ? 2 : 5)
                .offset(y: pressed ? Theme.scale : 0)
                .padding(.bottom, Instrument.keyDepth * Theme.scale)
                .background(InstrumentSurface(part: .key(pressed: pressed, role: role)).accessibilityHidden(true))
                // Die gezeichnete Kappe nimmt keine Klicks an (`hitTest` = nil); ohne Form träfe nur die Schrift.
                .contentShape(RoundedRectangle(cornerRadius: Instrument.keyRadius * Theme.scale))
        }
    }
}

extension Text {
    /// Gesperrtes Versal-Label: klein, gesperrt, gedämpft; mit `accent` in Akzent-Textfarbe.
    func capsLabel(bold: Bool = false, accent: Bool = false) -> some View { CapsLabel(text: self, bold: bold, accent: accent) }

    /// Abschnittstitel: gesperrtes Versal-Label in Akzent-Textfarbe.
    func sectionTitle() -> some View { capsLabel(bold: true, accent: true) }
}

/// Versal-Label als eigene View: die Farbe entsteht erst hier, beim Rendern mit dem Schema-Zähler, nicht beim Aufrufer.
/// So folgt ein Titel dem Schema auch in Views, die SwiftUI bei gleichen Eingaben überspringt.
private struct CapsLabel: View {
    let text: Text
    let bold: Bool
    let accent: Bool
    @Environment(\.instrumentThemeGeneration) private var themeGeneration

    var body: some View {
        text.font(Theme.ui(11, bold: bold)).kerning(Instrument.labelKern * Theme.scale)
            .foregroundStyle(accent ? Theme.accentTextColor : Theme.mutedColor)
    }
}
