import SwiftUI

/// Filter für Listen in Overlays (Einstellungen, Hilfe), bedient wie `/` in tmux, vim und less: `/` öffnet das Feld,
/// die Liste filtert beim Tippen, ⏎ geht zum ersten Treffer, Esc beendet den Filter. Esc schließt den Dialog erst,
/// wenn kein Filter mehr offen ist. Ein `/` in einem Textfeld ist ein normales Zeichen.
@Observable
@MainActor
final class ListFilter {
    var query = ""
    private(set) var isOpen = false
    /// Zählt hoch, sobald das Eingabefeld den Fokus nehmen soll (erstes Öffnen und jedes weitere `/`).
    private(set) var focusTick = 0
    /// Die View meldet hier, ob das Feld gerade den Fokus hat: nur dann geht ⏎ an den Filter statt an den Dialog.
    @ObservationIgnored var fieldFocused = false

    var needle: String { query.trimmingCharacters(in: .whitespacesAndNewlines) }
    /// Filtert wirklich: Feld offen und etwas eingegeben.
    var isActive: Bool { isOpen && !needle.isEmpty }

    /// Alle Wörter der Eingabe müssen vorkommen, Groß- und Kleinschreibung und Umlaute zählen nicht.
    nonisolated static func matches(_ text: String, query: String) -> Bool {
        let words = query.split(whereSeparator: \.isWhitespace)
        return words.allSatisfy { text.range(of: String($0), options: [.caseInsensitive, .diacriticInsensitive]) != nil }
    }

    /// Bleibt der Eintrag sichtbar? Ohne aktiven Filter immer.
    func keeps(_ texts: [String]) -> Bool { !isActive || Self.matches(texts.joined(separator: " "), query: needle) }

    func open() {
        isOpen = true
        focusTick += 1
    }

    func close() {
        isOpen = false
        query = ""
        fieldFocused = false
    }

    /// Taste im Panel, vor allem anderen. `true` heißt verbraucht.
    func handle(_ event: NSEvent, in window: NSWindow?) -> Bool {
        guard event.modifierFlags.intersection([.command, .control, .option]).isEmpty else { return false }
        switch event.keyCode {
        case KeyCode.escape:
            guard isOpen else { return false }
            close()
            window?.makeFirstResponder(nil)
            return true
        case KeyCode.returnKey:
            guard isOpen, fieldFocused else { return false }
            jumpToFirstHit(in: window)
            return true
        default:
            guard event.characters == "/", !(window?.firstResponder is NSText) else { return false }
            open()
            return true
        }
    }

    /// Das Feld gibt die Tastatur ab, der Fokus springt auf das nächste Bedienelement, bei gefilterter Liste der erste Treffer.
    private func jumpToFirstHit(in window: NSWindow?) {
        fieldFocused = false
        window?.selectNextKeyView(nil)
    }
}

extension EnvironmentValues {
    @Entry var listFilter: ListFilter?
}

/// Anzahl der Treffer einer gefilterten Liste, von den Abschnitten nach oben gemeldet.
struct FilterHitsKey: PreferenceKey {
    static var defaultValue: Int { 0 }
    static func reduce(value: inout Int, nextValue: () -> Int) { value += nextValue() }
}

extension View {
    /// Hängt den Filter an das Overlay-Fenster dieser View (Tasten) und in die Umgebung (Zeilen filtern sich selbst).
    func listFilter(_ filter: ListFilter) -> some View {
        environment(\.listFilter, filter).background(FilterKeyHook(filter: filter))
    }
}

/// Trägt `ListFilter.handle` in das `OverlayPanel` ein, sobald die View in einem Fenster steht.
private struct FilterKeyHook: NSViewRepresentable {
    let filter: ListFilter

    func makeNSView(context: Context) -> NSView { HookView(filter: filter) }
    func updateNSView(_ view: NSView, context: Context) {}

    private final class HookView: NSView {
        let filter: ListFilter
        init(filter: ListFilter) {
            self.filter = filter
            super.init(frame: .zero)
        }
        required init?(coder: NSCoder) { nil }

        override func viewDidMoveToWindow() {
            guard let panel = window as? OverlayPanel else { return }
            panel.keyHook = { [weak filter, weak panel] event in filter?.handle(event, in: panel) ?? false }
        }
    }
}

/// Eingabezeile des Filters: „/“, Feld, Trefferzahl. Nur sichtbar, solange der Filter offen ist.
struct FilterBar: View {
    @Bindable var filter: ListFilter
    let hits: Int
    @FocusState private var focused: Bool

    var body: some View {
        if filter.isOpen {
            HStack(spacing: 8) {
                Text("/").font(Theme.ui(12, bold: true)).foregroundStyle(Theme.accentTextColor)
                TextField(String(localized: "Filtern …", bundle: Bundle.app), text: $filter.query)
                    .textFieldStyle(.plain).foregroundStyle(Theme.fgColor)
                    .focused($focused).instrumentWell(focused: focused)
                if filter.isActive {
                    Text(hits == 0 ? String(localized: "Keine Treffer", bundle: Bundle.app) : String(localized: "\(hits) Treffer", bundle: Bundle.app))
                        .foregroundStyle(Theme.mutedColor)
                }
            }
            .font(Theme.ui(12))
            .padding(.horizontal, 16).padding(.top, 10)
            .onAppear { focused = true }
            .onChange(of: filter.focusTick) { focused = true }
            .onChange(of: focused) { filter.fieldFocused = focused }
        }
    }
}
