import SwiftUI

/// Bausteine aller Einstellungsseiten: Überschrift, Tabelle, Zeile und die Bedienelemente der rechten Spalte.
extension SettingsView {
    /// Breite der rechten Spalte: jedes Dropdown, Feld und Kürzel ist gleich breit, die Einstellungen lesen sich wie eine Tabelle.
    var controlWidth: CGFloat { 300 * Theme.scale }

    /// Bereichsüberschrift wie in der F1-Hilfe: Akzentfarbe, gesperrt, Großbuchstaben, optionaler Hinweis daneben.
    func heading(_ s: String, note: String = "", first: Bool = false) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(s.uppercased()).kerning(0.6).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.runningColor)
            if !note.isEmpty { Text(note).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor) }
        }
        .padding(.horizontal, 16).padding(.top, first ? 14 : 18).padding(.bottom, 6)
    }

    /// Zeilen einer Tabelle mit feiner Linie dazwischen.
    func table<C: View>(@ViewBuilder _ rows: () -> C) -> some View {
        VStack(spacing: 0) {
            SwiftUI.Group(subviews: rows()) { subviews in
                ForEach(subviews.indices, id: \.self) { i in
                    if i > 0 { Divider().overlay(Theme.lineColor) }
                    subviews[i]
                }
            }
        }
        .overlay(Rectangle().stroke(Theme.lineColor, lineWidth: 1))
        .padding(.horizontal, 16)
    }

    func setting<C: View>(_ title: String, @ViewBuilder _ control: () -> C) -> some View {
        HStack(spacing: 12) {
            Text(title).foregroundStyle(Theme.fgColor).lineLimit(1)
            Spacer(minLength: 12)
            control().frame(width: controlWidth)
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }

    /// Dropdown im App-Stil, füllt die rechte Spalte. Das native Popup nimmt nur die Breite seines Textes.
    func menu<T: Hashable>(_ selection: Binding<T>, _ items: [(T, String)], width: CGFloat? = nil) -> some View {
        let width = width ?? controlWidth
        return Menu {
            Picker("", selection: selection) {
                ForEach(items, id: \.0) { Text($0.1).tag($0.0) }
            }
            .pickerStyle(.inline).labelsHidden()
        } label: {
            HStack(spacing: 4) {
                Text(items.first { $0.0 == selection.wrappedValue }?.1 ?? "–").lineLimit(1).foregroundStyle(Theme.fgColor)
                Spacer(minLength: 4)
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 9 * Theme.scale)).foregroundStyle(Theme.mutedColor)
            }
            .padding(.horizontal, 8).padding(.vertical, 3)
            .frame(width: width).background(Theme.bgColor).contentShape(Rectangle())
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).kbdFocusRing()
        .frame(width: width)
    }

    /// Ganzzahliger Schieberegler mit Wert rechts daneben. `factor` rechnet gespeicherte Faktoren in Prozent um.
    /// `live: false` übernimmt erst beim Loslassen (UI-Größe: sonst skaliert der Dialog unter der Maus mit).
    func slider(_ value: Binding<Double>, _ range: ClosedRange<Double>, step: Double, factor: Double = 1, unit: String, live: Bool = true) -> some View {
        IntSlider(value: value, range: range, step: step, factor: factor, unit: unit, live: live)
    }

    /// Ganzzahlige Einstellung als Schieberegler.
    func intSlider(_ value: Binding<Int>, _ range: ClosedRange<Double>, step: Double, unit: String) -> some View {
        slider(Binding(get: { Double(value.wrappedValue) }, set: { value.wrappedValue = Int($0) }), range, step: step, unit: unit)
    }

    /// Pfadfeld der rechten Spalte, nimmt Dateien per Drag & Drop an.
    func pathField(_ text: Binding<String>, placeholder: String = "", onTab: @escaping () -> Void = {}, drop: @escaping ([URL]) -> Bool) -> some View {
        PathField(text: text, placeholder: placeholder, autofocus: false, size: 12, onTab: onTab, onSubmit: {}, onMove: { _ in })
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(Theme.bgColor)
            .dropDestination(for: URL.self) { urls, _ in drop(urls) }
    }

    /// Einzeiliges Textfeld der rechten Spalte.
    func textField(_ text: Binding<String>, placeholder: String) -> some View {
        TextField(placeholder, text: text)
            .textFieldStyle(.plain).foregroundStyle(Theme.fgColor)
            .padding(.horizontal, 8).padding(.vertical, 3).background(Theme.bgColor)
    }

    func iconButton(_ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            // Höhe knapp unter der Zeilenhöhe der Felder daneben, sonst zieht der Knopf die Tabellenzeile in die Länge.
            Image(systemName: symbol).foregroundStyle(Theme.mutedColor).frame(width: 22 * Theme.scale, height: 18 * Theme.scale)
        }
        .buttonStyle(.plain).kbdFocusRing()
    }

    /// Text-Knopf in einer Tabellenzeile.
    func textButton(_ title: String, color: Color = Theme.fgColor, action: @escaping () -> Void) -> some View {
        Button(action: action) { Text(title).foregroundStyle(color) }.buttonStyle(.plain).kbdFocusRing()
    }

    func onOff(_ value: Binding<Bool>) -> some View { menu(value, [(true, String(localized: "an", bundle: Bundle.app)), (false, String(localized: "aus", bundle: Bundle.app))]) }

    /// Texte als Dropdown. "" heißt Default der CLI, `defaultLabel` ersetzt dann „Standard“.
    func options(_ values: [String], _ value: Binding<String>, defaultLabel: String? = nil, width: CGFloat? = nil) -> some View {
        let standard = defaultLabel ?? String(localized: "Standard", bundle: Bundle.app)
        return menu(value, values.map { ($0, $0.isEmpty ? standard : $0) }, width: width)
    }
}

struct IntSlider: View {
    @Binding var value: Double
    let range: ClosedRange<Double>, step: Double, factor: Double, unit: String, live: Bool
    /// Wert während des Ziehens, solange er noch nicht übernommen ist.
    @State private var draft: Double?

    var body: some View {
        let shown = draft ?? (value * factor).rounded()
        HStack(spacing: 10) {
            Slider(value: Binding(get: { shown }, set: { v in
                let r = (v / step).rounded() * step
                if live { value = r / factor } else { draft = r }
            }), in: range, step: step) { editing in
                if !editing, let d = draft { value = d / factor; draft = nil }
            }
            .controlSize(.small).tint(Theme.runningColor)
            Text("\(Int(shown)) \(unit)").monospacedDigit().foregroundStyle(Theme.fgColor)
                .frame(width: 60 * Theme.scale, alignment: .trailing)
        }
    }
}

/// Auswahl „Auto-Modus zeigt“: zwei Schalter als ein Dropdown.
struct AutoShow: Hashable {
    let waiting: Bool
    let running: Bool
}
