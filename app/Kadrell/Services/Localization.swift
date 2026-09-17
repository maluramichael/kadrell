import Foundation

/// Oberflächensprache. macOS legt sie beim Start fest, ein Wechsel bräuchte dort einen Neustart, und der würde alle
/// Claude-Prozesse mitnehmen. Kadrell holt seine Texte deshalb selbst aus dem Bundle der eingestellten Sprache:
/// AppKit über `String(localized: …, bundle: Bundle.app)`, die SwiftUI-Dialoge über `\.locale` in der Umgebung.
enum Localization {
    enum Language: String, CaseIterable {
        case system, de, en
        /// Name in der jeweiligen Sprache, damit die Auswahl auch lesbar ist, wenn die Oberfläche fremd ist.
        var title: String {
            switch self {
            case .system: String(localized: "Sprache des Systems", bundle: Bundle.app)
            case .de: "Deutsch"
            case .en: "English"
            }
        }
        var flag: String {
            switch self {
            case .system: "🌐"
            case .de: "🇩🇪"
            case .en: "🇬🇧"
            }
        }
    }

    /// Bundle der eingestellten Sprache, sonst `Bundle.main` (Sprache des Systems).
    nonisolated(unsafe) private(set) static var bundle = Bundle.main
    nonisolated(unsafe) private(set) static var locale = Locale.current

    static func apply(_ language: Language) {
        guard language != .system, let path = Bundle.main.path(forResource: language.rawValue, ofType: "lproj"),
              let selected = Bundle(path: path) else {
            bundle = .main
            locale = .current
            return
        }
        bundle = selected
        locale = Locale(identifier: language.rawValue)
    }
}

extension Bundle {
    /// Bundle der eingestellten Sprache. Jeder Nutzertext in AppKit holt seine Übersetzung hierüber.
    static var app: Bundle { Localization.bundle }
}
