import SwiftUI

enum Settings {
    static let startFolderKey = "startFolder"
    /// Startordner für ⌘N, Default Home. Immer mit abschließendem Slash.
    static var startFolder: String {
        get {
            var p = Profile.defaults.string(forKey: startFolderKey) ?? NSHomeDirectory()
            if p.hasPrefix("~") { p = NSHomeDirectory() + p.dropFirst() }
            return p.hasSuffix("/") ? p : p + "/"
        }
        set { Profile.defaults.set(newValue, forKey: startFolderKey) }
    }

    /// Kommando des externen Editors, so wie im Terminal getippt (`code`, `subl`, `zed`). "" = Hotkey tut nichts.
    static var editorCommand: String {
        get { Profile.defaults.string(forKey: "editorCommand") ?? "" }
        set { Profile.defaults.set(newValue, forKey: "editorCommand") }
    }

    /// Letzte Antwort von Claude als zweite Zeile unter jeder Session im Baum.
    static var showLastMessage: Bool {
        get { Profile.defaults.bool(forKey: "showLastMessage") }
        set { Profile.defaults.set(newValue, forKey: "showLastMessage") }
    }

    /// Farbschema der Oberfläche und Terminals, Default Catppuccin Mocha.
    static var colorTheme: String {
        get { Profile.defaults.string(forKey: "colorTheme") ?? ColorTheme.all[0].id }
        set { Profile.defaults.set(newValue, forKey: "colorTheme") }
    }

    /// Aussehen des Baums, Default getönte Gruppen.
    static var sidebarStyle: SidebarStyle {
        get { Profile.defaults.string(forKey: "sidebar.style").flatMap(SidebarStyle.init) ?? .tinted }
        set { Profile.defaults.set(newValue.rawValue, forKey: "sidebar.style") }
    }

    /// Sortierung des Baums (Leiste), Default aus = Handreihenfolge.
    static var sidebarSort: SidebarSort {
        get { Profile.defaults.string(forKey: "sidebar.sort").flatMap(SidebarSort.init) ?? .off }
        set { Profile.defaults.set(newValue.rawValue, forKey: "sidebar.sort") }
    }

    /// Laufzeit („12m“) in jeder Session-Zeile des Baums, Default an.
    static var sidebarShowAge: Bool {
        get { Profile.defaults.object(forKey: "sidebar.showAge") as? Bool ?? true }
        set { Profile.defaults.set(newValue, forKey: "sidebar.showAge") }
    }

    /// Welche Sounds Kadrell spielt, Default alle.
    static var sounds: Feedback.Level {
        get { Profile.defaults.string(forKey: "sounds").flatMap(Feedback.Level.init) ?? .all }
        set { Profile.defaults.set(newValue.rawValue, forKey: "sounds") }
    }

    /// Stack-Zeilen zeigen zusätzlich den Pfad der Session, Default an.
    static var stackShowPath: Bool {
        get { Profile.defaults.object(forKey: "stackShowPath") as? Bool ?? true }
        set { Profile.defaults.set(newValue, forKey: "stackShowPath") }
    }

    /// Beendet sich Claude selbst (zweimal ⌃C, `/exit`), verschwindet die Kachel. Default aus: sie bleibt, Klick setzt fort.
    static var closeTileOnExit: Bool {
        get { Profile.defaults.bool(forKey: "closeTileOnExit") }
        set { Profile.defaults.set(newValue, forKey: "closeTileOnExit") }
    }

    /// Auto-Modus zeigt aus der Auswahl nur Sessions in diesen Zuständen. Default: nur wartende.
    static var autoWaiting: Bool {
        get { Profile.defaults.object(forKey: "auto.waiting") as? Bool ?? true }
        set { Profile.defaults.set(newValue, forKey: "auto.waiting") }
    }
    /// Auto-Modus filtert alle Sessions des Baums statt nur der Auswahl. Default an.
    static var autoAllSessions: Bool {
        get { Profile.defaults.object(forKey: "auto.allSessions") as? Bool ?? true }
        set { Profile.defaults.set(newValue, forKey: "auto.allSessions") }
    }
    static var autoRunning: Bool {
        get { Profile.defaults.bool(forKey: "auto.running") }
        set { Profile.defaults.set(newValue, forKey: "auto.running") }
    }

    /// Start-Flags für Claude, gelten ab dem nächsten Start eines Claude-Prozesses. "" = Claude-Default.
    static let claudeModes = ["", "acceptEdits", "auto", "plan", "dontAsk", "bypassPermissions"]
    static let claudeModels = ["", "fable", "opus", "sonnet"]
    static let claudeEfforts = ["", "low", "medium", "high", "xhigh", "max"]
    static var claudeAllowBypass: Bool {
        get { Profile.defaults.bool(forKey: "claude.allowBypass") }
        set { Profile.defaults.set(newValue, forKey: "claude.allowBypass") }
    }
    /// `kadrell` aus einer Session heraus darf Sessions anderer Gruppen lesen und steuern (send, capture, kill …).
    /// Default an: Agenten, die andere Sessions steuern, sollen ohne Umweg laufen. Aus: nur die eigene Gruppe.
    static var controlOtherSessions: Bool {
        get { Profile.defaults.object(forKey: "control.otherSessions") as? Bool ?? true }
        set { Profile.defaults.set(newValue, forKey: "control.otherSessions") }
    }
    static var claudeMode: String {
        get { Profile.defaults.string(forKey: "claude.mode") ?? "" }
        set { Profile.defaults.set(newValue, forKey: "claude.mode") }
    }
    static var claudeModel: String {
        get { Profile.defaults.string(forKey: "claude.model") ?? "" }
        set { Profile.defaults.set(newValue, forKey: "claude.model") }
    }
    static var claudeEffort: String {
        get { Profile.defaults.string(forKey: "claude.effort") ?? "" }
        set { Profile.defaults.set(newValue, forKey: "claude.effort") }
    }

    /// Bereiche der Schieberegler, Prozentwerte in Prozent (gespeichert als Faktor).
    static let uiScalePercent = 50.0...200.0
    static let lineSpacingPercent = 50.0...300.0
    static let pixelRange = 0.0...64.0
    static let fontSizes = 6.0...72.0
    static let defaultFontSize = 12.0
    static let defaultFontName = "JetBrainsMonoNF-Regular"

    private static func double(_ key: String, _ fallback: Double) -> Double {
        Profile.defaults.object(forKey: key) as? Double ?? fallback
    }

    static var uiScale: Double {
        get { double("uiScale", 1) }
        set { Profile.defaults.set(newValue, forKey: "uiScale") }
    }
    static var terminalFontSize: Double {
        get { min(max(double("terminal.fontSize", defaultFontSize), fontSizes.lowerBound), fontSizes.upperBound) }
        set { Profile.defaults.set(min(max(newValue, fontSizes.lowerBound), fontSizes.upperBound), forKey: "terminal.fontSize") }
    }
    static var terminalFontName: String {
        get { Profile.defaults.string(forKey: "terminal.fontName") ?? defaultFontName }
        set { Profile.defaults.set(newValue, forKey: "terminal.fontName") }
    }
    static var terminalLineSpacing: Double {
        get { double("terminal.lineSpacing", 1) }
        set { Profile.defaults.set(newValue, forKey: "terminal.lineSpacing") }
    }
    /// Abstand zwischen Kachelrahmen und Terminaltext, zusätzlich zu den 2 px Rahmenschutz.
    static var terminalPadding: Double {
        get { double("terminal.padding", 0) }
        set { Profile.defaults.set(newValue, forKey: "terminal.padding") }
    }

    /// Bild hinter den Kacheln, nur in der Arbeitsfläche. "" = keins.
    static var backgroundImage: String {
        get { Profile.defaults.string(forKey: "workspace.backgroundImage") ?? "" }
        set { Profile.defaults.set(newValue, forKey: "workspace.backgroundImage") }
    }
    /// Deckkraft des Kachelkörpers samt Terminal-Hintergrund; Text bleibt voll sichtbar. 1 = undurchsichtig.
    static var tileOpacity: Double {
        get { double("tiles.opacity", 1) }
        set { Profile.defaults.set(newValue, forKey: "tiles.opacity") }
    }

    /// Feste Spaltenzahl im Grid (Leiste), 0 = automatisch ⌈√n⌉.
    static var gridColumns: Int {
        get { Profile.defaults.integer(forKey: "layout.grid.columns") }
        set { Profile.defaults.set(newValue, forKey: "layout.grid.columns") }
    }

    /// Layout Frei: Teilungsrichtung je Position, „r“ rechts, „d“ unten, „a“ längere Seite. Bleibt beim Zurücksetzen der Trennlinien.
    static var customSplits: String {
        get { Profile.defaults.string(forKey: "workspace.custom.splits") ?? "" }
        set { Profile.defaults.set(newValue, forKey: "workspace.custom.splits") }
    }

    /// Gezogene Verhältnisse einer Layout-Vorlage (`Tiling.layout`), nil = gleich verteilt.
    static func layoutRatios(_ key: String, _ count: Int) -> [Double]? { Profile.defaults.array(forKey: "layout." + key) as? [Double] }
    static func setLayoutRatios(_ key: String, _ value: [Double]?) { Profile.defaults.set(value, forKey: "layout." + key) }
    /// Alle gezogenen Verhältnisse weg, Spaltenzahl bleibt.
    static func resetLayoutRatios() {
        for k in Profile.defaults.dictionaryRepresentation().keys where k.hasPrefix("layout.") && k != "layout.grid.columns" { Profile.defaults.removeObject(forKey: k) }
    }

    /// Abstand zwischen den Kacheln und zum Rand der Arbeitsfläche.
    static var tileGap: Double {
        get { double("tiles.gap", 6) }
        set { Profile.defaults.set(newValue, forKey: "tiles.gap") }
    }

    /// SwiftTerm zeichnet per Metal auf der GPU statt per CoreGraphics auf dem Main-Thread, Default an.
    static var terminalMetal: Bool {
        get { Profile.defaults.object(forKey: "terminal.metal") as? Bool ?? true }
        set { Profile.defaults.set(newValue, forKey: "terminal.metal") }
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
            get { Profile.defaults.object(forKey: key) as? Bool ?? true }
            nonmutating set { Profile.defaults.set(newValue, forKey: key) }
        }
    }

    static var version: String {
        (Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String) ?? "0"
    }
}

@Observable
@MainActor
final class SettingsModel {
    var startFolder = Settings.startFolder
    var editorCommand = Settings.editorCommand
    var showLastMessage = Settings.showLastMessage
    var stackShowPath = Settings.stackShowPath
    var sidebarStyle = Settings.sidebarStyle
    var sidebarShowAge = Settings.sidebarShowAge
    var sounds = Settings.sounds
    var closeTileOnExit = Settings.closeTileOnExit
    var autoWaiting = Settings.autoWaiting
    var autoRunning = Settings.autoRunning
    var autoAllSessions = Settings.autoAllSessions
    var claudeAllowBypass = Settings.claudeAllowBypass
    var controlOtherSessions = Settings.controlOtherSessions
    var claudeMode = Settings.claudeMode
    var claudeModel = Settings.claudeModel
    var claudeEffort = Settings.claudeEffort
    var hotkeys = Hotkeys.current
    var uiScale = Settings.uiScale
    var colorTheme = Settings.colorTheme
    var fontName = Settings.terminalFontName
    var fontSize = Settings.terminalFontSize
    var lineSpacing = Settings.terminalLineSpacing
    var padding = Settings.terminalPadding
    var tileGap = Settings.tileGap
    var backgroundImage = Settings.backgroundImage
    var tileOpacity = Settings.tileOpacity
    var metal = Settings.terminalMetal
    var ask = Dictionary(uniqueKeysWithValues: Settings.Ask.allCases.map { ($0, $0.enabled) })
    @ObservationIgnored lazy var fonts: [(name: String, display: String)] = {
        let list = Settings.monospaceFonts
        return list.contains { $0.name == fontName } ? list : [(fontName, fontName)] + list
    }()
    /// Aktion, deren Kürzel gerade aufgenommen wird.
    var recording: HotkeyAction?
    /// Nach jeder Änderung: die App zieht Menü, Aussehen und Sessions nach.
    @ObservationIgnored var onApply: (() -> Void)?
    @ObservationIgnored var onClose: (() -> Void)?
    @ObservationIgnored private var monitor: Any?

    init() { track(notify: false) }

    /// Jede Änderung gilt sofort, ohne Speichern: `write` liest alle Werte, jede Änderung daran schreibt neu.
    private func track(notify: Bool) {
        withObservationTracking { write() } onChange: { [weak self] in
            Task { @MainActor in self?.track(notify: true) }
        }
        if notify { onApply?() }
    }

    private func write() {
        let p = startFolder.trimmingCharacters(in: .whitespacesAndNewlines)
        if !p.isEmpty { Settings.startFolder = p }
        Settings.editorCommand = editorCommand.trimmingCharacters(in: .whitespacesAndNewlines)
        Settings.showLastMessage = showLastMessage
        Settings.stackShowPath = stackShowPath
        Settings.sidebarStyle = sidebarStyle
        Settings.sidebarShowAge = sidebarShowAge
        Settings.sounds = sounds
        Settings.closeTileOnExit = closeTileOnExit
        Settings.autoWaiting = autoWaiting
        Settings.autoRunning = autoRunning
        Settings.autoAllSessions = autoAllSessions
        Settings.claudeAllowBypass = claudeAllowBypass
        Settings.controlOtherSessions = controlOtherSessions
        Settings.claudeMode = claudeMode
        Settings.claudeModel = claudeModel
        Settings.claudeEffort = claudeEffort
        Hotkeys.current = hotkeys
        Settings.uiScale = uiScale
        Settings.colorTheme = colorTheme
        Settings.terminalFontName = fontName
        Settings.terminalFontSize = fontSize
        Settings.terminalLineSpacing = lineSpacing
        Settings.terminalPadding = padding
        Settings.tileGap = tileGap
        Settings.backgroundImage = backgroundImage.trimmingCharacters(in: .whitespacesAndNewlines)
        Settings.tileOpacity = tileOpacity
        Settings.terminalMetal = metal
        for (a, on) in ask { a.enabled = on }
    }

    /// Nächster Tastendruck wird das Kürzel. Esc bricht ab, ⌫ entfernt es. Läuft vor OverlayPanel und Terminal.
    func record(_ a: HotkeyAction) {
        stopRecording()
        recording = a
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, let a = self.recording else { return event }
            let plain = event.modifierFlags.intersection(Hotkey.modMask).isEmpty
            if plain, event.keyCode == 53 { self.stopRecording(); return nil }
            if plain, event.keyCode == 51 { self.hotkeys[a] = nil; self.stopRecording(); return nil }
            guard let k = Hotkey(event: event), k.isUsable else { NSSound.beep(); return nil }
            for (other, v) in self.hotkeys where v == k { self.hotkeys[other] = nil }   // doppelt vergeben geht nicht
            self.hotkeys[a] = k
            self.stopRecording()
            return nil
        }
    }

    func stopRecording() {
        if let m = monitor { NSEvent.removeMonitor(m) }
        monitor = nil
        recording = nil
    }
}

/// ⌘, Einstellungen. ⏎ speichert, Esc bricht ab.
struct SettingsView: View {
    @Bindable var model: SettingsModel

    /// Bereichsüberschrift wie in der F1-Hilfe: Akzentfarbe, gesperrt, Großbuchstaben, optionaler Hinweis daneben.
    private func heading(_ s: String, note: String = "", first: Bool = false) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(s.uppercased()).kerning(0.6).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.runningColor)
            if !note.isEmpty { Text(note).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor) }
        }
        .padding(.horizontal, 16).padding(.top, first ? 14 : 18).padding(.bottom, 6)
    }

    /// Kürzel nach denselben Bereichen wie die F1-Hilfe; die zweite Gruppe nimmt den Rest, damit keine Aktion verschwindet.
    private var hotkeyGroups: [(String, [HotkeyAction])] {
        let nav: [HotkeyAction] = [.focusLeft, .focusRight, .focusUp, .focusDown, .nextSession, .prevSession, .lastSession, .previewNext, .previewPrev, .nextWaiting]
            + HotkeyAction.allCases.filter { $0.tileIndex != nil } + [.focusSidebar, .focusWorkspace]
        return [(String(localized: "Navigation"), nav), (String(localized: "Kacheln verwalten"), HotkeyAction.allCases.filter { !nav.contains($0) })]
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("EINSTELLUNGEN").kerning(0.6)
                Spacer()
                Text("Kadrell v\(Settings.version)")
            }
            .font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
            .padding(.horizontal, 16).padding(.top, 12)
            Divider().overlay(Theme.lineColor).padding(.top, 12)

            heading(String(localized: "Sessions"), first: true)
            table {
                setting(String(localized: "Projektordner für ⌘N (nach Git-Repos durchsucht)")) {
                    HStack(spacing: 4) {
                        PathField(text: Binding(get: { Theme.shortPath(model.startFolder) }, set: { model.startFolder = $0 }), autofocus: false,
                                  onTab: { if let p = FolderIndex.expandAbbreviated(model.startFolder).first { model.startFolder = p } },
                                  onSubmit: {}, onMove: { _ in })
                            .frame(height: 18 * Theme.scale).padding(.horizontal, 8).padding(.vertical, 2)
                            .background(Theme.bgColor)
                            .dropDestination(for: URL.self) { urls, _ in
                                guard let d = urls.lazy.compactMap({ FolderIndex.folder(for: $0.path) }).first else { return false }
                                model.startFolder = d
                                return true
                            }
                        Button { chooseFolder(start: model.startFolder) { if let p = $0 { model.startFolder = p } } } label: {
                            Image(systemName: "folder").foregroundStyle(Theme.mutedColor).frame(width: 22 * Theme.scale, height: 22 * Theme.scale)
                        }
                        .buttonStyle(.plain).help("Im Finder wählen")
                    }
                }
                setting(String(localized: "Externer Editor (Kommando wie im Terminal)")) {
                    TextField("z. B. code", text: $model.editorCommand)
                        .textFieldStyle(.plain).foregroundStyle(Theme.fgColor)
                        .padding(.horizontal, 8).padding(.vertical, 3).background(Theme.bgColor)
                }
                setting(String(localized: "Wenn Claude endet (zweimal ⌃C, /exit)")) {
                    menu($model.closeTileOnExit, [(false, String(localized: "Kachel bleibt, Klick setzt fort")), (true, String(localized: "Kachel schließen"))])
                }
            }

            heading(String(localized: "Claude"), note: String(localized: "gilt für neu gestartete Claude-Prozesse"))
            table {
                setting(String(localized: "Bypass-Modus erlauben (--allow-dangerously-skip-permissions)")) { onOff($model.claudeAllowBypass) }
                setting(String(localized: "Startmodus")) { options(Settings.claudeModes, $model.claudeMode) }
                setting(String(localized: "Modell")) { options(Settings.claudeModels, $model.claudeModel) }
                setting(String(localized: "Effort")) { options(Settings.claudeEfforts, $model.claudeEffort) }
                setting(String(localized: "Sessions dürfen andere Sessions steuern (kadrell send, capture, kill)")) { onOff($model.controlOtherSessions) }
            }

            heading(String(localized: "Darstellung"))
            table {
                setting(String(localized: "Farbschema")) { menu($model.colorTheme, ColorTheme.all.map { ($0.id, $0.name) }) }
                setting(String(localized: "UI-Größe")) { slider($model.uiScale, Settings.uiScalePercent, step: 5, factor: 100, unit: "%", live: false) }
                setting(String(localized: "Terminal-Schrift")) { menu($model.fontName, model.fonts.map { ($0.name, $0.display) }) }
                setting(String(localized: "Terminal-Schriftgröße  ⌘+ ⌘- ⌘0  ⌘ Mausrad")) { slider($model.fontSize, Settings.fontSizes, step: 1, unit: "pt") }
                setting(String(localized: "Zeilenabstand")) { slider($model.lineSpacing, Settings.lineSpacingPercent, step: 5, factor: 100, unit: "%") }
                setting(String(localized: "Innenabstand der Kacheln")) { slider($model.padding, Settings.pixelRange, step: 1, unit: "px") }
                setting(String(localized: "Abstand zwischen Kacheln")) { slider($model.tileGap, Settings.pixelRange, step: 1, unit: "px") }
                setting(String(localized: "Terminal auf der GPU zeichnen (Metal)")) { onOff($model.metal) }
            }

            heading(String(localized: "Anpassen"), note: String(localized: "Bild nur hinter den Kacheln"))
            table {
                setting(String(localized: "Hintergrundbild")) {
                    HStack(spacing: 4) {
                        PathField(text: Binding(get: { Theme.shortPath(model.backgroundImage) }, set: { model.backgroundImage = FolderIndex.normalize($0) }),
                                  placeholder: String(localized: "kein Bild · Datei hineinziehen"), autofocus: false, onTab: {}, onSubmit: {}, onMove: { _ in })
                            .frame(height: 18 * Theme.scale).padding(.horizontal, 8).padding(.vertical, 2)
                            .background(Theme.bgColor)
                            .dropDestination(for: URL.self) { urls, _ in
                                guard let u = urls.first(where: { NSImage(contentsOf: $0) != nil }) else { return false }
                                model.backgroundImage = u.path
                                return true
                            }
                        Button { chooseFolder(start: model.backgroundImage.isEmpty ? "~/Pictures" : (model.backgroundImage as NSString).deletingLastPathComponent, images: true) { if let p = $0 { model.backgroundImage = p } } } label: {
                            Image(systemName: "photo").foregroundStyle(Theme.mutedColor).frame(width: 22 * Theme.scale, height: 22 * Theme.scale)
                        }
                        .buttonStyle(.plain).help("Bild wählen")
                        Button { model.backgroundImage = "" } label: {
                            Image(systemName: "xmark").foregroundStyle(Theme.mutedColor).frame(width: 22 * Theme.scale, height: 22 * Theme.scale)
                        }
                        .buttonStyle(.plain).help("Kein Bild")
                    }
                }
                setting(String(localized: "Deckkraft der Kacheln")) { slider($model.tileOpacity, 0...100, step: 1, factor: 100, unit: "%") }
            }

            heading(String(localized: "Baum und Kacheln"))
            table {
                setting(String(localized: "Design des Baums")) { menu($model.sidebarStyle, SidebarStyle.allCases.map { ($0, $0.title) }) }
                setting(String(localized: "Laufzeit im Baum")) { onOff($model.sidebarShowAge) }
                setting(String(localized: "Pfad in Stack-Zeilen")) { onOff($model.stackShowPath) }
                setting(String(localized: "Letzte Antwort von Claude im Baum")) { onOff($model.showLastMessage) }
                setting(String(localized: "Sounds")) { menu($model.sounds, Feedback.Level.allCases.map { ($0, $0.title) }) }
            }

            heading(String(localized: "Auto-Modus"), note: String(localized: "AUTO in der Leiste"))
            table {
                setting(String(localized: "Gilt für")) { menu($model.autoAllSessions, [(true, String(localized: "alle Sessions")), (false, String(localized: "nur die Auswahl im Baum"))]) }
                setting(String(localized: "Zeigt")) {
                    menu(Binding(get: { AutoShow(waiting: model.autoWaiting, running: model.autoRunning) },
                                 set: { model.autoWaiting = $0.waiting; model.autoRunning = $0.running }),
                         [(AutoShow(waiting: true, running: false), String(localized: "wartende")), (AutoShow(waiting: false, running: true), String(localized: "arbeitende")),
                          (AutoShow(waiting: true, running: true), String(localized: "wartende und arbeitende")), (AutoShow(waiting: false, running: false), String(localized: "keine"))])
                }
            }

            heading(String(localized: "Rückfragen"), note: String(localized: "aus = ohne Nachfrage ausführen"))
            table {
                ForEach(Settings.Ask.allCases, id: \.self) { a in
                    setting(a.title) { onOff(Binding(get: { model.ask[a] ?? true }, set: { model.ask[a] = $0 })) }
                }
            }

            heading(String(localized: "Tastenkürzel"), note: String(localized: "Kürzel anklicken, Tasten drücken · ⌫ entfernt"))
            ForEach(hotkeyGroups, id: \.0) { title, actions in
                Text(title).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.mutedColor)
                    .padding(.horizontal, 16).padding(.top, title == hotkeyGroups.first?.0 ? 0 : 12).padding(.bottom, 4)
                table { ForEach(actions, id: \.self) { a in row(a) } }
            }
            Color.clear.frame(height: 12)
            DialogFoot(hint: String(localized: "Änderungen gelten sofort · Esc schließt"), button: String(localized: "Fertig")) { model.stopRecording(); model.onClose?() }
        }
        .frame(width: 900 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
        .onDisappear { model.stopRecording() }
    }

    /// Breite der rechten Spalte: jedes Dropdown, Feld und Kürzel ist gleich breit, die Einstellungen lesen sich wie eine Tabelle.
    private var controlWidth: CGFloat { 300 * Theme.scale }

    /// Zeilen einer Tabelle mit feiner Linie dazwischen.
    private func table<C: View>(@ViewBuilder _ rows: () -> C) -> some View {
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

    private func setting<C: View>(_ title: String, @ViewBuilder _ control: () -> C) -> some View {
        HStack(spacing: 12) {
            Text(title).foregroundStyle(Theme.fgColor).lineLimit(1)
            Spacer(minLength: 12)
            control().frame(width: controlWidth)
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }

    /// Dropdown im App-Stil, füllt die rechte Spalte. Das native Popup nimmt nur die Breite seines Textes.
    private func menu<T: Hashable>(_ selection: Binding<T>, _ items: [(T, String)]) -> some View {
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
            .padding(.horizontal, 8).padding(.vertical, 3)
            .frame(width: controlWidth).background(Theme.bgColor).contentShape(Rectangle())
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden)
        .frame(width: controlWidth)
    }

    /// Ganzzahliger Schieberegler mit Wert rechts daneben. `factor` rechnet gespeicherte Faktoren in Prozent um.
    /// `live: false` übernimmt erst beim Loslassen (UI-Größe: sonst skaliert der Dialog unter der Maus mit).
    private func slider(_ value: Binding<Double>, _ range: ClosedRange<Double>, step: Double, factor: Double = 1, unit: String, live: Bool = true) -> some View {
        IntSlider(value: value, range: range, step: step, factor: factor, unit: unit, live: live)
    }

    private func onOff(_ value: Binding<Bool>) -> some View { menu(value, [(true, String(localized: "an")), (false, String(localized: "aus"))]) }

    /// Texte als Dropdown. "" heißt Claude-Default.
    private func options(_ values: [String], _ value: Binding<String>) -> some View {
        menu(value, values.map { ($0, $0.isEmpty ? String(localized: "Standard") : $0) })
    }

    private func row(_ a: HotkeyAction) -> some View {
        let key = model.hotkeys[a]
        let isRecording = model.recording == a
        return HStack(spacing: 8) {
            Text(a.title).foregroundStyle(Theme.fgColor).lineLimit(1)
            Spacer(minLength: 12)
            if key != a.defaultKey, !isRecording {
                Button { model.hotkeys[a] = a.defaultKey } label: { Text("Standard").foregroundStyle(Theme.mutedColor) }
                    .buttonStyle(.plain).help("Zurück auf \(a.defaultKey.display)")
            }
            Button { isRecording ? model.stopRecording() : model.record(a) } label: {
                Text(isRecording ? String(localized: "Tasten drücken …") : key?.display ?? "–")
                    .font(Theme.ui(12, bold: true))
                    .foregroundStyle(isRecording ? Theme.bgColor : Theme.fgColor)
                    .padding(.horizontal, 8).padding(.vertical, 3).frame(width: controlWidth, alignment: .leading)
                    .background(isRecording ? Theme.runningColor : Theme.bgColor)
            }
            .buttonStyle(.plain)
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }
}

private struct IntSlider: View {
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
private struct AutoShow: Hashable {
    let waiting: Bool
    let running: Bool
}
