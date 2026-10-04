import Foundation

/// Freie JSON-Werte (Einstellungen, Event-Daten, Panel-Bäume), bevor Kadrell sie prüft.
enum JSONValue: Codable, Equatable {
    case string(String), number(Double), bool(Bool), null, array([JSONValue]), object([String: JSONValue])

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let v = try? c.decode(Bool.self) { self = .bool(v) }
        else if let v = try? c.decode(Double.self) { self = .number(v) }
        else if let v = try? c.decode(String.self) { self = .string(v) }
        else if let v = try? c.decode([JSONValue].self) { self = .array(v) }
        else { self = .object(try c.decode([String: JSONValue].self)) }
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .string(let v): try c.encode(v)
        case .number(let v): try c.encode(v)
        case .bool(let v): try c.encode(v)
        case .null: try c.encodeNil()
        case .array(let v): try c.encode(v)
        case .object(let v): try c.encode(v)
        }
    }

    var string: String? { if case .string(let v) = self { return v } else { return nil } }
    var bool: Bool? { if case .bool(let v) = self { return v } else { return nil } }
    var array: [JSONValue]? { if case .array(let v) = self { return v } else { return nil } }
    var object: [String: JSONValue]? { if case .object(let v) = self { return v } else { return nil } }
    /// Ganzzahl; json.lua schreibt `3.0` als `3`, ein Bruch oder eine riesige Zahl ist keine id.
    var int: Int? {
        guard case .number(let v) = self, v == v.rounded(), abs(v) < 1e15 else { return nil }
        return Int(v)
    }
    subscript(key: String) -> JSONValue? { object?[key] }
}

/// `kadrell.json` einer Extension.
struct ExtensionManifest: Codable, Equatable {
    struct Setting: Codable, Equatable {
        var key, type, label: String
        /// `string`, `secret` oder `bool`; `secret` liegt im Schlüsselbund, nie in einer Datei.
        var `default`: JSONValue?
    }

    static let supportedAPI = 1

    var name, version, description: String
    var apiVersion: Int
    var permissions: [String]
    var settings: [Setting]
}

extension ExtensionManifest {
    /// Nur `name` und `apiVersion` sind Pflicht; ein Manifest ohne Beschreibung oder Einstellungen ist gültig.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        apiVersion = try c.decode(Int.self, forKey: .apiVersion)
        version = try c.decodeIfPresent(String.self, forKey: .version) ?? ""
        description = try c.decodeIfPresent(String.self, forKey: .description) ?? ""
        permissions = try c.decodeIfPresent([String].self, forKey: .permissions) ?? []
        settings = try c.decodeIfPresent([Setting].self, forKey: .settings) ?? []
    }
}

/// Ein Ordner unter `~/.config/kadrell/extensions`; `problem` ist ein Klartext für den Dialog, dann lädt sie nicht.
struct FoundExtension: Equatable {
    var name: String
    var dir: URL
    var manifest: ExtensionManifest?
    var problem: String?
}

enum ExtensionCatalog {
    static let dir = URL(fileURLWithPath: NSHomeDirectory() + "/.config/kadrell/extensions")

    static func scan(_ dir: URL = dir) -> [FoundExtension] {
        let fm = FileManager.default
        let names = ((try? fm.contentsOfDirectory(atPath: dir.path)) ?? []).filter { !$0.hasPrefix(".") }
        return names.sorted().compactMap { name in
            let url = dir.appendingPathComponent(name)
            var isDir: ObjCBool = false
            guard fm.fileExists(atPath: url.path, isDirectory: &isDir), isDir.boolValue else { return nil }
            return inspect(name: name, url: url)
        }
    }

    private static func inspect(name: String, url: URL) -> FoundExtension {
        let manifestURL = url.appendingPathComponent("kadrell.json")
        let manifest = (try? Data(contentsOf: manifestURL)).flatMap { try? JSONDecoder().decode(ExtensionManifest.self, from: $0) }
        return FoundExtension(name: name, dir: url, manifest: manifest, problem: problem(name: name, url: url, manifest: manifest))
    }

    private static func problem(name: String, url: URL, manifest: ExtensionManifest?) -> String? {
        guard let manifest else { return String(localized: "kadrell.json fehlt oder ist kaputt", bundle: Bundle.app) }
        if manifest.name != name {
            return String(localized: "Name im Manifest („\(manifest.name)“) passt nicht zum Ordnernamen", bundle: Bundle.app)
        }
        if manifest.apiVersion > ExtensionManifest.supportedAPI {
            return String(localized: "Braucht neueres Kadrell (API \(manifest.apiVersion), unterstützt: \(ExtensionManifest.supportedAPI))", bundle: Bundle.app)
        }
        let initLua = url.appendingPathComponent("init.lua")
        if !FileManager.default.fileExists(atPath: initLua.path) { return String(localized: "init.lua fehlt", bundle: Bundle.app) }
        // Wie bei Shell-Hooks: Ordner und Dateien müssen dir gehören und dürfen für andere nicht beschreibbar sein.
        let untrusted = [url, url.appendingPathComponent("kadrell.json"), initLua].first { !Hooks.trusted($0.path) }
        return untrusted.map { String(localized: "\($0.lastPathComponent) gehört nicht dir oder ist für andere beschreibbar", bundle: Bundle.app) }
    }
}
