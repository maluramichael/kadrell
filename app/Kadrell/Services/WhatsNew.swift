import Foundation

/// Changelog-Zeilen der aktuellen Version, aus dem als Ressource mitgelieferten CHANGELOG.md (siehe project.yml).
/// Einmalig nach einem Versionssprung gezeigt (`AppDelegate`), zusätzlich jederzeit über „Was ist neu“ im Menü.
enum WhatsNew {
    /// Zeilen der Version ohne führendes „- ", leer wenn der Abschnitt fehlt. `## 1.38.0 (…)` startet ihn, die
    /// nächste `## `-Überschrift beendet ihn.
    static func notes(version: String, changelog: String) -> [String] {
        var lines: [String] = []
        var inSection = false
        for raw in changelog.components(separatedBy: "\n") {
            if raw.hasPrefix("## ") {
                if inSection { break }
                inSection = raw == "## \(version)" || raw.hasPrefix("## \(version) ")
                continue
            }
            guard inSection, raw.trimmingCharacters(in: .whitespaces).hasPrefix("- ") else { continue }
            lines.append(String(raw.trimmingCharacters(in: .whitespaces).dropFirst(2)))
        }
        return lines
    }

    static func bundledChangelog() -> String {
        guard let url = Bundle.main.url(forResource: "CHANGELOG", withExtension: "md"), let s = try? String(contentsOf: url, encoding: .utf8) else { return "" }
        return s
    }

    /// Zuletzt gezeigte Version. Weicht `Settings.version` davon ab, ist es ein Versionssprung.
    static var lastSeenVersion: String? {
        get { Profile.defaults.string(forKey: "whatsNew.lastVersion") }
        set { Profile.defaults.set(newValue, forKey: "whatsNew.lastVersion") }
    }
}
