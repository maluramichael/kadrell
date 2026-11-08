import SwiftUI

// Bausteine der Einstellungen (⌘,). Jeder Abschnitt ist eine eigene View, die `SettingsBlock` erfüllt und darin
// einen `SettingsSection` liefert; `SettingsView.sections` reiht sie auf, `SettingsColumns` verteilt sie auf eine
// oder zwei Spalten. Ein neuer Abschnitt braucht nur eine eigene View und eine Zeile in `SettingsView.sections`.

/// Abschnitt der Einstellungen: Überschrift wie in der F1-Hilfe, darunter die Zeilen als Tabelle mit Fugen.
/// Mit offenem Filter bleiben die Zeilen, deren Titel passt (alle, wenn Titel oder Hinweis des Abschnitts passt);
/// ohne Zeile verschwindet der Abschnitt samt Überschrift.
struct SettingsSection<Rows: View>: View {
    let title: String
    var note = ""
    @ViewBuilder let rows: Rows
    @Environment(\.listFilter) private var filter

    init(_ title: String, note: String = "", @ViewBuilder rows: () -> Rows) {
        self.title = title
        self.note = note
        self.rows = rows()
    }

    var body: some View {
        SwiftUI.Group(subviews: rows) { subviews in
            let shown = visible(subviews)
            let filtering = filter?.isActive == true
            if filtering && shown.isEmpty {
                SettingsHidden()
            } else {
                VStack(alignment: .leading, spacing: 0) {
                    SettingsHeading(title: title, note: note)
                    SettingsSubviewTable(subviews: shown)
                }
                .preference(key: FilterHitsKey.self, value: filtering ? shown.count : 0)
            }
        }
    }

    private func visible(_ subviews: SubviewsCollection) -> [Subview] {
        guard let filter, filter.isActive, !filter.keeps([title, note]) else { return Array(subviews) }
        return subviews.filter { sub in sub.containerValues.settingTitle.map { filter.keeps([$0]) } ?? false }
    }
}

extension ContainerValues {
    /// Titel einer Zeile (`setting(_:_:)`), nach dem der Filter sie findet.
    @Entry var settingTitle: String?
}

/// Platzhalter für einen ausgeblendeten Abschnitt: ohne Höhe und ohne den Spaltenabstand, der sonst bliebe.
struct SettingsHidden: View {
    var body: some View {
        Color.clear.frame(height: 0).padding(.top, -SettingsColumns<EmptyView>.spacing)
    }
}

/// Bereichsüberschrift: gesperrte Versalien in Akzent-Textfarbe, optionaler Hinweis daneben.
struct SettingsHeading: View {
    let title: String
    var note = ""

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(title.uppercased()).sectionTitle()
            if !note.isEmpty { Text(note).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1) }
        }
        .padding(.horizontal, 16).padding(.top, 14).padding(.bottom, 6)
    }
}

/// Zeilen einer Tabelle als Gruppe mit Fugenring, Fugen zwischen den Zeilen.
struct SettingsTable<Rows: View>: View {
    @ViewBuilder let rows: Rows

    var body: some View {
        SwiftUI.Group(subviews: rows) { SettingsSubviewTable(subviews: Array($0)) }
    }
}

/// Die Tabelle aus schon gesammelten Zeilen: `SettingsSection` filtert sie vorher.
struct SettingsSubviewTable: View {
    let subviews: [Subview]

    var body: some View {
        VStack(spacing: 0) {
            ForEach(subviews.indices, id: \.self) { i in
                if i > 0 { SeamLine() }
                subviews[i]
            }
        }
        .instrumentGroup()
        .padding(.horizontal, 16)
    }
}

/// Zeile: Beschriftung links (höchstens zwei Zeilen), Steuerelement rechts in der für alle Zeilen gleichen Breite.
struct SettingRow<Control: View>: View {
    let title: String
    @ViewBuilder let control: Control
    @Environment(\.settingsControlWidth) private var controlWidth

    var body: some View {
        HStack(spacing: 12) {
            Text(title).foregroundStyle(Theme.fgColor).lineLimit(2).fixedSize(horizontal: false, vertical: true)
                .layoutPriority(1)   // freier Platz geht an die Beschriftung, nicht an den Spacer
            Spacer(minLength: 12)
            control.frame(width: controlWidth)
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }
}

extension EnvironmentValues {
    /// Breite der rechten Spalte jeder Zeile: jedes Dropdown, Feld und Kürzel ist gleich breit, die Einstellungen lesen
    /// sich wie eine Tabelle. Setzt `SettingsColumns` passend zur Spaltenbreite.
    @Entry var settingsControlWidth: CGFloat = 300
}

/// Ordnet die Abschnitte in einer oder zwei Spalten an, je nach Breite, und setzt die Breite der Steuerelemente.
/// Die Abschnitte bleiben in ihrer Reihenfolge: die linke Spalte nimmt die ersten, die rechte den Rest, geteilt dort,
/// wo beide etwa gleich lang sind. Jede Spalte ist ein eigener Fokusbereich: Tab läuft erst die linke Spalte hinunter,
/// dann die rechte, nicht zeilenweise quer über beide.
struct SettingsColumns<Content: View>: View {
    @ViewBuilder let content: Content
    @State private var width: CGFloat = 0
    /// Gemessene Höhe je Abschnitt. Beide Spalten sind gleich breit, die Höhe hängt also nicht an der Spalte.
    @State private var heights: [Subview.ID: CGFloat] = [:]

    static var spacing: CGFloat { 4 }

    var body: some View {
        let columns = ColumnBalance.count(width: width, minColumnWidth: ColumnBalance.minColumnWidth)
        VStack(spacing: 0) {
            // Misst die angebotene Breite, nicht die eigene: zwei Spalten mit festen Steuerelementen würden beim
            // Verkleinern sonst ihre alte Breite melden und nie zurück auf eine Spalte fallen.
            Color.clear.frame(height: 0)
                .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
            SwiftUI.Group(subviews: content) { subviews in
                let left = columns == 2 ? ColumnBalance.split(subviews.map { heights[$0.id] ?? 0 }, spacing: Self.spacing) : subviews.count
                HStack(alignment: .top, spacing: 0) {
                    column(subviews[..<left])
                    if columns == 2 { column(subviews[left...]) }
                }
            }
        }
        .environment(\.settingsControlWidth, ColumnBalance.controlWidth(forColumn: width / CGFloat(columns)))
    }

    private func column(_ sections: SubviewsCollection.SubSequence) -> some View {
        VStack(alignment: .leading, spacing: Self.spacing) {
            ForEach(sections) { section in
                section.onGeometryChange(for: CGFloat.self) { $0.size.height } action: { heights[section.id] = $0 }
            }
        }
        .frame(maxWidth: .infinity, alignment: .topLeading)
        .focusSection()
    }
}

/// Rechnung hinter `SettingsColumns`, ohne Views: Spaltenzahl, Teilung und Breite der Steuerelemente.
@MainActor
enum ColumnBalance {
    /// Ab dieser Spaltenbreite stehen zwei Spalten nebeneinander, darunter eine.
    static var minColumnWidth: CGFloat { 600 * Theme.scale }

    static func count(width: CGFloat, minColumnWidth: CGFloat) -> Int { width >= 2 * minColumnWidth ? 2 : 1 }

    /// Anzahl der Abschnitte in der linken Spalte, bei der die höhere Spalte am niedrigsten ist. Bei Gleichstand
    /// bekommt die linke Spalte mehr, damit ein einzelner Abschnitt links steht und nicht rechts.
    static func split(_ heights: [CGFloat], spacing: CGFloat = 0) -> Int {
        let column = { (part: ArraySlice<CGFloat>) in part.reduce(0, +) + spacing * CGFloat(max(0, part.count - 1)) }
        let longer = { (k: Int) in max(column(heights[..<k]), column(heights[k...])) }
        return (0...heights.count).min { longer($0) < longer($1) || (longer($0) == longer($1) && $0 > $1) } ?? heights.count
    }

    /// Rund 45 % der Spalte nach Abzug der Ränder, höchstens 300 pt (logisch). Ohne gemessene Breite die volle Breite.
    static func controlWidth(forColumn column: CGFloat) -> CGFloat {
        let full = 300 * Theme.scale
        guard column > 0 else { return full }
        return max(120 * Theme.scale, min(full, ((column - 64) * 0.45).rounded()))
    }
}

/// Abschnitt der Einstellungen. Bringt die Steuerelemente im App-Stil mit, alle in der Breite der rechten Spalte.
protocol SettingsBlock: View {}

extension SettingsBlock {
    func setting<C: View>(_ title: String, @ViewBuilder _ control: () -> C) -> some View {
        SettingRow(title: title, control: control).containerValue(\.settingTitle, title)
    }

    /// Dropdown im App-Stil, füllt die rechte Spalte. Das native Popup nimmt nur die Breite seines Textes.
    func menu<T: Hashable>(_ selection: Binding<T>, _ items: [(T, String)]) -> some View {
        Menu {
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
            .instrumentWell()
            .frame(maxWidth: .infinity).contentShape(Rectangle())
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).kbdFocusRing()
        .frame(maxWidth: .infinity)
    }

    func onOff(_ value: Binding<Bool>) -> some View {
        menu(value, [(true, String(localized: "an", bundle: Bundle.app)), (false, String(localized: "aus", bundle: Bundle.app))])
    }

    /// Texte als Dropdown. "" heißt Claude-Default.
    func options(_ values: [String], _ value: Binding<String>) -> some View {
        menu(value, values.map { ($0, $0.isEmpty ? String(localized: "Standard", bundle: Bundle.app) : $0) })
    }

    /// Ganzzahliger Schieberegler mit Wert rechts daneben. `factor` rechnet gespeicherte Faktoren in Prozent um.
    /// `live: false` übernimmt erst beim Loslassen (UI-Größe: sonst skaliert der Dialog unter der Maus mit).
    func slider(_ value: Binding<Double>, _ range: ClosedRange<Double>, step: Double, factor: Double = 1, unit: String, live: Bool = true) -> some View {
        IntSlider(value: value, range: range, step: step, factor: factor, unit: unit, live: live)
    }

    /// Schieberegler für ganzzahlige Einstellungen.
    func slider(_ value: Binding<Int>, _ range: ClosedRange<Double>, step: Double, unit: String) -> some View {
        slider(Binding(get: { Double(value.wrappedValue) }, set: { value.wrappedValue = Int($0) }), range, step: step, unit: unit)
    }

    /// Pfadfeld der rechten Spalte, nimmt Dateien per Drag & Drop an.
    func pathField(_ text: Binding<String>, placeholder: String = "", onTab: @escaping () -> Void = {}, drop: @escaping ([URL]) -> Bool) -> some View {
        PathField(text: text, placeholder: placeholder, autofocus: false, size: 12, onTab: onTab, onSubmit: {}, onMove: { _ in })
            .dropDestination(for: URL.self) { urls, _ in drop(urls) }
    }

    func iconButton(_ symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            // Symbol plus Dicke der Taste so hoch wie bisher der Knopf (18), sonst zieht die Taste die Tabellenzeile in die Länge.
            Image(systemName: symbol).frame(width: 22 * Theme.scale, height: 16 * Theme.scale)
        }
        .buttonStyle(InstrumentKeyStyle(.neutral, compact: true)).kbdFocusRing()
    }

    /// Kleine neutrale Taste in einer Zeile (Konten wechseln, entfernen, hinzufügen).
    func smallKey(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Text(title) }.buttonStyle(InstrumentKeyStyle(.neutral, compact: true)).kbdFocusRing()
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
            .controlSize(.small).tint(Theme.accentColor)
            Text("\(Int(shown)) \(unit)").monospacedDigit().foregroundStyle(Theme.fgColor)
                .frame(width: 60 * Theme.scale, alignment: .trailing)
        }
    }
}
