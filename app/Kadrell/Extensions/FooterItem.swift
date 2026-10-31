import Foundation

/// Ein Eintrag der Statusleiste unten (`kadrell.footer.set`). Gezeichnet in dieser Reihenfolge: `label` gedämpft,
/// `bars` als senkrechte Mini-Balken, `gauge` als waagerechter Balken, `text` in `color`.
struct FooterItem: Equatable {
    enum Align: String, Equatable { case left, right }

    var id: String
    var align: Align = .left
    var label: String?
    var text: String?
    var color: ThemeColor?
    /// Werte 0...1, je einer ein senkrechter Balken (z. B. Last je Kern).
    var bars: [Double] = []
    /// Ein Wert 0...1 als waagerechter Balken (z. B. Plattenbelegung).
    var gauge: Double?
    var action: String?
    /// Tooltip; ohne steht dort, was sichtbar ist.
    var tip: String?
    /// Name in der Kommandopalette (⌘P `>`) und für VoiceOver; ohne gilt `label` und `text`.
    var title: String?

    /// Was VoiceOver und die Palette nennen, wenn `title` fehlt.
    var spoken: String {
        if let title { return title }
        let parts = [label, text].compactMap { $0 }.filter { !$0.isEmpty }
        return parts.isEmpty ? id : parts.joined(separator: " ")
    }
}

extension PanelValidation {
    static let maxFooterItems = 16
    static let maxBars = 64

    /// Einträge für die Statusleiste unten. `null` oder Nicht-Array heißt leer. Einträge ohne `id` oder ganz ohne Inhalt
    /// werden übersprungen und gemeldet, Werte auf 0...1 begrenzt, Anzahl und Längen gedeckelt. Fremdes JSON darf nie abstürzen.
    static func footer(_ v: JSONValue?) -> (items: [FooterItem], warnings: [String]) {
        var warnings: [String] = []
        let raw = v?.array ?? []
        if raw.count > maxFooterItems {
            warnings.append(String(localized: "Statusleiste: höchstens \(maxFooterItems) Einträge, der Rest fehlt", bundle: Bundle.app))
        }
        var seen = Set<String>()
        let items: [FooterItem] = raw.prefix(maxFooterItems).compactMap { entry in
            guard let item = footerItem(entry, &warnings) else { return nil }
            guard seen.insert(item.id).inserted else {
                warnings.append(String(localized: "Statusleiste: id „\(item.id)“ doppelt, übersprungen", bundle: Bundle.app))
                return nil
            }
            return item
        }
        return (items, warnings)
    }

    private static func footerItem(_ v: JSONValue, _ warnings: inout [String]) -> FooterItem? {
        guard let id = v["id"]?.string, !id.isEmpty else {
            warnings.append(String(localized: "Statusleiste: Eintrag ohne „id“ übersprungen", bundle: Bundle.app))
            return nil
        }
        var item = FooterItem(id: clip(id, maxText))
        item.align = v["align"]?.string.flatMap(FooterItem.Align.init(rawValue:)) ?? .left
        item.label = v["label"]?.string.map { clip($0, maxStatus) }
        item.text = v["text"]?.string.map { clip($0, maxStatus) }
        item.color = footerColor(v["color"], &warnings)
        item.bars = (v["bars"]?.array ?? []).prefix(maxBars).compactMap(\.unit)
        item.gauge = v["gauge"]?.unit
        item.action = v["action"]?.string.map { clip($0, maxText) }
        item.tip = v["tip"]?.string.map { clip($0, maxText) }
        item.title = v["title"]?.string.map { clip($0, maxStatus) }
        guard item.label != nil || item.text != nil || !item.bars.isEmpty || item.gauge != nil else {
            warnings.append(String(localized: "Statusleiste: Eintrag „\(item.id)“ ohne Inhalt übersprungen", bundle: Bundle.app))
            return nil
        }
        return item
    }

    private static func footerColor(_ v: JSONValue?, _ warnings: inout [String]) -> ThemeColor? {
        guard let v else { return nil }
        if let name = v.string, let color = ThemeColor(rawValue: name) { return color }
        warnings.append(String(localized: "Unbekannte Farbe ignoriert (erlaubt: accent, muted, ok, warn, err)", bundle: Bundle.app))
        return nil
    }
}

private extension JSONValue {
    /// Zahl, auf 0...1 begrenzt; `NaN` und Nicht-Zahlen ergeben nil.
    var unit: Double? {
        guard case .number(let v) = self, !v.isNaN else { return nil }
        return min(max(v, 0), 1)
    }
}
