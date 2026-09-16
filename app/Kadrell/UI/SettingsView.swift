import SwiftUI

enum Settings {
    static let startFolderKey = "startFolder"
    /// Startordner für ⌘N, Default Home. Immer mit abschließendem Slash.
    static var startFolder: String {
        get {
            var p = UserDefaults.standard.string(forKey: startFolderKey) ?? NSHomeDirectory()
            if p.hasPrefix("~") { p = NSHomeDirectory() + p.dropFirst() }
            return p.hasSuffix("/") ? p : p + "/"
        }
        set { UserDefaults.standard.set(newValue, forKey: startFolderKey) }
    }

    /// Kommando des externen Editors, so wie im Terminal getippt (`code`, `subl`, `zed`). "" = Hotkey tut nichts.
    static var editorCommand: String {
        get { UserDefaults.standard.string(forKey: "editorCommand") ?? "" }
        set { UserDefaults.standard.set(newValue, forKey: "editorCommand") }
    }

    /// Letzte Antwort von Claude als zweite Zeile unter jeder Session im Baum.
    static var showLastMessage: Bool {
        get { UserDefaults.standard.bool(forKey: "showLastMessage") }
        set { UserDefaults.standard.set(newValue, forKey: "showLastMessage") }
    }

    /// Stack-Zeilen zeigen zusätzlich den Pfad der Session, Default an.
    static var stackShowPath: Bool {
        get { UserDefaults.standard.object(forKey: "stackShowPath") as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: "stackShowPath") }
    }

    /// Beendet sich Claude selbst (zweimal ⌃C, `/exit`), verschwindet die Kachel. Default aus: sie bleibt, Klick setzt fort.
    static var closeTileOnExit: Bool {
        get { UserDefaults.standard.bool(forKey: "closeTileOnExit") }
        set { UserDefaults.standard.set(newValue, forKey: "closeTileOnExit") }
    }

    /// Start-Flags für Claude, gelten ab dem nächsten Start eines Claude-Prozesses. "" = Claude-Default.
    static let claudeModes = ["", "acceptEdits", "auto", "plan", "dontAsk", "bypassPermissions"]
    static let claudeModels = ["", "fable", "opus", "sonnet"]
    static let claudeEfforts = ["", "low", "medium", "high", "xhigh", "max"]
    static var claudeAllowBypass: Bool {
        get { UserDefaults.standard.bool(forKey: "claude.allowBypass") }
        set { UserDefaults.standard.set(newValue, forKey: "claude.allowBypass") }
    }
    static var claudeMode: String {
        get { UserDefaults.standard.string(forKey: "claude.mode") ?? "" }
        set { UserDefaults.standard.set(newValue, forKey: "claude.mode") }
    }
    static var claudeModel: String {
        get { UserDefaults.standard.string(forKey: "claude.model") ?? "" }
        set { UserDefaults.standard.set(newValue, forKey: "claude.model") }
    }
    static var claudeEffort: String {
        get { UserDefaults.standard.string(forKey: "claude.effort") ?? "" }
        set { UserDefaults.standard.set(newValue, forKey: "claude.effort") }
    }

    static let uiScales: [Double] = [0.9, 1, 1.15, 1.3]
    static let lineSpacings: [Double] = [1, 1.1, 1.2, 1.35]
    static let paddings: [Double] = [0, 4, 8, 12]
    static let fontSizes = 9.0...28.0
    static let defaultFontSize = 12.0
    static let defaultFontName = "JetBrainsMonoNF-Regular"

    private static func double(_ key: String, _ fallback: Double) -> Double {
        UserDefaults.standard.object(forKey: key) as? Double ?? fallback
    }

    static var uiScale: Double {
        get { double("uiScale", 1) }
        set { UserDefaults.standard.set(newValue, forKey: "uiScale") }
    }
    static var terminalFontSize: Double {
        get { min(max(double("terminal.fontSize", defaultFontSize), fontSizes.lowerBound), fontSizes.upperBound) }
        set { UserDefaults.standard.set(min(max(newValue, fontSizes.lowerBound), fontSizes.upperBound), forKey: "terminal.fontSize") }
    }
    static var terminalFontName: String {
        get { UserDefaults.standard.string(forKey: "terminal.fontName") ?? defaultFontName }
        set { UserDefaults.standard.set(newValue, forKey: "terminal.fontName") }
    }
    static var terminalLineSpacing: Double {
        get { double("terminal.lineSpacing", 1) }
        set { UserDefaults.standard.set(newValue, forKey: "terminal.lineSpacing") }
    }
    /// Abstand zwischen Kachelrahmen und Terminaltext, zusätzlich zu den 2 px Rahmenschutz.
    static var terminalPadding: Double {
        get { double("terminal.padding", 0) }
        set { UserDefaults.standard.set(newValue, forKey: "terminal.padding") }
    }

    /// SwiftTerm zeichnet per Metal auf der GPU statt per CoreGraphics auf dem Main-Thread, Default an.
    static var terminalMetal: Bool {
        get { UserDefaults.standard.object(forKey: "terminal.metal") as? Bool ?? true }
        set { UserDefaults.standard.set(newValue, forKey: "terminal.metal") }
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
            case .closeSession: "Session beenden und entfernen"
            case .closeGroup: "Gruppe schließen"
            case .stopSession: "Session stoppen"
            case .quit: "Kadrell beenden, während Claude läuft"
            case .adoptBackground: "Hintergrund-Sessions übernehmen"
            }
        }
        var key: String { "ask.\(rawValue)" }
        var enabled: Bool {
            get { UserDefaults.standard.object(forKey: key) as? Bool ?? true }
            nonmutating set { UserDefaults.standard.set(newValue, forKey: key) }
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
    var closeTileOnExit = Settings.closeTileOnExit
    var claudeAllowBypass = Settings.claudeAllowBypass
    var claudeMode = Settings.claudeMode
    var claudeModel = Settings.claudeModel
    var claudeEffort = Settings.claudeEffort
    var hotkeys = Hotkeys.current
    var uiScale = Settings.uiScale
    var fontName = Settings.terminalFontName
    var fontSize = Settings.terminalFontSize
    var lineSpacing = Settings.terminalLineSpacing
    var padding = Settings.terminalPadding
    var metal = Settings.terminalMetal
    var ask = Dictionary(uniqueKeysWithValues: Settings.Ask.allCases.map { ($0, $0.enabled) })
    @ObservationIgnored lazy var fonts: [(name: String, display: String)] = {
        let list = Settings.monospaceFonts
        return list.contains { $0.name == fontName } ? list : [(fontName, fontName)] + list
    }()
    /// Aktion, deren Kürzel gerade aufgenommen wird.
    var recording: HotkeyAction?
    var onDone: (() -> Void)?
    @ObservationIgnored private var monitor: Any?

    func save() {
        stopRecording()
        let p = startFolder.trimmingCharacters(in: .whitespacesAndNewlines)
        if !p.isEmpty { Settings.startFolder = p }
        Settings.editorCommand = editorCommand.trimmingCharacters(in: .whitespacesAndNewlines)
        Settings.showLastMessage = showLastMessage
        Settings.stackShowPath = stackShowPath
        Settings.closeTileOnExit = closeTileOnExit
        Settings.claudeAllowBypass = claudeAllowBypass
        Settings.claudeMode = claudeMode
        Settings.claudeModel = claudeModel
        Settings.claudeEffort = claudeEffort
        Hotkeys.current = hotkeys
        Settings.uiScale = uiScale
        Settings.terminalFontName = fontName
        Settings.terminalFontSize = fontSize
        Settings.terminalLineSpacing = lineSpacing
        Settings.terminalPadding = padding
        Settings.terminalMetal = metal
        for (a, on) in ask { a.enabled = on }
        onDone?()
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

/// ⌘, Einstellungen. ⌘⏎ speichert, Esc bricht ab.
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
        let nav: [HotkeyAction] = [.focusLeft, .focusRight, .focusUp, .focusDown, .nextSession, .prevSession, .lastSession, .previewNext, .previewPrev]
            + HotkeyAction.allCases.filter { $0.tileIndex != nil } + [.focusSidebar, .focusWorkspace]
        return [("Navigation", nav), ("Kacheln verwalten", HotkeyAction.allCases.filter { !nav.contains($0) })]
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

            heading("Sessions", first: true)
            VStack(spacing: 6) {
                setting("Startordner für ⌘N") {
                    HStack(spacing: 10) {
                        Text(Theme.shortPath(model.startFolder)).font(Theme.ui(12)).foregroundStyle(Theme.mutedColor)
                            .lineLimit(1).truncationMode(.head)
                        Button("Ordner wählen …") {
                            chooseFolder(start: model.startFolder) { if let p = $0 { model.startFolder = p } }
                        }
                        .buttonStyle(.plain).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
                        .padding(.horizontal, 10).padding(.vertical, 4).overlay(Rectangle().stroke(Theme.lineColor, lineWidth: 1))
                    }
                }
                setting("Externer Editor (Kommando wie im Terminal)") {
                    TextField("z. B. code", text: $model.editorCommand)
                        .textFieldStyle(.plain).font(Theme.ui(12)).foregroundStyle(Theme.fgColor)
                        .frame(width: 260 * Theme.scale).padding(.horizontal, 8).padding(.vertical, 3)
                        .background(Theme.bgColor)
                }
                setting("Wenn Claude endet (zweimal ⌃C, /exit)") {
                    HStack(spacing: 2) {
                        pill("Kachel bleibt, Klick setzt fort", on: !model.closeTileOnExit) { model.closeTileOnExit = false }
                        pill("Kachel schließen", on: model.closeTileOnExit) { model.closeTileOnExit = true }
                    }
                }
            }
            .padding(.horizontal, 16)

            heading("Claude", note: "gilt für neu gestartete Claude-Prozesse")
            VStack(spacing: 6) {
                setting("Bypass-Modus erlauben (--allow-dangerously-skip-permissions)") {
                    pill(model.claudeAllowBypass ? "an" : "aus", on: model.claudeAllowBypass) { model.claudeAllowBypass.toggle() }
                }
                setting("Startmodus") { options(Settings.claudeModes, $model.claudeMode) }
                setting("Modell") { options(Settings.claudeModels, $model.claudeModel) }
                setting("Effort") { options(Settings.claudeEfforts, $model.claudeEffort) }
            }
            .padding(.horizontal, 16)

            heading("Darstellung")
            VStack(spacing: 6) {
                setting("UI-Größe") { choice(Settings.uiScales, $model.uiScale) { "\(Int(($0 * 100).rounded())) %" } }
                setting("Terminal-Schrift") {
                    Picker("", selection: $model.fontName) {
                        ForEach(model.fonts, id: \.name) { Text($0.display).tag($0.name) }
                    }
                    .labelsHidden().pickerStyle(.menu).frame(width: 260 * Theme.scale)
                }
                setting("Terminal-Schriftgröße  ⌘+ ⌘- ⌘0  ⌘ Mausrad") {
                    HStack(spacing: 2) {
                        pill("−", on: false) { model.fontSize = max(Settings.fontSizes.lowerBound, model.fontSize - 1) }
                        Text("\(Int(model.fontSize)) pt").font(Theme.ui(12, bold: true)).frame(width: 60 * Theme.scale)
                        pill("+", on: false) { model.fontSize = min(Settings.fontSizes.upperBound, model.fontSize + 1) }
                    }
                }
                setting("Zeilenabstand") { choice(Settings.lineSpacings, $model.lineSpacing) { "\(Int(($0 * 100).rounded())) %" } }
                setting("Innenabstand der Kacheln") { choice(Settings.paddings, $model.padding) { "\(Int($0)) px" } }
                setting("Terminal auf der GPU zeichnen (Metal)") { pill(model.metal ? "an" : "aus", on: model.metal) { model.metal.toggle() } }
            }
            .padding(.horizontal, 16)

            heading("Baum und Kacheln")
            VStack(spacing: 6) {
                setting("Pfad in Stack-Zeilen") { pill(model.stackShowPath ? "an" : "aus", on: model.stackShowPath) { model.stackShowPath.toggle() } }
                setting("Letzte Antwort von Claude im Baum") { pill(model.showLastMessage ? "an" : "aus", on: model.showLastMessage) { model.showLastMessage.toggle() } }
            }
            .padding(.horizontal, 16)

            heading("Rückfragen", note: "aus = ohne Nachfrage ausführen")
            VStack(spacing: 6) {
                ForEach(Settings.Ask.allCases, id: \.self) { a in
                    let on = model.ask[a] ?? true
                    setting(a.title) { pill(on ? "an" : "aus", on: on) { model.ask[a] = !on } }
                }
            }
            .padding(.horizontal, 16)

            heading("Tastenkürzel")
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(hotkeyGroups, id: \.0) { title, actions in
                        Text(title).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.mutedColor)
                            .padding(.top, title == hotkeyGroups.first?.0 ? 0 : 12).padding(.bottom, 4)
                        ForEach(actions, id: \.self) { a in row(a) }
                    }
                }
                .padding(.horizontal, 16).padding(.vertical, 4)
            }
            .frame(height: 320 * Theme.scale)
            DialogFoot(hint: "Kürzel anklicken, Tasten drücken · ⌫ entfernt · Esc abbrechen", button: "Speichern") { model.save() }
        }
        .frame(width: 900 * Theme.scale, alignment: .leading)
        .background(Theme.panelColor)
        .onDisappear { model.stopRecording() }
    }

    private func setting<C: View>(_ title: String, @ViewBuilder _ control: () -> C) -> some View {
        HStack(spacing: 8) {
            Text(title).foregroundStyle(Theme.fgColor)
            Spacer()
            control()
        }
        .font(Theme.ui(12))
    }

    /// Segmentierte Auswahl im App-Stil: gewählter Wert hervorgehoben.
    private func choice(_ values: [Double], _ value: Binding<Double>, _ text: @escaping (Double) -> String) -> some View {
        HStack(spacing: 2) {
            ForEach(values, id: \.self) { v in pill(text(v), on: abs(value.wrappedValue - v) < 0.001) { value.wrappedValue = v } }
        }
    }

    /// Wie `choice`, für Texte. "" heißt Claude-Default.
    private func options(_ values: [String], _ value: Binding<String>) -> some View {
        HStack(spacing: 2) {
            ForEach(values, id: \.self) { v in pill(v.isEmpty ? "Standard" : v, on: value.wrappedValue == v) { value.wrappedValue = v } }
        }
    }

    private func pill(_ text: String, on: Bool, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(text).font(Theme.ui(12, bold: on))
                .foregroundStyle(on ? Theme.bgColor : Theme.fgColor)
                .frame(minWidth: 28 * Theme.scale).padding(.horizontal, 8).padding(.vertical, 3)
                .background(on ? Theme.runningColor : Theme.bgColor)
        }
        .buttonStyle(.plain)
    }

    private func row(_ a: HotkeyAction) -> some View {
        let key = model.hotkeys[a]
        let isRecording = model.recording == a
        return HStack(spacing: 8) {
            Text(a.title).foregroundStyle(Theme.fgColor)
            Spacer()
            if key != a.defaultKey, !isRecording {
                Button { model.hotkeys[a] = a.defaultKey } label: { Text("Standard").foregroundStyle(Theme.mutedColor) }
                    .buttonStyle(.plain).help("Zurück auf \(a.defaultKey.display)")
            }
            Button { isRecording ? model.stopRecording() : model.record(a) } label: {
                Text(isRecording ? "Tasten drücken …" : key?.display ?? "–")
                    .font(Theme.ui(12, bold: true))
                    .foregroundStyle(isRecording ? Theme.bgColor : Theme.fgColor)
                    .frame(width: 150 * Theme.scale).padding(.vertical, 3)
                    .background(isRecording ? Theme.runningColor : Theme.bgColor)
            }
            .buttonStyle(.plain)
        }
        .font(Theme.ui(12))
    }
}
