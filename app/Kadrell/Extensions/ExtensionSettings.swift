import Foundation

/// Werte der `settings` aus dem Manifest. Normale liegen in den Einstellungen des Profils, `secret` im Schlüsselbund
/// (ein Eintrag pro Profil, Extension und Schlüssel), fehlt ein Wert, gilt der Default aus dem Manifest.
enum ExtensionSettings {
    static let keychainService = "de.malura.kadrell.extension"

    static func values(for ext: FoundExtension) async -> [String: JSONValue] {
        var out: [String: JSONValue] = [:]
        for s in ext.manifest?.settings ?? [] {
            out[s.key] = await stored(s, ext.name) ?? s.default
        }
        return out
    }

    /// `null` oder ein leerer Text löscht den Wert, danach gilt wieder der Default. Unbekannte Schlüssel ignoriert.
    static func set(_ value: JSONValue, for ext: FoundExtension, key: String) async {
        guard let s = ext.manifest?.settings.first(where: { $0.key == key }) else { return }
        let text = value.string.flatMap { $0.isEmpty ? nil : $0 }
        switch s.type {
        case "secret":
            let account = account(ext.name, key)
            if let text { await Keychain.write(service: keychainService, account: account, value: text) }
            else { await Keychain.delete(service: keychainService, account: account) }
        case "bool": Profile.defaults.set(value.bool, forKey: defaultsKey(ext.name, key))
        case "number": Profile.defaults.set(number(value), forKey: defaultsKey(ext.name, key))
        default: Profile.defaults.set(text, forKey: defaultsKey(ext.name, key))
        }
    }

    /// Zahl aus dem Formular: Text mit Punkt oder Komma, oder schon eine Zahl. Alles andere löscht den Wert.
    static func number(_ value: JSONValue) -> Double? {
        if case .number(let v) = value { return v.isFinite ? v : nil }
        let text = value.string?.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ",", with: ".") ?? ""
        return Double(text).flatMap { $0.isFinite ? $0 : nil }
    }

    static func account(_ name: String, _ key: String) -> String { "\(Profile.name ?? "default")/\(name)/\(key)" }

    private static func defaultsKey(_ name: String, _ key: String) -> String { "extensions.\(name).\(key)" }

    private static func stored(_ s: ExtensionManifest.Setting, _ name: String) async -> JSONValue? {
        switch s.type {
        case "secret": await Keychain.read(service: keychainService, account: account(name, s.key)).map(JSONValue.string)
        case "bool": (Profile.defaults.object(forKey: defaultsKey(name, s.key)) as? Bool).map(JSONValue.bool)
        case "number": (Profile.defaults.object(forKey: defaultsKey(name, s.key)) as? Double).map(JSONValue.number)
        default: Profile.defaults.string(forKey: defaultsKey(name, s.key)).map(JSONValue.string)
        }
    }
}
