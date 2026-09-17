import AppKit

enum Settings {
    private static func value<T>(_ key: String, _ fallback: @autoclosure () -> T) -> T { Profile.defaults.object(forKey: key) as? T ?? fallback() }
    private static func enumValue<T: RawRepresentable>(_ key: String, _ fallback: T) -> T where T.RawValue == String {
        Profile.defaults.string(forKey: key).flatMap { T(rawValue: $0) } ?? fallback
    }
    private static func store(_ key: String, _ value: Any?) { Profile.defaults.set(value, forKey: key) }

    static let startFolderKey = "startFolder"
    /// Startordner für ⌘N. Immer mit abschließendem Slash.
    static var startFolder: String {
        get {
            var p = value(startFolderKey, defaultStartFolder)
            if p.hasPrefix("~") { p = NSHomeDirectory() + p.dropFirst() }
            return p.hasSuffix("/") ? p : p + "/"
        }
        set { store(startFolderKey, newValue) }
    }

    /// Erster existierender Kandidat aus den üblichen Projektordnern, sonst Home. `~` selbst als Startordner scannt
    /// beim ersten ⌘N auch Schreibtisch/Dokumente/Downloads an und löst eine Kaskade an TCC-Abfragen aus (#23).
    private static var defaultStartFolder: String {
        let home = NSHomeDirectory()
        let candidates = ["development", "Development", "Projects", "projects", "code", "Code", "src",
                           "workspace", "git", "Documents/GitHub"]
        return candidates.map { home + "/" + $0 }.first { FolderIndex.isDirectory($0) } ?? home
    }

    /// Kommando des externen Editors, so wie im Terminal getippt (`code`, `subl`, `zed`). "" = Hotkey tut nichts.
    static var editorCommand: String { get { value("editorCommand", "") } set { store("editorCommand", newValue) } }
    /// Letzte Antwort von Claude als zweite Zeile unter jeder Session im Baum.
    static var showLastMessage: Bool { get { value("showLastMessage", false) } set { store("showLastMessage", newValue) } }
    /// Farbschema der Oberfläche und Terminals, Default Catppuccin Mocha.
    static var colorTheme: String { get { value("colorTheme", ColorTheme.all[0].id) } set { store("colorTheme", newValue) } }
    /// Aussehen des Baums, Default getönte Gruppen.
    static var sidebarStyle: SidebarStyle { get { enumValue("sidebar.style", .tinted) } set { store("sidebar.style", newValue.rawValue) } }
    /// Sortierung des Baums (Leiste), Default aus = Handreihenfolge.
    static var sidebarSort: SidebarSort { get { enumValue("sidebar.sort", .off) } set { store("sidebar.sort", newValue.rawValue) } }
    /// Laufzeit („12m“) in jeder Session-Zeile des Baums, Default an.
    static var sidebarShowAge: Bool { get { value("sidebar.showAge", true) } set { store("sidebar.showAge", newValue) } }
    /// Welche Sounds Kadrell spielt, Default alle.
    static var sounds: Feedback.Level { get { enumValue("sounds", .all) } set { store("sounds", newValue.rawValue) } }
    /// Systembenachrichtigungen bei wartenden/fertigen Sessions, Default nur wartet.
    static var notifications: Notifications.Level { get { enumValue("notifications", .waiting) } set { store("notifications", newValue.rawValue) } }
    /// Beim Start und danach alle 24 h ohne Tracking-Parameter auf eine neuere Version prüfen, Default an.
    static var checkForUpdates: Bool { get { value("checkForUpdates", true) } set { store("checkForUpdates", newValue) } }
    /// Stack-Zeilen zeigen zusätzlich den Pfad der Session, Default an.
    static var stackShowPath: Bool { get { value("stackShowPath", true) } set { store("stackShowPath", newValue) } }
    /// Beendet sich Claude selbst (zweimal ⌃C, `/exit`), verschwindet die Kachel. Default aus: sie bleibt, Klick setzt fort.
    static var closeTileOnExit: Bool { get { value("closeTileOnExit", false) } set { store("closeTileOnExit", newValue) } }

    /// Auto-Modus zeigt aus der Auswahl nur Sessions in diesen Zuständen. Default: nur wartende.
    static var autoWaiting: Bool { get { value("auto.waiting", true) } set { store("auto.waiting", newValue) } }
    static var autoRunning: Bool { get { value("auto.running", false) } set { store("auto.running", newValue) } }
    /// Auto-Modus filtert alle Sessions des Baums statt nur der Auswahl. Default an.
    static var autoAllSessions: Bool { get { value("auto.allSessions", true) } set { store("auto.allSessions", newValue) } }

    /// Start-Flags für Claude, gelten ab dem nächsten Start eines Claude-Prozesses. "" = Claude-Default.
    static let claudeModes = ["", "acceptEdits", "auto", "plan", "dontAsk", "bypassPermissions"]
    static let claudeModels = ["", "fable", "opus", "sonnet"]
    static let claudeEfforts = ["", "low", "medium", "high", "xhigh", "max"]
    static var claudeAllowBypass: Bool { get { value("claude.allowBypass", false) } set { store("claude.allowBypass", newValue) } }
    static var claudeMode: String { get { value("claude.mode", "") } set { store("claude.mode", newValue) } }
    static var claudeModel: String { get { value("claude.model", "") } set { store("claude.model", newValue) } }
    static var claudeEffort: String { get { value("claude.effort", "") } set { store("claude.effort", newValue) } }
    /// `kadrell` aus einer Session heraus darf Sessions anderer Gruppen lesen und steuern (send, capture, kill …).
    /// Default an: Agenten, die andere Sessions steuern, sollen ohne Umweg laufen. Aus: nur die eigene Gruppe.
    static var controlOtherSessions: Bool { get { value("control.otherSessions", true) } set { store("control.otherSessions", newValue) } }
    /// Kompakte Zeile für den ⌘N-Dialog: mit welchen Start-Flags eine neue Session gleich läuft (Kanboard #75).
    static var claudeSummary: String {
        var parts = [claudeModel.isEmpty ? String(localized: "Standardmodell") : claudeModel,
                     claudeMode.isEmpty ? String(localized: "fragt nach Rechten") : claudeMode]
        if !claudeEffort.isEmpty { parts.append(claudeEffort) }
        if claudeAllowBypass { parts.append(String(localized: "Bypass")) }
        return parts.joined(separator: " · ")
    }

    /// Bereiche der Schieberegler, Prozentwerte in Prozent (gespeichert als Faktor).
    static let uiScalePercent = 50.0...200.0
    static let lineSpacingPercent = 50.0...300.0
    static let pixelRange = 0.0...64.0
    static let fontSizes = 6.0...72.0
    static let defaultFontSize = 12.0
    static let defaultFontName = "JetBrainsMonoNF-Regular"
    /// Zeilen Verlauf pro Terminal (SwiftTerm hält sonst nur 500). Kostet grob 1 MB je 1000 Zeilen bei breiten Kacheln.
    static let scrollbackRange = 1_000.0...50_000.0

    static var uiScale: Double { get { value("uiScale", 1.0) } set { store("uiScale", newValue) } }
    static var terminalFontSize: Double {
        get { min(max(value("terminal.fontSize", defaultFontSize), fontSizes.lowerBound), fontSizes.upperBound) }
        set { store("terminal.fontSize", min(max(newValue, fontSizes.lowerBound), fontSizes.upperBound)) }
    }
    static var terminalFontName: String { get { value("terminal.fontName", defaultFontName) } set { store("terminal.fontName", newValue) } }
    static var terminalLineSpacing: Double { get { value("terminal.lineSpacing", 1.0) } set { store("terminal.lineSpacing", newValue) } }
    static var terminalScrollback: Int {
        get { Int(min(max(value("terminal.scrollback", 10_000.0), scrollbackRange.lowerBound), scrollbackRange.upperBound)) }
        set { store("terminal.scrollback", Double(newValue)) }
    }
    /// Abstand zwischen Kachelrahmen und Terminaltext, zusätzlich zu den 2 px Rahmenschutz.
    static var terminalPadding: Double { get { value("terminal.padding", 0.0) } set { store("terminal.padding", newValue) } }
    /// SwiftTerm zeichnet per Metal auf der GPU statt per CoreGraphics auf dem Main-Thread, Default an.
    static var terminalMetal: Bool { get { value("terminal.metal", true) } set { store("terminal.metal", newValue) } }

    /// Bild hinter den Kacheln, nur in der Arbeitsfläche. "" = keins.
    static var backgroundImage: String { get { value("workspace.backgroundImage", "") } set { store("workspace.backgroundImage", newValue) } }
    /// Deckkraft des Kachelkörpers samt Terminal-Hintergrund; Text bleibt voll sichtbar. 1 = undurchsichtig.
    static var tileOpacity: Double { get { value("tiles.opacity", 1.0) } set { store("tiles.opacity", newValue) } }
    /// Abstand zwischen den Kacheln und zum Rand der Arbeitsfläche.
    static var tileGap: Double { get { value("tiles.gap", 6.0) } set { store("tiles.gap", newValue) } }

    /// Feste Spaltenzahl im Grid (Leiste), 0 = automatisch ⌈√n⌉.
    static var gridColumns: Int { get { value("layout.grid.columns", 0) } set { store("layout.grid.columns", newValue) } }
    /// Layout Frei: Teilungsrichtung je Position, „r“ rechts, „d“ unten, „a“ längere Seite. Bleibt beim Zurücksetzen der Trennlinien.
    static var customSplits: String { get { value("workspace.custom.splits", "") } set { store("workspace.custom.splits", newValue) } }

    /// Gezogene Verhältnisse einer Layout-Vorlage (`Tiling.layout`), nil = gleich verteilt.
    static func layoutRatios(_ key: String, _ count: Int) -> [Double]? { Profile.defaults.array(forKey: "layout." + key) as? [Double] }
    static func setLayoutRatios(_ key: String, _ value: [Double]?) { store("layout." + key, value) }
    /// Alle gezogenen Verhältnisse weg, Spaltenzahl bleibt.
    static func resetLayoutRatios() {
        for k in Profile.defaults.dictionaryRepresentation().keys where k.hasPrefix("layout.") && k != "layout.grid.columns" { Profile.defaults.removeObject(forKey: k) }
    }

    static var terminalFont: NSFont {
        let size = CGFloat(terminalFontSize)
        return NSFont(name: terminalFontName, size: size) ?? Theme.font(size)
    }

    /// Installierte Monospace-Schriften, nur der normale Schnitt jeder Familie.
    static var monospaceFonts: [(name: String, display: String)] {
        let fm = NSFontManager.shared
        var out: [(String, String)] = []
        for family in fm.availableFontFamilies {
            guard let members = fm.availableMembers(ofFontFamily: family) else { continue }
            // Einträge: [PostScript-Name, Schnitt, Gewicht, Traits]
            let regular = members.first { ($0[3] as? UInt).map { NSFontTraitMask(rawValue: $0).contains(.fixedPitchFontMask) } == true
                && ($0[1] as? String) == "Regular" }
            if let name = regular?[0] as? String { out.append((name, family)) }
        }
        return out
    }

    /// Rückfragen, die ein Häkchen „Nicht mehr fragen“ abschalten kann. Abgeschaltet heißt: Aktion läuft sofort.
    enum Ask: String, CaseIterable {
        case closeSession, closeGroup, stopSession, quit, adoptBackground
        var title: String {
            switch self {
            case .closeSession: String(localized: "Session beenden und entfernen")
            case .closeGroup: String(localized: "Gruppe schließen")
            case .stopSession: String(localized: "Session stoppen")
            case .quit: String(localized: "Kadrell beenden, während Claude läuft")
            case .adoptBackground: String(localized: "Hintergrund-Sessions übernehmen")
            }
        }
        var key: String { "ask.\(rawValue)" }
        var enabled: Bool {
            get { Settings.value(key, true) }
            nonmutating set { Settings.store(key, newValue) }
        }
    }

    static var version: String {
        (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "0"
    }
}
