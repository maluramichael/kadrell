import SwiftUI

// Abschnitte der Einstellungen (⌘,), jeder für sich. Reihenfolge und Spalten bestimmt `SettingsView.sections`.

struct SessionsSettings: SettingsBlock {
    @Bindable var model: SettingsModel
    @FocusState private var editorFocused: Bool

    var body: some View {
        SettingsSection(String(localized: "Sessions", bundle: Bundle.app)) {
            setting(String(localized: "Projektordner für ⌘N (nach Git-Repos durchsucht)", bundle: Bundle.app)) {
                HStack(spacing: 4) {
                    pathField(Binding(get: { Theme.shortPath(model.startFolder) }, set: { model.startFolder = $0 }),
                              onTab: { if let p = FolderIndex.expandAbbreviated(model.startFolder).first { model.startFolder = p } }) { urls in
                        guard let d = urls.lazy.compactMap({ FolderIndex.folder(for: $0.path) }).first else { return false }
                        model.startFolder = d
                        return true
                    }
                    iconButton("folder") { chooseFolder(start: model.startFolder) { if let p = $0 { model.startFolder = p } } }.help("Im Finder wählen")
                }
            }
            setting(String(localized: "Externer Editor (Kommando wie im Terminal)", bundle: Bundle.app)) {
                TextField("z. B. code", text: $model.editorCommand)
                    .textFieldStyle(.plain).foregroundStyle(Theme.fgColor)
                    .focused($editorFocused).instrumentWell(focused: editorFocused)
            }
            setting(String(localized: "Fertige Sessions automatisch trennen (0 = nie)", bundle: Bundle.app)) {
                slider(model.binding(\.autoDetachMinutes), Settings.autoDetachRange, step: 5, unit: String(localized: "min", bundle: Bundle.app))
            }
            setting(String(localized: "Wenn Claude endet (zweimal ⌃C, /exit)", bundle: Bundle.app)) {
                menu(model.binding(\.closeTileOnExit), [(false, String(localized: "Kachel bleibt, Klick setzt fort", bundle: Bundle.app)), (true, String(localized: "Kachel schließen", bundle: Bundle.app))])
            }
        }
    }
}

struct ClaudeSettings: SettingsBlock {
    @Bindable var model: SettingsModel

    var body: some View {
        SettingsSection(String(localized: "Claude", bundle: Bundle.app), note: String(localized: "gilt für neu gestartete Claude-Prozesse", bundle: Bundle.app)) {
            setting(String(localized: "Im Fullscreen starten (kein doppelter Text beim Scrollen)", bundle: Bundle.app)) { onOff(model.binding(\.claudeFullscreen)) }
            setting(String(localized: "Bypass-Modus erlauben (--allow-dangerously-skip-permissions)", bundle: Bundle.app)) { onOff(model.binding(\.claudeAllowBypass)) }
            setting(String(localized: "Startmodus", bundle: Bundle.app)) { options(Settings.claudeModes, model.binding(\.claudeMode)) }
            setting(String(localized: "Modell", bundle: Bundle.app)) { options(Settings.claudeModels, model.binding(\.claudeModel)) }
            setting(String(localized: "Effort", bundle: Bundle.app)) { options(Settings.claudeEfforts, model.binding(\.claudeEffort)) }
            setting(String(localized: "Advisor (stärkeres Modell für Plan, Fehler, Abschluss)", bundle: Bundle.app)) { options(Settings.claudeAdvisors, model.binding(\.claudeAdvisor)) }
            setting(String(localized: "Sessions dürfen andere Sessions steuern (kadrell send, capture, kill)", bundle: Bundle.app)) { onOff(model.binding(\.controlOtherSessions)) }
        }
    }
}

struct AccountsSettings: SettingsBlock {
    @Bindable var model: SettingsModel

    var body: some View {
        SettingsSection(String(localized: "Konten", bundle: Bundle.app), note: String(localized: "mehrere Claude-Accounts, Umschalten ohne neues Login", bundle: Bundle.app)) {
            ForEach(model.accountList) { a in
                setting(a.title + (a.planName.map { " · \($0)" } ?? "")) {
                    HStack(spacing: 10) {
                        if a.id == model.activeAccountId {
                            Text(String(localized: "aktiv", bundle: Bundle.app)).foregroundStyle(Theme.accentTextColor)
                        } else {
                            smallKey(String(localized: "Wechseln", bundle: Bundle.app)) { model.switchAccount(a.id) }
                        }
                        Spacer(minLength: 8)
                        smallKey(String(localized: "Entfernen", bundle: Bundle.app)) { model.removeAccount(a.id) }
                    }.frame(maxWidth: .infinity, alignment: .trailing)
                }
            }
            if model.currentEmail == nil || !model.accountList.contains(where: { $0.email == model.currentEmail }) {
                setting(String(localized: "Angemeldeten Account hinzufügen", bundle: Bundle.app)) {
                    HStack(spacing: 8) {
                        if let e = model.currentEmail { Text(e).foregroundStyle(Theme.mutedColor).lineLimit(1) }
                        Spacer(minLength: 8)
                        smallKey(String(localized: "Hinzufügen", bundle: Bundle.app)) { model.addAccount() }
                    }.frame(maxWidth: .infinity)
                }
            }
            setting(String(localized: "Automatisch wechseln bei Limit", bundle: Bundle.app)) { onOff(model.binding(\.autoswitchEnabled)) }
            setting(String(localized: "Umschalten ab Auslastung", bundle: Bundle.app)) {
                slider(model.binding(\.autoswitchThreshold), Settings.autoswitchRange, step: 1, unit: "%")
            }
            setting(String(localized: "Wechsel-Meldung anzeigen", bundle: Bundle.app)) { onOff(model.binding(\.autoswitchNotice)) }
            setting(String(localized: "Warnen bei zu schnellem 7-Tage-Verbrauch", bundle: Bundle.app)) { onOff(model.binding(\.autoswitchPaceWarning)) }
            setting(String(localized: "Nach Wochentempo verteilen", bundle: Bundle.app)) { onOff(model.binding(\.autoswitchOnPace)) }
            setting(String(localized: "5-Stunden-Limit in der Leiste (bei Ultra nie)", bundle: Bundle.app)) { onOff(model.binding(\.barShowSession)) }
            setting(String(localized: "7-Tage-Limit in der Leiste", bundle: Bundle.app)) { onOff(model.binding(\.barShowWeekly)) }
            setting(String(localized: "Fable-Kontingent in der Leiste", bundle: Bundle.app)) { onOff(model.binding(\.barShowFable)) }
        }
    }
}

struct AppearanceSettings: SettingsBlock {
    @Bindable var model: SettingsModel

    var body: some View {
        SettingsSection(String(localized: "Darstellung", bundle: Bundle.app)) {
            setting(String(localized: "Sprache", bundle: Bundle.app)) {
                menu(model.binding(\.language), Localization.Language.allCases.map { ($0, "\($0.flag)  \($0.title)") })
            }
            setting(String(localized: "Farbschema", bundle: Bundle.app)) { menu(model.binding(\.colorTheme), ColorTheme.all.map { ($0.id, $0.name) }) }
            setting(String(localized: "Menüleiste", bundle: Bundle.app)) { menu(model.binding(\.statusBar), StatusMenu.Display.allCases.map { ($0, $0.title) }) }
            setting(String(localized: "UI-Größe", bundle: Bundle.app)) { slider(model.binding(\.uiScale), Settings.uiScalePercent, step: 5, factor: 100, unit: "%", live: false) }
            setting(String(localized: "Terminal-Schrift", bundle: Bundle.app)) { menu(model.binding(\.terminalFontName), model.fonts.map { ($0.name, $0.display) }) }
            setting(String(localized: "Terminal-Schriftgröße  ⌘+ ⌘- ⌘0  ⌘ Mausrad", bundle: Bundle.app)) { slider(model.binding(\.terminalFontSize), Settings.fontSizes, step: 1, unit: "pt") }
            setting(String(localized: "Zeilenabstand", bundle: Bundle.app)) { slider(model.binding(\.terminalLineSpacing), Settings.lineSpacingPercent, step: 5, factor: 100, unit: "%") }
            setting(String(localized: "Innenabstand der Kacheln", bundle: Bundle.app)) { slider(model.binding(\.terminalPadding), Settings.pixelRange, step: 1, unit: "px") }
            setting(String(localized: "Verlauf zum Zurückscrollen", bundle: Bundle.app)) { slider(model.binding(\.terminalScrollback), Settings.scrollbackRange, step: 1_000, unit: String(localized: "Zeilen", bundle: Bundle.app)) }
            setting(String(localized: "Abstand zwischen Kacheln", bundle: Bundle.app)) { slider(model.binding(\.tileGap), Settings.pixelRange, step: 1, unit: "px") }
            setting(String(localized: "Terminal auf der GPU zeichnen (Metal)", bundle: Bundle.app)) { onOff(model.binding(\.terminalMetal)) }
        }
    }
}

struct CustomizeSettings: SettingsBlock {
    @Bindable var model: SettingsModel

    var body: some View {
        SettingsSection(String(localized: "Anpassen", bundle: Bundle.app), note: String(localized: "Bild hinter Baum und Kacheln", bundle: Bundle.app)) {
            setting(String(localized: "Hintergrundbild", bundle: Bundle.app)) {
                let image = model.binding(\.backgroundImage)
                HStack(spacing: 4) {
                    pathField(Binding(get: { Theme.shortPath(image.wrappedValue) }, set: { image.wrappedValue = FolderIndex.normalize($0) }),
                              placeholder: String(localized: "kein Bild · Datei hineinziehen", bundle: Bundle.app)) { urls in
                        guard let u = urls.first(where: { NSImage(contentsOf: $0) != nil }) else { return false }
                        image.wrappedValue = u.path
                        return true
                    }
                    iconButton("photo") {
                        chooseFolder(start: image.wrappedValue.isEmpty ? "~/Pictures" : (image.wrappedValue as NSString).deletingLastPathComponent, images: true) { if let p = $0 { image.wrappedValue = p } }
                    }.help("Bild wählen")
                    iconButton("xmark") { image.wrappedValue = "" }.help("Kein Bild")
                }
            }
            setting(String(localized: "Deckkraft von Baum und Kacheln", bundle: Bundle.app)) { slider(model.binding(\.tileOpacity), 0...100, step: 1, factor: 100, unit: "%") }
        }
    }
}

struct TreeSettings: SettingsBlock {
    @Bindable var model: SettingsModel

    var body: some View {
        SettingsSection(String(localized: "Baum und Kacheln", bundle: Bundle.app)) {
            setting(String(localized: "Design des Baums", bundle: Bundle.app)) { menu(model.binding(\.sidebarStyle), SidebarStyle.allCases.map { ($0, $0.title) }) }
            setting(String(localized: "Laufzeit im Baum", bundle: Bundle.app)) { onOff(model.binding(\.sidebarShowAge)) }
            setting(String(localized: "Pfad in Stack-Zeilen", bundle: Bundle.app)) { onOff(model.binding(\.stackShowPath)) }
            setting(String(localized: "Letzte Antwort von Claude im Baum", bundle: Bundle.app)) { onOff(model.binding(\.showLastMessage)) }
            setting(String(localized: "Sounds", bundle: Bundle.app)) { menu(model.binding(\.sounds), Feedback.Level.allCases.map { ($0, $0.title) }) }
            setting(String(localized: "Systembenachrichtigungen", bundle: Bundle.app)) { menu(model.binding(\.notifications), Notifications.Level.allCases.map { ($0, $0.title) }) }
        }
    }
}

struct UpdatesSettings: SettingsBlock {
    @Bindable var model: SettingsModel

    var body: some View {
        SettingsSection(String(localized: "Updates", bundle: Bundle.app)) {
            setting(String(localized: "Nach Updates suchen", bundle: Bundle.app)) { onOff(model.binding(\.checkForUpdates)) }
        }
    }
}

struct AutoModeSettings: SettingsBlock {
    @Bindable var model: SettingsModel

    var body: some View {
        SettingsSection(String(localized: "Auto-Modus", bundle: Bundle.app), note: String(localized: "AUTO in der Leiste", bundle: Bundle.app)) {
            setting(String(localized: "Gilt für", bundle: Bundle.app)) { menu(model.binding(\.autoAllSessions), [(true, String(localized: "alle Sessions", bundle: Bundle.app)), (false, String(localized: "nur die Auswahl im Baum", bundle: Bundle.app))]) }
            setting(String(localized: "Zeigt", bundle: Bundle.app)) {
                let waiting = model.binding(\.autoWaiting), running = model.binding(\.autoRunning)
                menu(Binding(get: { AutoShow(waiting: waiting.wrappedValue, running: running.wrappedValue) },
                             set: { waiting.wrappedValue = $0.waiting; running.wrappedValue = $0.running }),
                     [(AutoShow(waiting: true, running: false), String(localized: "wartende", bundle: Bundle.app)), (AutoShow(waiting: false, running: true), String(localized: "arbeitende", bundle: Bundle.app)),
                      (AutoShow(waiting: true, running: true), String(localized: "wartende und arbeitende", bundle: Bundle.app)), (AutoShow(waiting: false, running: false), String(localized: "keine", bundle: Bundle.app))])
            }
        }
    }
}

/// Auswahl „Auto-Modus zeigt“: zwei Schalter als ein Dropdown.
private struct AutoShow: Hashable {
    let waiting: Bool
    let running: Bool
}

struct AsksSettings: SettingsBlock {
    @Bindable var model: SettingsModel

    var body: some View {
        SettingsSection(String(localized: "Rückfragen", bundle: Bundle.app), note: String(localized: "aus = ohne Nachfrage ausführen", bundle: Bundle.app)) {
            ForEach(Settings.Ask.allCases, id: \.self) { a in
                setting(a.title) { onOff(model.binding(a, \.enabled)) }
            }
        }
    }
}

/// Kürzel nach denselben Bereichen wie die F1-Hilfe, je Bereich ein eigener Baustein: so kann die Navigation links
/// unten und der Rest rechts stehen, wenn das die Spalten ausgleicht. Die Überschrift trägt der erste Bereich.
struct HotkeySettings: SettingsBlock {
    enum Part { case navigation, tiles }

    @Bindable var model: SettingsModel
    let part: Part

    /// Navigation; „Kacheln verwalten“ nimmt den Rest, damit keine Aktion verschwindet.
    static var navigation: [HotkeyAction] {
        [.focusLeft, .focusRight, .focusUp, .focusDown, .nextSession, .prevSession, .lastSession, .previewNext, .previewPrev, .nextWaiting]
            + HotkeyAction.allCases.filter { $0.tileIndex != nil } + [.focusSidebar, .focusWorkspace, .focusRightSidebar]
    }

    private var actions: [HotkeyAction] {
        part == .navigation ? Self.navigation : HotkeyAction.allCases.filter { !Self.navigation.contains($0) }
    }

    private var title: String {
        part == .navigation ? String(localized: "Navigation", bundle: Bundle.app) : String(localized: "Kacheln verwalten", bundle: Bundle.app)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if part == .navigation {
                SettingsHeading(title: String(localized: "Tastenkürzel", bundle: Bundle.app), note: String(localized: "Kürzel anklicken, Tasten drücken · ⌫ entfernt", bundle: Bundle.app))
            }
            Text(title).capsLabel(bold: true)
                .padding(.horizontal, 16).padding(.top, part == .navigation ? 0 : 14).padding(.bottom, 4)
            SettingsTable { ForEach(actions, id: \.self) { a in HotkeyRow(model: model, action: a) } }
        }
    }
}

/// Kürzel-Zeile: Feld in der rechten Spalte, „Standard“ davor, solange das Kürzel abweicht.
private struct HotkeyRow: View {
    @Bindable var model: SettingsModel
    let action: HotkeyAction
    @Environment(\.settingsControlWidth) private var controlWidth

    var body: some View {
        let key = model.hotkeys[action]
        let isRecording = model.recording == action
        HStack(spacing: 8) {
            Text(action.title).foregroundStyle(Theme.fgColor).lineLimit(1)
            Spacer(minLength: 12)
            if key != action.defaultKey, !isRecording {
                Button { model.hotkeys[action] = action.defaultKey } label: { Text("Standard") }
                    .buttonStyle(InstrumentKeyStyle(.neutral, compact: true)).kbdFocusRing().help("Zurück auf \(action.defaultKey.display)")
            }
            Button { isRecording ? model.stopRecording() : model.record(action) } label: {
                Text(isRecording ? String(localized: "Tasten drücken …", bundle: Bundle.app) : key?.display ?? "–")
                    .font(Theme.ui(12, bold: true))
                    .foregroundStyle(isRecording ? Theme.accentTextColor : Theme.fgColor)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .instrumentWell(focused: isRecording).frame(width: controlWidth)
            }
            .buttonStyle(.plain).kbdFocusRing()
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }
}
