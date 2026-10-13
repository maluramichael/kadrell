import SwiftUI

/// Inhalt je Seite der Einstellungen (`SettingsPage`).
extension SettingsView {
    @ViewBuilder
    func content(_ page: SettingsPage) -> some View {
        switch page {
        case .general: general
        case .appearance: appearance
        case .hotkeys: hotkeysPage
        case .claude: claude
        case .accounts: accountsPage
        case .pi: pi
        }
    }

    // MARK: Allgemein

    @ViewBuilder
    private var general: some View {
        heading(String(localized: "Sessions", bundle: Bundle.app), first: true)
        table {
            setting(String(localized: "Projektordner für ⌘N (nach Git-Repos durchsucht)", bundle: Bundle.app)) { startFolderField }
            setting(String(localized: "Standard-Harness (⌘N, ⌘⏎ aus Terminal oder Remote)", bundle: Bundle.app)) {
                menu(model.binding(\.defaultAgent), AgentKind.allCases.map { ($0, $0.displayName) })
            }
            setting(String(localized: "Externer Editor (Kommando wie im Terminal)", bundle: Bundle.app)) {
                textField($model.editorCommand, placeholder: String(localized: "z. B. code", bundle: Bundle.app))
            }
            setting(String(localized: "Fertige Sessions automatisch trennen (0 = nie)", bundle: Bundle.app)) {
                intSlider(model.binding(\.autoDetachMinutes), Settings.autoDetachRange, step: 5, unit: String(localized: "min", bundle: Bundle.app))
            }
            setting(String(localized: "Wenn der Agent endet (zweimal ⌃C, /exit)", bundle: Bundle.app)) {
                menu(model.binding(\.closeTileOnExit), [(false, String(localized: "Kachel bleibt, Klick setzt fort", bundle: Bundle.app)), (true, String(localized: "Kachel schließen", bundle: Bundle.app))])
            }
            setting(String(localized: "Sessions dürfen andere Sessions steuern (kadrell send, capture, kill)", bundle: Bundle.app)) { onOff(model.binding(\.controlOtherSessions)) }
        }

        heading(String(localized: "Auto-Modus", bundle: Bundle.app), note: String(localized: "AUTO in der Leiste", bundle: Bundle.app))
        table {
            setting(String(localized: "Gilt für", bundle: Bundle.app)) { menu(model.binding(\.autoAllSessions), [(true, String(localized: "alle Sessions", bundle: Bundle.app)), (false, String(localized: "nur die Auswahl im Baum", bundle: Bundle.app))]) }
            setting(String(localized: "Zeigt", bundle: Bundle.app)) { autoShowMenu }
        }

        heading(String(localized: "Rückfragen", bundle: Bundle.app), note: String(localized: "aus = ohne Nachfrage ausführen", bundle: Bundle.app))
        table {
            ForEach(Settings.Ask.allCases, id: \.self) { a in
                setting(a.title) { onOff(model.binding(a, \.enabled)) }
            }
        }

        heading(String(localized: "Updates", bundle: Bundle.app))
        table {
            setting(String(localized: "Nach Updates suchen", bundle: Bundle.app)) { onOff(model.binding(\.checkForUpdates)) }
        }
    }

    private var startFolderField: some View {
        HStack(spacing: 4) {
            pathField(Binding(get: { Theme.shortPath(model.startFolder) }, set: { model.startFolder = $0 }),
                      onTab: { if let p = FolderIndex.expandAbbreviated(model.startFolder).first { model.startFolder = p } }) { urls in
                guard let d = urls.lazy.compactMap({ FolderIndex.folder(for: $0.path) }).first else { return false }
                model.startFolder = d
                return true
            }
            iconButton("folder") { chooseFolder(start: model.startFolder) { if let p = $0 { model.startFolder = p } } }
                .help(String(localized: "Im Finder wählen", bundle: Bundle.app))
        }
    }

    private var autoShowMenu: some View {
        let waiting = model.binding(\.autoWaiting), running = model.binding(\.autoRunning)
        return menu(Binding(get: { AutoShow(waiting: waiting.wrappedValue, running: running.wrappedValue) },
                            set: { waiting.wrappedValue = $0.waiting; running.wrappedValue = $0.running }),
                    [(AutoShow(waiting: true, running: false), String(localized: "wartende", bundle: Bundle.app)), (AutoShow(waiting: false, running: true), String(localized: "arbeitende", bundle: Bundle.app)),
                     (AutoShow(waiting: true, running: true), String(localized: "wartende und arbeitende", bundle: Bundle.app)), (AutoShow(waiting: false, running: false), String(localized: "keine", bundle: Bundle.app))])
    }

    // MARK: Darstellung

    @ViewBuilder
    private var appearance: some View {
        heading(String(localized: "Darstellung", bundle: Bundle.app), first: true)
        table {
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
            setting(String(localized: "Verlauf zum Zurückscrollen", bundle: Bundle.app)) { intSlider(model.binding(\.terminalScrollback), Settings.scrollbackRange, step: 1_000, unit: String(localized: "Zeilen", bundle: Bundle.app)) }
            setting(String(localized: "Abstand zwischen Kacheln", bundle: Bundle.app)) { slider(model.binding(\.tileGap), Settings.pixelRange, step: 1, unit: "px") }
            setting(String(localized: "Terminal auf der GPU zeichnen (Metal)", bundle: Bundle.app)) { onOff(model.binding(\.terminalMetal)) }
        }

        heading(String(localized: "Anpassen", bundle: Bundle.app), note: String(localized: "Bild hinter Baum und Kacheln", bundle: Bundle.app))
        table {
            setting(String(localized: "Hintergrundbild", bundle: Bundle.app)) { backgroundImageField }
            setting(String(localized: "Deckkraft von Baum und Kacheln", bundle: Bundle.app)) { slider(model.binding(\.tileOpacity), 0...100, step: 1, factor: 100, unit: "%") }
        }

        heading(String(localized: "Baum und Kacheln", bundle: Bundle.app))
        table {
            setting(String(localized: "Design des Baums", bundle: Bundle.app)) { menu(model.binding(\.sidebarStyle), SidebarStyle.allCases.map { ($0, $0.title) }) }
            setting(String(localized: "Laufzeit im Baum", bundle: Bundle.app)) { onOff(model.binding(\.sidebarShowAge)) }
            setting(String(localized: "Pfad in Stack-Zeilen", bundle: Bundle.app)) { onOff(model.binding(\.stackShowPath)) }
            setting(String(localized: "Letzte Antwort des Agents im Baum", bundle: Bundle.app)) { onOff(model.binding(\.showLastMessage)) }
            setting(String(localized: "Sounds", bundle: Bundle.app)) { menu(model.binding(\.sounds), Feedback.Level.allCases.map { ($0, $0.title) }) }
            setting(String(localized: "Systembenachrichtigungen", bundle: Bundle.app)) { menu(model.binding(\.notifications), Notifications.Level.allCases.map { ($0, $0.title) }) }
        }
    }

    private var backgroundImageField: some View {
        let image = model.binding(\.backgroundImage)
        return HStack(spacing: 4) {
            pathField(Binding(get: { Theme.shortPath(image.wrappedValue) }, set: { image.wrappedValue = FolderIndex.normalize($0) }),
                      placeholder: String(localized: "kein Bild · Datei hineinziehen", bundle: Bundle.app)) { urls in
                guard let u = urls.first(where: { NSImage(contentsOf: $0) != nil }) else { return false }
                image.wrappedValue = u.path
                return true
            }
            iconButton("photo") {
                chooseFolder(start: image.wrappedValue.isEmpty ? "~/Pictures" : (image.wrappedValue as NSString).deletingLastPathComponent, images: true) { if let p = $0 { image.wrappedValue = p } }
            }.help(String(localized: "Bild wählen", bundle: Bundle.app))
            iconButton("xmark") { image.wrappedValue = "" }.help(String(localized: "Kein Bild", bundle: Bundle.app))
        }
    }

    // MARK: Tastenkürzel

    /// Kürzel nach denselben Bereichen wie die F1-Hilfe; die zweite Gruppe nimmt den Rest, damit keine Aktion verschwindet.
    private var hotkeyGroups: [(String, [HotkeyAction])] {
        let nav: [HotkeyAction] = [.focusLeft, .focusRight, .focusUp, .focusDown, .nextSession, .prevSession, .lastSession, .previewNext, .previewPrev, .nextWaiting]
            + HotkeyAction.allCases.filter { $0.tileIndex != nil } + [.focusSidebar, .focusWorkspace, .focusRightSidebar]
        return [(String(localized: "Navigation", bundle: Bundle.app), nav), (String(localized: "Kacheln verwalten", bundle: Bundle.app), HotkeyAction.allCases.filter { !nav.contains($0) })]
    }

    @ViewBuilder
    private var hotkeysPage: some View {
        heading(String(localized: "Tastenkürzel", bundle: Bundle.app), note: String(localized: "Kürzel anklicken, Tasten drücken · ⌫ entfernt", bundle: Bundle.app), first: true)
        ForEach(hotkeyGroups, id: \.0) { title, actions in
            Text(title).font(Theme.ui(11, bold: true)).foregroundStyle(Theme.mutedColor)
                .padding(.horizontal, 16).padding(.top, title == hotkeyGroups.first?.0 ? 0 : 12).padding(.bottom, 4)
            table { ForEach(actions, id: \.self) { a in hotkeyRow(a) } }
        }
    }

    private func hotkeyRow(_ a: HotkeyAction) -> some View {
        let key = model.hotkeys[a]
        let isRecording = model.recording == a
        return HStack(spacing: 8) {
            Text(a.title).foregroundStyle(Theme.fgColor).lineLimit(1)
            Spacer(minLength: 12)
            if key != a.defaultKey, !isRecording {
                textButton(String(localized: "Standard", bundle: Bundle.app), color: Theme.mutedColor) { model.hotkeys[a] = a.defaultKey }
                    .help(String(localized: "Zurück auf \(a.defaultKey.display)", bundle: Bundle.app))
            }
            Button { isRecording ? model.stopRecording() : model.record(a) } label: {
                Text(isRecording ? String(localized: "Tasten drücken …", bundle: Bundle.app) : key?.display ?? "–")
                    .font(Theme.ui(12, bold: true))
                    .foregroundStyle(isRecording ? Theme.bgColor : Theme.fgColor)
                    .padding(.horizontal, 8).padding(.vertical, 3).frame(width: controlWidth, alignment: .leading)
                    .background(isRecording ? Theme.runningColor : Theme.bgColor)
            }
            .buttonStyle(.plain).kbdFocusRing()
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }

    // MARK: Claude

    @ViewBuilder
    private var claude: some View {
        heading(AgentKind.claude.displayName, note: String(localized: "gilt für neu gestartete Claude-Prozesse", bundle: Bundle.app), first: true)
        table {
            setting(String(localized: "Modell", bundle: Bundle.app)) { options(Settings.claudeModels, model.binding(\.claudeModel)) }
            setting(String(localized: "Effort", bundle: Bundle.app)) { options(Settings.claudeEfforts, model.binding(\.claudeEffort)) }
            setting(String(localized: "Startmodus", bundle: Bundle.app)) { options(Settings.claudeModes, model.binding(\.claudeMode)) }
            setting(String(localized: "Bypass-Modus erlauben (--allow-dangerously-skip-permissions)", bundle: Bundle.app)) { onOff(model.binding(\.claudeAllowBypass)) }
            setting(String(localized: "Im Fullscreen starten (kein doppelter Text beim Scrollen)", bundle: Bundle.app)) { onOff(model.binding(\.claudeFullscreen)) }
            setting(String(localized: "Programm", bundle: Bundle.app)) {
                Text(model.cli.map { Theme.shortPath($0.binary) } ?? "–").foregroundStyle(Theme.mutedColor).lineLimit(1).truncationMode(.head)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    // MARK: Konten

    @ViewBuilder
    private var accountsPage: some View {
        heading(String(localized: "Konten", bundle: Bundle.app), note: String(localized: "mehrere Claude-Accounts, Umschalten ohne neues Login", bundle: Bundle.app), first: true)
        table {
            ForEach(model.accountList) { a in
                setting(a.title + (a.subscription.map { " · \($0)" } ?? "")) { accountActions(a) }
            }
            if model.currentEmail == nil || !model.accountList.contains(where: { $0.email == model.currentEmail }) {
                setting(String(localized: "Angemeldeten Account hinzufügen", bundle: Bundle.app)) {
                    HStack(spacing: 8) {
                        if let e = model.currentEmail { Text(e).foregroundStyle(Theme.mutedColor).lineLimit(1) }
                        Spacer(minLength: 8)
                        textButton(String(localized: "Hinzufügen", bundle: Bundle.app), color: Theme.runningColor) { model.addAccount() }
                    }.frame(width: controlWidth)
                }
            }
        }

        heading(String(localized: "Automatisch wechseln", bundle: Bundle.app))
        table {
            setting(String(localized: "Automatisch wechseln bei Limit", bundle: Bundle.app)) { onOff(model.binding(\.autoswitchEnabled)) }
            setting(String(localized: "Umschalten ab Auslastung", bundle: Bundle.app)) {
                intSlider(model.binding(\.autoswitchThreshold), Settings.autoswitchRange, step: 1, unit: "%")
            }
            setting(String(localized: "Wechsel-Meldung anzeigen", bundle: Bundle.app)) { onOff(model.binding(\.autoswitchNotice)) }
            setting(String(localized: "Warnen bei zu schnellem 7-Tage-Verbrauch", bundle: Bundle.app)) { onOff(model.binding(\.autoswitchPaceWarning)) }
            setting(String(localized: "Bei zu schnellem 7-Tage-Verbrauch wechseln", bundle: Bundle.app)) { onOff(model.binding(\.autoswitchOnPace)) }
        }
    }

    private func accountActions(_ a: Account) -> some View {
        HStack(spacing: 10) {
            if a.id == model.activeAccountId {
                Text(String(localized: "aktiv", bundle: Bundle.app)).foregroundStyle(Theme.runningColor)
            } else {
                textButton(String(localized: "Wechseln", bundle: Bundle.app)) { model.switchAccount(a.id) }
            }
            Spacer(minLength: 8)
            textButton(String(localized: "Entfernen", bundle: Bundle.app), color: Theme.mutedColor) { model.removeAccount(a.id) }
        }.frame(width: controlWidth, alignment: .trailing)
    }

    // MARK: Pi

    @ViewBuilder
    private var pi: some View {
        heading(AgentKind.pi.displayName, note: String(localized: "gilt für neu gestartete Pi-Prozesse", bundle: Bundle.app), first: true)
        table {
            setting(String(localized: "Modell", bundle: Bundle.app)) { piModelMenu }
            setting(String(localized: "Thinking", bundle: Bundle.app)) { options(Settings.piThinkingLevels, model.binding(\.piThinking)) }
            setting(String(localized: "Programm (leer = automatisch)", bundle: Bundle.app)) {
                textField($model.piBinary, placeholder: model.piResolvedBinary.map(Theme.shortPath) ?? String(localized: "nicht gefunden", bundle: Bundle.app))
            }
        }
        Text(piNote).font(Theme.ui(11)).foregroundStyle(model.piResolvedBinary == nil ? Theme.errorColor : Theme.mutedColor)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.horizontal, 16).padding(.top, 8)
    }

    /// Modelle aus `pi --list-models`, dazu das gespeicherte, falls es dort (noch) fehlt.
    private var piModelMenu: some View {
        let current = Settings.piModel
        let listed = model.piModels ?? []
        let values = [""] + (current.isEmpty || listed.contains(current) ? [] : [current]) + listed
        let standard = model.piDefaultModel.map { String(localized: "Standard (\($0))", bundle: Bundle.app) }
            ?? String(localized: "Standard (Pi-Einstellung)", bundle: Bundle.app)
        return HStack(spacing: 4) {
            options(values, model.binding(\.piModel), defaultLabel: model.piModels == nil ? String(localized: "lädt …", bundle: Bundle.app) : standard,
                    width: controlWidth - 26 * Theme.scale)
            iconButton("arrow.clockwise") { model.reloadPiModels() }.help(String(localized: "Modelle neu einlesen", bundle: Bundle.app))
        }
    }

    private var piNote: String {
        guard model.piResolvedBinary != nil else {
            return String(localized: "Pi ist nicht installiert oder nicht im PATH der Login-Shell. Installation: pi.dev", bundle: Bundle.app)
        }
        return String(localized: "Modelle mit Zugangsdaten aus Pis eigener Konfiguration (~/.pi/agent). Kadrell gibt jeder Pi-Kachel eine Integration für Status und Titel mit, an Pis Dateien ändert es nichts.", bundle: Bundle.app)
    }
}
