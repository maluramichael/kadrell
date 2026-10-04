import Foundation

enum ThemeColor: String, Equatable { case accent, muted, ok, warn, err }

struct PanelAction: Equatable { var id, label: String }

indirect enum PanelNode: Equatable {
    case section(title: String, collapsed: Bool, children: [PanelNode])
    case item(text: String, detail: String?, color: ThemeColor?, actions: [PanelAction])
    case text(String, ThemeColor?)
    case button(label: String, action: String)
}

struct PanelTree: Equatable {
    var title: String
    var nodes: [PanelNode]
}

enum PanelError: Error, Equatable { case tooLarge(Int) }

/// Prüft, was eine Extension als Panel oder Statuseintrag schickt: unbekannte Knoten und Felder werden übersprungen
/// und als Warnung gemeldet, zu große Bäume abgelehnt. Fremdes JSON darf nie abstürzen.
enum PanelValidation {
    static let maxNodes = 2000
    static let maxText = 500
    static let maxStatus = 40

    static func tree(_ v: JSONValue) -> Result<(PanelTree, warnings: [String]), PanelError> {
        let count = nodeCount(v["children"])
        if count > maxNodes { return .failure(.tooLarge(count)) }
        var builder = Builder()
        let nodes = builder.nodes(v["children"])
        let title = clip(v["title"]?.string ?? "", maxText)
        return .success((PanelTree(title: title, nodes: nodes), warnings: builder.warnings))
    }

    static func status(_ v: JSONValue) -> (text: String, color: ThemeColor?, action: String?)? {
        guard let text = v["text"]?.string else { return nil }
        let color = v["color"]?.string.flatMap(ThemeColor.init(rawValue:))
        return (clip(text, maxStatus), color, v["action"]?.string.map { clip($0, maxText) })
    }

    fileprivate static func clip(_ s: String, _ limit: Int) -> String { String(s.prefix(limit)) }

    /// Zählt Rohknoten vor dem Prüfen, damit ein Flut-Panel gar nicht erst gebaut wird.
    private static func nodeCount(_ children: JSONValue?) -> Int {
        (children?.array ?? []).reduce(0) { $0 + ($1.object == nil ? 0 : 1 + nodeCount($1["children"])) }
    }

    /// Sammelt Warnungen beim Bauen; ein Knoten pro Aufruf, damit jede Funktion klein bleibt.
    private struct Builder {
        var warnings: [String] = []

        mutating func nodes(_ v: JSONValue?) -> [PanelNode] {
            // Eine leere Lua-Tabelle kommt als `[]`, ein Nicht-Array (`{}`, Zahl, …) heißt: keine Kinder.
            (v?.array ?? []).compactMap { node($0) }
        }

        mutating func node(_ v: JSONValue) -> PanelNode? {
            guard v.object != nil, let type = v["type"]?.string else {
                warn(String(localized: "Knoten ohne Typ übersprungen", bundle: Bundle.app))
                return nil
            }
            switch type {
            case "section": return section(v)
            case "item": return item(v)
            case "text": return text(v)
            case "button": return button(v)
            default:
                warn(String(localized: "Unbekannter Knotentyp „\(type)“ übersprungen", bundle: Bundle.app))
                return nil
            }
        }

        mutating func section(_ v: JSONValue) -> PanelNode {
            .section(title: str(v["title"]), collapsed: v["collapsed"]?.bool ?? false, children: nodes(v["children"]))
        }

        mutating func item(_ v: JSONValue) -> PanelNode? {
            guard let text = required(v, "item", "text") else { return nil }
            return .item(text: text, detail: v["detail"]?.string.map { clip($0) }, color: color(v["color"]), actions: actions(v["actions"]))
        }

        mutating func text(_ v: JSONValue) -> PanelNode? {
            guard let text = required(v, "text", "text") else { return nil }
            return .text(text, color(v["color"]))
        }

        mutating func button(_ v: JSONValue) -> PanelNode? {
            guard let label = required(v, "button", "label"), let action = required(v, "button", "action") else { return nil }
            return .button(label: label, action: action)
        }

        mutating func actions(_ v: JSONValue?) -> [PanelAction] {
            (v?.array ?? []).compactMap { a in
                guard let id = a["id"]?.string, let label = a["label"]?.string else {
                    warn(String(localized: "Aktion ohne id oder label übersprungen", bundle: Bundle.app))
                    return nil
                }
                return PanelAction(id: clip(id), label: clip(label))
            }
        }

        mutating func color(_ v: JSONValue?) -> ThemeColor? {
            guard let v else { return nil }
            if let name = v.string, let color = ThemeColor(rawValue: name) { return color }
            warn(String(localized: "Unbekannte Farbe ignoriert (erlaubt: accent, muted, ok, warn, err)", bundle: Bundle.app))
            return nil
        }

        mutating func required(_ v: JSONValue, _ type: String, _ field: String) -> String? {
            if let s = v[field]?.string { return clip(s) }
            warn(String(localized: "Knoten „\(type)“ ohne Feld „\(field)“ übersprungen", bundle: Bundle.app))
            return nil
        }

        func str(_ v: JSONValue?) -> String { clip(v?.string ?? "") }
        func clip(_ s: String) -> String { PanelValidation.clip(s, PanelValidation.maxText) }
        mutating func warn(_ s: String) { warnings.append(s) }
    }
}
