import AppKit
import SwiftUI
import os

/// Fest verdrahtete Tasten im Event-Monitor, Namen wie `Hotkey.specials`.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "app")
    /// Offene Hauptfenster. `current` ist das zuletzt aktive: Menü, Kürzel, Palette, Dialoge und Fernsteuerung wirken dort.
    private var windows: [MainWindowController] = []
    private var current: MainWindowController!
    private var window: NSWindow! { current?.window }
    private var bar: StatusBarView { current.bar }
    private var split: ThinSplitView { current.split }
    private var sidebarScroll: NSScrollView { current.sidebarScroll }
    private var sidebar: SidebarView { current.sidebar }
    var workspace: WorkspaceView { current.workspace }
    let store = GroupStore()
    private var cli: ClaudeCLI!
    var registry: SessionRegistry!
    var attach: AttachManager!
    private let usage = UsageService()
    let accounts = AccountService()
    private let updateChecker = UpdateChecker()
    private var palette: PaletteWindow!
    lazy var sheets = SheetPresenter(host: { [weak self] in self?.current }, palette: { [weak self] in self?.palette })
    private let attention = AttentionTracker()
    private var fontScrollAccum: CGFloat = 0
    private var statusItem: StatusItemController?
    /// ⌘A/⌘⇧A: Auswahl davor und danach, damit ein zweiter Druck zurückschaltet.
    private var selectAllUndo: (shift: Bool, before: [String], focus: String?, after: Set<String>)?
    /// Sprache, in der die Palette gebaut wurde.
    private var paletteLanguage = Settings.language
    var controlServer: ControlServer?
    /// claude läuft, ist aber älter als `ClaudeCLI.minVersion`: nicht blockierend, nur die Leiste warnt (`recheckCLI`).
    private var versionWarning: String?
    /// Seit wann eine Session fertig (grün) ist, für das automatische Trennen. Kein Eintrag = arbeitet oder ist getrennt.
    private var idleSince: [String: Date] = [:]
    private var autoDetachTask: Task<Void, Never>?

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = AppBinary.atLaunch
        Localization.apply(Settings.language)
        // Nur eine Instanz pro Profil: hält schon eine das Profil, die nach vorn holen und selbst beenden.
        if NSClassFromString("XCTestCase") == nil, let other = Profile.acquire() {
            NSRunningApplication(processIdentifier: other)?.activate()
            NSApp.terminate(nil)
            return
        }
        Profile.sweepStale()
        Feedback.observeReduceMotion()
        // Dock-Icon direkt aus dem Bundle: LaunchServices hält für Debug-Builds am selben Pfad gern das alte, leere Icon.
        if let url = Bundle.main.url(forResource: "Kadrell", withExtension: "icns"), let img = NSImage(contentsOf: url) { NSApp.applicationIconImage = img }
        buildMenu()
        attention.onTip = { [weak self] tip in self?.windows.forEach { $0.bar.tip = tip; $0.bar.needsDisplay = true } }
        attention.notify = { [weak self] key, s, waiting in
            Notifications.notify(sessionKey: key, group: self?.store.group(forSession: key)?.name ?? "", title: s.title,
                                 message: self?.registry?.lastMessages[key], waiting: waiting)
        }
        buildWindows()
        // Unter XCTest nur das Fenster, kein Polling.
        guard NSClassFromString("XCTestCase") == nil else { return }
        trapSignals()
        Task { await boot() }
        // Kein automatisches F1 mehr: der Leerzustand führt selbst zur ersten Session, F1 bleibt zum Nachschlagen.
        // Beim allerersten Start gibt es nichts „Neues“, danach nach jedem Versionssprung einmal.
        if !Profile.defaults.bool(forKey: "helpShown") {
            Profile.defaults.set(true, forKey: "helpShown")
            WhatsNew.lastSeenVersion = Settings.version
        } else {
            showWhatsNewIfNeeded()
        }
        let item = StatusItemController(store: store)
        item.onFocus = { [weak self] in self?.focusSession($0) }
        item.onNextWaiting = { [weak self] in self?.focusNextWaiting() }
        item.onOpen = { [weak self] in self?.showMainWindow() }
        statusItem = item
    }

    /// Erststart zeigt „Was ist neu“ nicht (nichts davon war vorher da, siehe `helpShown`-Zweig), jeder spätere
    /// Versionssprung einmalig schon. `WhatsNew.lastSeenVersion` übersteht einen Neustart.
    private func showWhatsNewIfNeeded() {
        let version = Settings.version
        // Temp-Profile erben den Lesestand des Standardprofils, sonst öffnet jeder Teststart den Dialog.
        guard !Profile.isTemporary, WhatsNew.lastSeenVersion != version else { return }
        WhatsNew.lastSeenVersion = version
        showWhatsNew(version: version, fallback: nil)
    }

    /// `fallback` steht, wenn die Version keine Changelog-Zeilen hat (Menüpunkt „Was ist neu“ manuell aufgerufen).
    private func showWhatsNew(version: String, fallback: String?) {
        let notes = WhatsNew.notes(version: version, changelog: WhatsNew.bundledChangelog())
        let title = String(localized: "Neu in \(version)", bundle: Bundle.app)
        if !notes.isEmpty {
            sheets.confirm(title, notes.joined(separator: "\n\n"), button: String(localized: "Super", bundle: Bundle.app), destructive: false) {}
        } else if let fallback {
            sheets.confirm(title, fallback, button: String(localized: "OK", bundle: Bundle.app), destructive: false) {}
        }
    }

    /// Fenster nach vorn: Menüleisten-Menü, Dock-Klick, Benachrichtigung, Beenden-Rückfrage.
    private func showMainWindow() {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
    }

    /// Session auf der Arbeitsfläche zeigen und fokussieren, im Baum aufklappen.
    private func reveal(_ key: String) {
        workspace.addMissing([key])
        workspace.setFocus(key)
        sidebar.reveal(key)
    }

    /// Fenster nach vorn, Session fokussieren: Menüleisten-Menü und Klick auf eine Systembenachrichtigung.
    func focusSession(_ key: String) {
        guard workspace.sessions[key] != nil else { return }
        showMainWindow()
        reveal(key)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    /// Dock-Klick, während das Fenster versteckt ist: zurückholen statt neu zu starten.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showMainWindow()
        return true
    }

    /// Laufen Claude-Prozesse, erst nachfragen (⌘Q, Menü, Dock, Abmelden, SIGTERM). Abbrechen und Rückfrage
    /// statt `.terminateLater`: der Dialog ist ein eigenes Overlay und braucht die normale Run-Loop.
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard !quitConfirmed, let attach, attach.attachedCount > 0 else { return .terminateNow }
        confirmQuit()
        return .terminateCancel
    }

    func applicationWillTerminate(_ notification: Notification) {
        autoDetachTask?.cancel()
        // Nur noch Reste (z. B. Abmelden ohne Prozesse): SIGHUP, beim nächsten Start setzt `--resume` fort.
        attach?.detachAll()
        controlServer?.stop()
        Profile.cleanUp()
    }

    private var quitConfirmed = false
    private var signalSources: [DispatchSourceSignal] = []

    private func confirmQuit() {
        NSApp.unhide(nil)
        window.deminiaturize(nil)
        showMainWindow()
        Task {
            await registry.pollNow()
            let running = registry.sessions.filter { attach.isAttached($0.id) }
            let busy = running.filter { $0.status == .running || $0.status == .waiting }
            let list = running.map { s in
                "· \(s.title)" + (s.status == .running ? String(localized: "  ARBEITET", bundle: Bundle.app) : s.status == .waiting ? String(localized: "  WARTET AUF ANTWORT", bundle: Bundle.app) : "")
            }.joined(separator: "\n")
            let title = busy.isEmpty ? String(localized: "Kadrell beenden?", bundle: Bundle.app) : String(localized: "Kadrell beenden? \(busy.count) Session(s) arbeiten gerade!", bundle: Bundle.app)
            let info = String(localized: "\(running.count) Claude-Prozess(e) werden sauber beendet. Laufende Arbeit bricht dabei ab. Die Konversationen bleiben erhalten und werden beim nächsten Start fortgesetzt.", bundle: Bundle.app)
                + "\n\n\(list)"
            sheets.confirm(title, info, button: String(localized: "Beenden", bundle: Bundle.app), ask: .quit) { [weak self] in
                guard let self else { return }
                Task {
                    await self.attach.shutdown()
                    self.quitConfirmed = true
                    NSApp.terminate(nil)
                }
            }
        }
    }

    /// `kill` (SIGTERM) und Ctrl-C im Terminal laufen über dieselbe Rückfrage. Force Quit (SIGKILL) lässt sich nicht abfangen.
    private func trapSignals() {
        for sig in [SIGTERM, SIGINT] {
            // Leerer Handler statt SIG_IGN: ignorierte Signale erben die Claude-Prozesse über fork/exec, Handler nicht.
            signal(sig) { _ in }
            let src = DispatchSource.makeSignalSource(signal: sig, queue: .main)
            src.setEventHandler { NSApp.terminate(nil) }
            src.resume()
            signalSources.append(src)
        }
    }

    // MARK: Aufbau

    /// Beim Start die zuletzt offenen Fenster wieder aufmachen, mindestens eins.
    private func buildWindows() {
        palette = PaletteWindow()
        let open = (Profile.defaults.array(forKey: "windows.open") as? [Int] ?? []).sorted()
        for i in open.isEmpty ? [0] : open { openWindow(index: i) }
        installMonitors()
        windows[0].show()
        current = windows[0]
        NSApp.activate()
    }

    /// ⌘⇧T: weiteres Fenster mit eigener Auswahl, eigenem Layout und Fokus; kleinste freie Nummer.
    @objc private func menuNewWindow() {
        let used = Set(windows.map(\.index))
        openWindow(index: (1...).first { !used.contains($0) }!)
    }

    private func openWindow(index: Int) {
        let c = MainWindowController(index: index, cascade: current?.window)
        c.window.delegate = self
        c.workspace.attach = attach
        c.sidebar.attach = attach
        c.workspace.otherInteractiveCount = current?.workspace.otherInteractiveCount ?? 0
        windows.append(c)
        current = c
        persistWindows()
        wire(c)
        if registry != nil { reloadViews() }
        c.show()
    }

    private func persistWindows() { Profile.defaults.set(windows.map(\.index).sorted(), forKey: "windows.open") }

    private func controller(for w: NSWindow?) -> MainWindowController? { windows.first { $0.window === w } }

    /// Zusatzfenster schließen: Sessions laufen weiter, seine Terminals werden frei für die anderen Fenster.
    private func closeWindow(_ c: MainWindowController) {
        windows.removeAll { $0 === c }
        if current === c { current = windows.last }
        c.workspace.close()
        for k in ["workspace.selected", "workspace.mode", "workspace.auto"] { Profile.defaults.removeObject(forKey: k + MainWindowController.suffix(c.index)) }
        persistWindows()
        syncSidebar()
    }

    /// Rückrufe von Baum, Arbeitsfläche und Leiste eines Fensters. Was nur das Fenster betrifft, geht an `c`,
    /// der Rest an Menü-Wege, die über `current` laufen (ein Klick hat das Fenster schon zum Key-Fenster gemacht).
    private func wire(_ c: MainWindowController) {
        let workspace = c.workspace, sidebar = c.sidebar, bar = c.bar

        workspace.onChange = { [weak self] in self?.syncSidebar() }
        // Terminal hier ausgehängt: andere Fenster, die es als Hinweis zeigen, dürfen es jetzt einhängen.
        workspace.onReleaseTerminal = { [weak self, weak c] in
            DispatchQueue.main.async { self?.windows.filter { $0 !== c }.forEach { $0.workspace.relayout() } }
        }
        workspace.onFocusChange = { [weak self] key in self?.attention.markSeen(key); self?.syncSidebar() }
        workspace.onActivate = { [weak self] key in if self?.attention.markSeen(key) == true { self?.syncSidebar() } }
        workspace.onCloseSession = { [weak self] key, force in self?.closeSession(key, force: force) }
        workspace.onEmptyClick = { [weak self] in self?.openNewSession(groupId: nil) }
        workspace.onRecheckCLI = { [weak self] in Task { await self?.recheckCLI() } }
        sidebar.onSelect = { [weak self, weak workspace] ids, mode in
            guard let self, let workspace else { return }
            // Eine einzelne Session anklicken quittiert ihre Marke „neu“, eine ganze Gruppe nicht.
            if ids.count == 1, attention.markSeen(ids[0]) { syncSidebar() }
            switch mode {
            case .replace: workspace.select(ids, add: false)
            case .toggle: workspace.select(ids, add: true)
            case .add: workspace.addMissing(ids)
            case .cursor: workspace.select(ids, add: false, takeKeyboard: false)
            }
        }
        sidebar.onNewSession = { [weak self] gid in self?.openNewSession(groupId: gid) }
        sidebar.onNewTerminal = { [weak self] gid in self?.openNewTerminal(groupId: gid) }
        sidebar.onDropFolder = { [weak self] dir in self?.startSession(group: nil, cwd: dir) }
        sidebar.onEditGroup = { [weak self] gid in self?.openEditGroup(gid) }
        sidebar.onToggleFavorite = { [weak self] gid in self?.store.toggleFavorite(id: gid); self?.reloadViews() }
        sidebar.onCloseGroup = { [weak self] gid, force in self?.closeGroup(gid, force: force) }
        sidebar.onCloseSession = { [weak self] key, force in self?.closeSession(key, force: force) }
        sidebar.onRenameSession = { [weak self] key in self?.renameSession(key) }
        workspace.onRenameSession = { [weak self] key in self?.renameSession(key) }
        sidebar.onMoveSession = { [weak self] id, target in self?.moveSession(id, to: target) }
        sidebar.onReorderFlat = { [weak self] order in Settings.sidebarFlatOrder = order; self?.reloadViews() }
        sidebar.onMoveGroup = { [weak self] gid, target in self?.store.moveGroup(gid, to: target); self?.reloadViews() }
        workspace.onMoveSession = { [weak self] id, target in self?.moveSession(id, to: target) }
        sidebar.onContextMenu = { [weak self] id in self?.sessionMenu(for: id) }
        workspace.onContextMenu = { [weak self] id in self?.sessionMenu(for: id) }
        workspace.onPickLanguage = { [weak self] language in
            Settings.language = language
            self?.buildMenu()
            self?.reloadViews()
        }
        bar.onPickLayout = { [weak workspace] m in workspace?.setMode(m) }
        bar.onGridColumns = { [weak workspace] c in workspace?.setGridColumns(c) }
        bar.onScrollColumns = { [weak workspace] c in workspace?.setScrollColumns(c) }
        bar.onSplit = { [weak workspace] c in workspace?.setSplit(c) }
        bar.onToggleZoom = { [weak workspace] in workspace?.toggleZen() }
        bar.onToggleAuto = { [weak workspace] in Feedback.play(.toggle); workspace?.toggleAuto() }
        bar.onToggleSync = { [weak workspace] in Feedback.play(.toggle); workspace?.toggleSync() }
        bar.onCycleSort = { [weak self] in self?.cycleSort() }
        bar.onToggleGrouping = { [weak self] in self?.toggleGrouping() }
        bar.onDismissTip = { [weak self] in self?.attention.tip = nil }
        bar.onSelectWaiting = { [weak self, weak workspace] in guard let self else { return }; workspace?.select(waitingIds(), add: false) }
        bar.onShowUpdate = { [weak self] in self?.showUpdateAvailable() }
        bar.onDetachIdle = { [weak self] in self?.detachIdleSessions() }
        bar.onSwitchAccount = { [weak self] id in self?.switchAccount(id) }
        bar.onAddAccount = { [weak self] in self?.addAccount() }
        bar.onManageAccounts = { [weak self] in self?.menuSettings() }
        bar.onToggleAutoswitch = { [weak self] in Settings.autoswitchEnabled.toggle(); self?.refreshAccountBars() }
        applyAccounts(to: c.bar)
    }

    /// Konto-Pille aller Fenster auf den aktuellen Stand bringen.
    private func refreshAccountBars() { for c in windows { applyAccounts(to: c.bar) } }

    private func applyAccounts(to bar: StatusBarView) {
        bar.accounts = accounts.menuItems
        bar.autoswitch = Settings.autoswitchEnabled
        bar.needsDisplay = true
    }

    /// Auf einen anderen Account umschalten (Global-Swap), danach die Nutzung sofort neu holen.
    private func switchAccount(_ id: String) { Task { if await accounts.switchTo(id) { usage.refreshNow() } } }

    /// Den gerade angemeldeten Account als Slot aufnehmen; scheitert, wenn keiner angemeldet ist.
    private func addAccount() {
        Task { [weak self] in
            guard let self else { return }
            if await self.accounts.addCurrent() { Feedback.play(.done) } else { self.warnNoAccount() }
        }
    }

    /// Nach einem automatischen Account-Wechsel einmal melden, welcher Account jetzt aktiv ist. Die
    /// „Nicht mehr anzeigen"-Checkbox (NSAlert-Suppression) schaltet die Meldung dauerhaft ab, wieder an
    /// in den Einstellungen. Als Sheet am Fenster, damit sie den Fokus paralleler Arbeit nicht stiehlt.
    private func notifyAutoSwitch(_ to: Account) {
        guard Settings.autoswitchNotice else { return }
        let alert = NSAlert()
        alert.messageText = String(localized: "Account automatisch gewechselt", bundle: Bundle.app)
        alert.informativeText = String(localized: "Jetzt aktiv: „\(to.title)“. Grund: Nutzungslimit erreicht.", bundle: Bundle.app)
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title = String(localized: "Nicht mehr anzeigen", bundle: Bundle.app)
        let apply = { if alert.suppressionButton?.state == .on { Settings.autoswitchNotice = false } }
        if let window = bar.window {
            alert.beginSheetModal(for: window) { _ in apply() }
        } else {
            alert.runModal()
            apply()
        }
    }

    /// Latch, damit die Pace-Warnung nur einmal pro Überschreitung kommt statt bei jedem Poll.
    private var paceWarned = false
    /// Nicht blockierende Warnung: liegt das 7-Tage-Limit über dem gleichmäßigen Wochen-Plan und erreicht bei diesem
    /// Tempo vor dem Reset 100 %, droht ein früher Lockout. Einmal melden, „Nicht mehr anzeigen" schaltet dauerhaft ab.
    private func notifyPaceIfNeeded(_ usage: Usage) {
        guard Settings.autoswitchPaceWarning, let lockout = usage.weeklyLockout(), let reset = usage.weeklyResets else {
            paceWarned = false
            return
        }
        guard !paceWarned else { return }
        paceWarned = true
        let when = lockout.formatted(.dateTime.weekday(.abbreviated).hour().minute())
        let until = reset.formatted(.dateTime.weekday(.abbreviated).hour().minute())
        let alert = NSAlert()
        alert.messageText = String(localized: "7-Tage-Limit läuft zu schnell voll", bundle: Bundle.app)
        alert.informativeText = String(localized: "Bei diesem Tempo ist das 7-Tage-Limit am \(when) voll. Reset ist aber erst am \(until), bis dahin wärst du gesperrt.", bundle: Bundle.app)
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title = String(localized: "Nicht mehr anzeigen", bundle: Bundle.app)
        let apply = { if alert.suppressionButton?.state == .on { Settings.autoswitchPaceWarning = false } }
        if let window = bar.window {
            alert.beginSheetModal(for: window) { _ in apply() }
        } else {
            alert.runModal()
            apply()
        }
    }

    private func warnNoAccount() {
        let alert = NSAlert()
        alert.messageText = String(localized: "Kein angemeldeter Claude-Account gefunden", bundle: Bundle.app)
        alert.informativeText = String(localized: "Melde dich zuerst in einer Session mit „claude“ an, dann versuch es erneut.", bundle: Bundle.app)
        alert.runModal()
    }

    /// Einmal für alle Fenster: Tasten wirken im Key-Fenster, Mausrad in dem Fenster unter der Maus.
    /// Die Handler halten den AppDelegate fest, der ohnehin so lange lebt wie die App (`main.swift`).
    private func installMonitors() {
        _ = NSEvent.addLocalMonitorForEvents(matching: .keyDown, handler: handleKey)
        _ = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown, handler: handleMouseDown)
        _ = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel, handler: handleScroll)
    }

    /// Belegbare Kürzel (Einstellungen) und F1 gehen vor, egal ob Terminal oder Fläche die Tastatur hat.
    /// Dialoge sind eigene Fenster und bekommen ihre Tasten unverändert.
    private func handleKey(_ event: NSEvent) -> NSEvent? {
        let code = event.keyCode, mods = event.modifierFlags.intersection(Hotkey.modMask)
        if code == KeyCode.f1, sheets.isPanel(.about, event.window) { sheets.dismiss(); return nil }
        if code == KeyCode.f3, sheets.isPanel(.stats, event.window) { sheets.dismiss(); return nil }
        guard controller(for: event.window) != nil else { return event }
        if code == KeyCode.escape, sheets.cancelVisible() { return nil }
        // Vorschau offen: ⏎ übernimmt die Session als Auswahl, Esc zeigt wieder die alte.
        if workspace.preview != nil, mods.isEmpty, [KeyCode.returnKey, KeyCode.keypadEnter, KeyCode.escape].contains(code) {
            endPreview(commit: code != KeyCode.escape)
            return nil
        }
        if let action = Hotkeys.action(for: event) { perform(action); return nil }
        // ⌘⏎: neue Session im Ordner der fokussierten. Vor dem Terminal abgefangen.
        if code == KeyCode.returnKey, mods == .command { newSessionInFocusedFolder(); return nil }
        if code == KeyCode.f1 { sheets.togglePanel(.about); return nil }
        if code == KeyCode.f3 { sheets.togglePanel(.stats); return nil }
        forwardSync(event, mods: mods)
        return event
    }

    /// Sync: dieselbe Taste an alle anderen Kacheln (siehe `forward`). ⌘V fügt überall ein, andere ⌘-Kürzel bleiben lokal.
    private func forwardSync(_ event: NSEvent, mods: NSEvent.ModifierFlags) {
        guard let src = window.firstResponder as? KadrellTerminalView else { return }
        for t in workspace.syncTargets(except: src) {
            if !mods.contains(.command) { KadrellTerminalView.forward(event, to: t) }
            else if mods == .command, event.charactersIgnoringModifiers == "v" { t.paste(self) }
        }
    }

    /// Klick in ein Terminal quittiert die Marke „neu“, auch wenn es schon die Tastatur hatte.
    private func handleMouseDown(_ event: NSEvent) -> NSEvent? {
        guard controller(for: event.window) != nil, !attention.unseen.isEmpty,
              var v = event.window?.contentView?.hitTest(event.locationInWindow) else { return event }
        while !(v is KadrellTerminalView), let up = v.superview { v = up }
        if let key = attach?.terminals.first(where: { $0.value === v })?.key, attention.markSeen(key) { syncSidebar() }
        return event
    }

    /// Layout Scrollen: seitliches Wischen (bzw. ⇧ + Mausrad) über der Arbeitsfläche verschiebt die Spalten, nicht das Terminal.
    /// ⌘ + Mausrad: Schriftgröße aller Terminals wie ⌘+/⌘-. Trackpad-Deltas sammeln, sonst springt es pro Wisch zweistellig.
    private func handleScroll(_ event: NSEvent) -> NSEvent? {
        guard let workspace = controller(for: event.window)?.workspace else { return event }
        if workspace.mode == .scroll, abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY),
           workspace.bounds.contains(workspace.convert(event.locationInWindow, from: nil)) {
            workspace.scrollBy(-event.scrollingDeltaX * (event.hasPreciseScrollingDeltas ? 1 : 10))
            return nil
        }
        guard event.modifierFlags.intersection(Hotkey.modMask) == .command else { return event }
        fontScrollAccum += event.hasPreciseScrollingDeltas ? event.scrollingDeltaY / 20 : event.scrollingDeltaY
        let steps = Int(fontScrollAccum)
        guard steps != 0 else { return nil }
        fontScrollAccum -= CGFloat(steps)
        Settings.terminalFontSize += Double(steps)
        applyAppearance()
        return nil
    }

    private func boot() async {
        cli = await ClaudeCLI.resolve()
        attention.fireFocusHook = { [cli] s in Hooks.fire(.sessionFocus, s, environment: cli!.environment) }
        attach = AttachManager(cli: cli)
        for c in windows { c.workspace.attach = attach; c.sidebar.attach = attach }
        attach.onChange = { [weak self] in self?.windows.forEach { $0.workspace.relayout() } }
        // Einstellung: Kachel einer beendeten Session schließen statt mit „Klick setzt fort" stehen zu lassen.
        attach.onEnded = { [weak self] key in
            guard let self else { return }
            // `exit` in einem Terminal ohne Claude: nichts fortzusetzen, Kachel und Eintrag weg.
            if workspace.session(key)?.isShell == true { closeSession(key, force: true); return }
            guard Settings.closeTileOnExit else { return }
            for c in windows where c.workspace.selected.contains(key) { c.workspace.select([key], add: true) }
            syncSidebar()
        }
        registry = SessionRegistry(cli: cli)
        registry.pids = { [weak attach] in attach?.pids ?? [:] }
        registry.onChange = { [weak self] sessions in self?.sessionsChanged(sessions) }
        // Fenster versteckt (Menüleisten-Betrieb) oder App im Hintergrund: seltener pollen, siehe `updatePollBackground`.
        // Aktiviert sich die App wieder und steht noch der alte Fehler (claude fehlt/zu alt), gleich nochmal prüfen:
        // ohne das bleibt „claude nicht gefunden“ auch nach einer Installation bis zum Neustart stehen.
        for name in [NSApplication.didBecomeActiveNotification, NSApplication.didResignActiveNotification] {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.updatePollBackground() }
                guard name == NSApplication.didBecomeActiveNotification else { return }
                MainActor.assumeIsolated { self?.recheckIfBroken() }
            }
        }
        NotificationCenter.default.addObserver(forName: NSWindow.didChangeOcclusionStateNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.updatePollBackground() }
        }
        await recheckCLI()
        // Leer nicht abgleichen: das würde Gruppen alter Hintergrund-Sessions verwerfen, bevor sie übernommen sind.
        if !registry.sessions.isEmpty { sessionsChanged(registry.sessions) }
        Task {
            await registry.pollNow()
            await offerAdopt()
            registry.start()
        }
        startControlServer()
        usage.onChange = { [weak self] u in
            guard let self else { return }
            self.bar.usage = u
            self.bar.needsDisplay = true
            self.accounts.recordUsage(u)
            self.accounts.considerAutoSwitch(u)
            self.notifyPaceIfNeeded(u)
        }
        if let cached = accounts.active?.usage {
            let seeded = cached.asUsage()
            usage.seed(seeded, at: cached.at)
            bar.usage = seeded
            bar.needsDisplay = true
        }
        usage.start()
        accounts.onChange = { [weak self] in self?.refreshAccountBars() }
        accounts.onAutoSwitch = { [weak self] to in self?.notifyAutoSwitch(to) }
        updateChecker.onChange = { [weak self] m in self?.bar.updateAvailable = m; self?.bar.needsDisplay = true }
        updateChecker.start()
        Notifications.setup()
        Notifications.onSelect = { [weak self] key in self?.focusSession(key) }
        // Minutengenau reicht; die Einstellung liest jeder Durchlauf neu, ein Wechsel wirkt also ohne Neustart.
        autoDetachTask = Task { [weak self] in
            while !Task.isCancelled {
                self?.autoDetachIdleSessions()
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    /// Fenster verdeckt/versteckt oder App nicht aktiv: `SessionRegistry` seltener pollen lassen.
    private func updatePollBackground() {
        registry?.setBackground(!NSApp.isActive || !windows.contains { $0.window.isVisible })
    }

    /// claude fehlt oder `claude --version` liefert nichts Brauchbares: blockierender Fehler, der Leerzustand
    /// bietet „Erneut prüfen“ genau hierauf. Zu alt ist keine Blockade mehr (siehe `ClaudeCLI.VersionCheck`),
    /// nur eine Warnung in der Leiste, solange `claude agents` trotzdem läuft.
    func recheckCLI() async {
        guard let cli else { return }
        guard FileManager.default.isExecutableFile(atPath: cli.binary) else {
            registry.fail(String(localized: "\(cli.binary): claude nicht gefunden", bundle: Bundle.app), missingBinary: true)
            versionWarning = nil
            reloadViews()
            return
        }
        switch await cli.checkVersion() {
        case .failed(let message): registry.fail(message); versionWarning = nil
        case .tooOld(let message): registry.clearError(); versionWarning = message; await registry.pollNow()
        case .ok: registry.clearError(); versionWarning = nil; await registry.pollNow()
        }
        reloadViews()
    }

    private func recheckIfBroken() {
        guard registry?.lastError != nil else { return }
        Task { await self.recheckCLI() }
    }

    private func sessionsChanged(_ sessions: [Session]) {
        store.assign(sessions)
        attach.sync(with: sessions)
        reloadViews()
        attach.enqueue(([current] + windows.filter { $0 !== current }).flatMap { $0.workspace.shownSessions() })
    }

    /// Fremde Claude-Sessions (andere Terminals, frühere Kadrell-Versionen) beim Start erfassen. Hintergrund-Sessions
    /// aus `claude --bg` bietet es zur Übernahme an: `claude stop` hält sie an, danach setzt Kadrell sie als eigenen
    /// Prozess mit `--resume` fort. Die Kurz-Id bleibt Schlüssel, damit Gruppen und Auswahl passen. Kein `claude rm`:
    /// das löscht ggf. den Worktree, in dem die Session arbeitet. Interaktive Sessions (tmux, iTerm) laufen noch, die
    /// zeigt nur der Leerzustand als Hinweis (Übernahme dort per `tools/tmux-dump.py`, das startet sichere Kopien).
    private func offerAdopt() async {
        let owned = Set(registry.sessions.map(\.sessionId))
        let agents: [Agent]
        do { agents = try await cli.agents() } catch {
            AppDelegate.log.error("agents: \(String(describing: error), privacy: .private)")
            registry.fail("\(cli.binary): \(CLIError.firstLine(of: error))")
            return
        }
        let elsewhere = agents.filter { !owned.contains($0.sessionId) }
        for c in windows { c.workspace.otherInteractiveCount = elsewhere.filter { $0.kind == "interactive" }.count }
        // Übernahme in die Sessions-Liste nur das Standardprofil, sonst stiehlt ein Testprofil sie.
        guard Profile.name == nil else { return }
        let bg = elsewhere.filter(\.isRunningBackground)
        guard !bg.isEmpty else { return }
        let busy = bg.contains { $0.status == "busy" }
        let list = bg.map { "· \($0.name)\($0.status == "busy" ? String(localized: " (arbeitet gerade)", bundle: Bundle.app) : "")" }.joined(separator: "\n")
        // Arbeitende Sessions per blankem ⏎ zu stoppen wäre destruktiv (Kanboard #16): sobald eine busy ist, gilt ⌘⏎ wie überall sonst.
        sheets.confirm(String(localized: "\(bg.count) Hintergrund-Session(s) übernehmen?", bundle: Bundle.app), String(localized: "Kadrell startet Claude jetzt selbst statt mit claude --bg. Diese Sessions werden mit claude stop angehalten (laufende Arbeit bricht ab) und hier fortgesetzt:", bundle: Bundle.app) + "\n\(list)", button: String(localized: "Übernehmen", bundle: Bundle.app), destructive: busy, ask: .adoptBackground) { [weak self] in
            guard let self else { return }
            Task {
                var failures: [String] = []
                for a in bg {
                    guard let id = a.shortId else { continue }
                    do { try await self.cli.stop(id: id) } catch { failures.append("· \(a.name): \(CLIError.firstLine(of: error))"); continue }
                    self.registry.add(Session(id: id, cwd: a.cwd, startedAt: a.startedAt, sessionId: a.sessionId,
                                              name: Session.isAutoName(a.name, cwd: a.cwd) ? "" : a.name))
                }
                // Eine Meldung für alle Fehlschläge statt einer je Session, die sich sonst gegenseitig verdrängen (#73).
                if !failures.isEmpty {
                    self.sheets.report(failures.joined(separator: "\n"), title: String(localized: "\(failures.count) Session(s) konnten nicht übernommen werden", bundle: Bundle.app))
                }
            }
        }
    }

    func reloadViews() {
        for c in windows {
            c.workspace.lastError = registry?.lastError
            c.workspace.lastErrorIsMissingBinary = registry?.lastErrorIsMissingBinary ?? false
            c.workspace.polled = registry?.polled ?? false
            c.workspace.reload(groups: store.groups, sessions: registry?.sessions ?? [])
        }
        syncSidebar()
    }

    /// Baum und Leiste jedes Fensters folgen seiner Arbeitsfläche (Auswahl, Fokus, Layout). Hook, Dock, Sounds und
    /// Marke „neu“ (`AttentionTracker`) gelten global und folgen dem Fokus des Key-Fensters.
    private func syncSidebar() {
        guard current != nil else { return }
        let sessions = workspace.sessions
        attention.prune(to: sessions)
        let waiting = waitingIds()
        for c in windows { syncWindow(c, waiting: waiting.count) }
        let before = attention.unseen
        let changed = attention.update(sessions: sessions, waiting: waiting,
                                       statuses: sessions.filter { attach?.isAttached($0.key) ?? false }.mapValues(\.status),
                                       focused: workspace.focused, looking: workspace.visibleTiles,
                                       isKey: window?.isKeyWindow != false)
        for c in windows {
            if attention.unseen != before { c.sidebar.unread = attention.unseen; c.sidebar.needsDisplay = true }
            c.sidebar.flash(waiting: changed.waiting, done: changed.done)
        }
        statusItem?.update(Array(sessions.values), unseen: attention.unseen)
    }

    private func syncWindow(_ c: MainWindowController, waiting: Int) {
        let workspace = c.workspace, sidebar = c.sidebar, bar = c.bar
        sidebar.selected = Set(workspace.selected)
        sidebar.focused = workspace.preview ?? workspace.focused
        sidebar.showMessages = Settings.showLastMessage
        sidebar.showAge = Settings.sidebarShowAge
        sidebar.sort = Settings.sidebarSort
        sidebar.grouped = Settings.sidebarGrouped
        sidebar.flatOrder = Settings.sidebarFlatOrder
        sidebar.renderer = Settings.sidebarStyle.renderer
        sidebar.messages = registry?.lastMessages ?? [:]
        sidebar.unread = attention.unseen
        sidebar.reload(groups: store.groups, sessions: Array(workspace.sessions.values))
        let sessions = workspace.sessions
        let focused = (workspace.preview ?? workspace.focused).flatMap { sessions[$0] }
        let fg = focused.flatMap { workspace.group(forSession: $0.id) }
        bar.crumb = focused.map { (fg?.name ?? "", $0.title) }
        bar.crumbGroupAttrs = fg.map { Theme.attrs(11.5, Theme.group($0.color)) }
        bar.errorText = registry?.lastError
        bar.versionWarning = versionWarning
        bar.tip = attention.tip
        bar.sessionCount = sessions.count
        bar.openCount = workspace.selected.count
        bar.layoutMode = workspace.mode
        bar.gridColumns = Settings.gridColumns
        bar.scrollColumns = Settings.scrollColumns
        bar.split = workspace.focusedSplit
        bar.zoomed = workspace.zen
        bar.auto = workspace.auto
        bar.sync = workspace.sync
        bar.sort = sidebar.sort
        bar.grouped = sidebar.grouped
        bar.counts = statusCounts(sessions)
        bar.detachableCount = detachableIdleIds().count
        bar.waitingCount = waiting
        applyAccounts(to: bar)
        bar.needsDisplay = true
    }

    /// Wartende Sessions in Baumreihenfolge, unabhängig von eingeklappten Gruppen: `waitingFor` kommt nur bei
    /// laufendem eigenen Prozess (siehe `SessionRegistry.merge`), `isAttached` ist die zusätzliche Absicherung.
    private func waitingIds() -> [String] {
        let sessions = workspace.sessions
        return store.groups.flatMap(\.sessionIds).filter { sessions[$0]?.status == .waiting && (attach?.isAttached($0) ?? false) }
    }

    /// Zahlen für die Leiste: Sessions mit laufendem Claude-Prozess nach Status, alle anderen gelten als getrennt.
    private func statusCounts(_ sessions: [String: Session]) -> StatusCounts {
        var c = StatusCounts()
        for (key, s) in sessions {
            guard attach?.isAttached(key) == true else { c.detached += 1; continue }
            switch s.status {
            case .running: c.running += 1
            case .waiting: c.waiting += 1
            case .idle: c.idle += 1
            case .error: c.error += 1
            }
        }
        return c
    }

    /// Fertige Sessions, deren Prozess sich trennen lässt. Terminals ohne Claude bleiben außen vor (SIGHUP beendet
    /// die Shell samt allem darin), Remote-Kacheln ebenso: deren ssh steht ohnehin fast immer auf grün.
    private func detachableIdleIds() -> [String] {
        workspace.sessions.values
            .filter { $0.status == .idle && !$0.isShell && !$0.isRemote && attach?.isAttached($0.id) == true }
            .map(\.id)
    }

    /// Grüne Pille in der Leiste: alle fertigen Sessions trennen. Die Kacheln bleiben mit ihrem letzten Bildschirm
    /// stehen, ein Klick darauf setzt die Konversation fort.
    private func detachIdleSessions() {
        let ids = detachableIdleIds()
        guard !ids.isEmpty else { NSSound.beep(); return }
        Feedback.play(.close)
        for id in ids { attach.stop(id) }
        syncSidebar()
    }

    /// Einstellung „Fertige Sessions automatisch trennen“: was länger als die eingestellten Minuten grün ist,
    /// verliert seinen Prozess. Die gerade fokussierten Sessions bleiben verbunden, in denen liest oder tippt man.
    /// `idleSince` wird auch bei abgeschalteter Einstellung gepflegt, sonst trennt das Einschalten sofort alles.
    private func autoDetachIdleSessions() {
        let ids = Set(detachableIdleIds()), now = Date()
        idleSince = idleSince.filter { ids.contains($0.key) }
        for id in ids where idleSince[id] == nil { idleSince[id] = now }
        let minutes = Settings.autoDetachMinutes
        guard minutes > 0 else { return }
        let focused = Set(windows.compactMap { $0.workspace.focused })
        let due = ids.filter { !focused.contains($0) && now.timeIntervalSince(idleSince[$0] ?? now) >= Double(minutes) * 60 }
        guard !due.isEmpty else { return }
        AppDelegate.log.info("automatisch getrennt nach \(minutes) min: \(due.count, privacy: .public) Session(s)")
        for id in due { attach.stop(id) }
        syncSidebar()
    }

    // MARK: Menü

    private func buildMenu() {
        let main = NSMenu()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: String(localized: "Über Kadrell", bundle: Bundle.app), action: #selector(menuAbout), keyEquivalent: "")
        appMenu.addItem(withTitle: String(localized: "Was ist neu", bundle: Bundle.app), action: #selector(menuWhatsNew), keyEquivalent: "")
        appMenu.addItem(withTitle: String(localized: "Statistik", bundle: Bundle.app), action: #selector(menuStats), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: String(localized: "Einstellungen …", bundle: Bundle.app), action: #selector(menuSettings), keyEquivalent: ",")
        appMenu.addItem(withTitle: String(localized: "Kommandozeilen-Tool installieren …", bundle: Bundle.app), action: #selector(menuInstallCLI), keyEquivalent: "")
        appMenu.addItem(withTitle: String(localized: "Neue Instanz mit temporärem Profil", bundle: Bundle.app), action: #selector(menuTemporaryInstance), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: String(localized: "Kadrell ausblenden", bundle: Bundle.app), action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: String(localized: "Kadrell beenden", bundle: Bundle.app), action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        main.addItem(withTitle: "Kadrell", action: nil, keyEquivalent: "").submenu = appMenu

        let file = NSMenu(title: String(localized: "Datei", bundle: Bundle.app))
        file.addItem(withTitle: String(localized: "Neue Session", bundle: Bundle.app), action: #selector(menuNewSession), keyEquivalent: "n")
        file.addItem(withTitle: String(localized: "Neue Session im selben Ordner", bundle: Bundle.app), action: #selector(menuNewSessionHere), keyEquivalent: "\r")
        file.addItem(withTitle: String(localized: "Neues Terminal ohne Claude", bundle: Bundle.app), action: #selector(menuNewShell), keyEquivalent: "t")
        let remote = file.addItem(withTitle: String(localized: "Remote verbinden …", bundle: Bundle.app), action: #selector(menuRemote), keyEquivalent: "n")
        remote.keyEquivalentModifierMask = [.command, .shift]
        file.addItem(.separator())
        let newWindow = file.addItem(withTitle: String(localized: "Neues Fenster", bundle: Bundle.app), action: #selector(menuNewWindow), keyEquivalent: "t")
        newWindow.keyEquivalentModifierMask = [.command, .shift]
        let closeWindow = file.addItem(withTitle: String(localized: "Fenster schließen", bundle: Bundle.app), action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        closeWindow.keyEquivalentModifierMask = [.command, .shift]
        file.addItem(.separator())
        file.addItem(withTitle: String(localized: "Session schließen", bundle: Bundle.app), action: #selector(menuCloseSession), keyEquivalent: "w")
        main.addItem(withTitle: String(localized: "Datei", bundle: Bundle.app), action: nil, keyEquivalent: "").submenu = file

        let edit = NSMenu(title: String(localized: "Bearbeiten", bundle: Bundle.app))
        edit.addItem(withTitle: String(localized: "Ausschneiden", bundle: Bundle.app), action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: String(localized: "Kopieren", bundle: Bundle.app), action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: String(localized: "Einsetzen", bundle: Bundle.app), action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: String(localized: "Ganze Gruppen auswählen", bundle: Bundle.app), action: #selector(menuSelectAll), keyEquivalent: "a")
        let selectEverything = NSMenuItem(title: String(localized: "Alle Sessions auswählen", bundle: Bundle.app), action: #selector(menuSelectEverything), keyEquivalent: "a")
        selectEverything.keyEquivalentModifierMask = [.command, .shift]
        edit.addItem(selectEverything)
        edit.addItem(.separator())
        // Suchleiste von SwiftTerm im Terminal mit der Tastatur, über die Responder-Kette.
        for (title, key, shift, action) in [(String(localized: "Im Terminal suchen …", bundle: Bundle.app), "f", false, NSTextFinder.Action.showFindInterface),
                                            (String(localized: "Weitersuchen", bundle: Bundle.app), "g", false, .nextMatch), (String(localized: "Rückwärts suchen", bundle: Bundle.app), "g", true, .previousMatch)] {
            let item = NSMenuItem(title: title, action: #selector(NSResponder.performTextFinderAction(_:)), keyEquivalent: key)
            item.keyEquivalentModifierMask = shift ? [.command, .shift] : .command
            item.tag = action.rawValue
            edit.addItem(item)
        }
        let findAll = NSMenuItem(title: String(localized: "In allen Terminals suchen …", bundle: Bundle.app), action: #selector(menuFindAll), keyEquivalent: "f")
        findAll.keyEquivalentModifierMask = [.command, .shift]
        edit.addItem(findAll)
        main.addItem(withTitle: String(localized: "Bearbeiten", bundle: Bundle.app), action: nil, keyEquivalent: "").submenu = edit

        let view = NSMenu(title: String(localized: "Ansicht", bundle: Bundle.app))
        for m in LayoutMode.allCases {
            let item = NSMenuItem(title: m.title, action: #selector(menuLayout(_:)), keyEquivalent: "")
            item.representedObject = m.rawValue
            view.addItem(item)
        }
        view.addItem(withTitle: String(localized: "Auto-Modus ein/aus", bundle: Bundle.app), action: #selector(menuAuto), keyEquivalent: "")
        view.addItem(withTitle: String(localized: "Baum ein/aus", bundle: Bundle.app), action: #selector(menuSidebar), keyEquivalent: "b")
        let toggleGroups = NSMenuItem(title: String(localized: "Alle Gruppen auf-/zuklappen", bundle: Bundle.app), action: #selector(menuToggleGroups), keyEquivalent: "b")
        toggleGroups.keyEquivalentModifierMask = [.command, .shift]
        view.addItem(toggleGroups)
        view.addItem(.separator())
        view.addItem(withTitle: String(localized: "Terminal-Schrift größer", bundle: Bundle.app), action: #selector(menuFontBigger), keyEquivalent: "+")
        view.addItem(withTitle: String(localized: "Terminal-Schrift kleiner", bundle: Bundle.app), action: #selector(menuFontSmaller), keyEquivalent: "-")
        view.addItem(withTitle: String(localized: "Terminal-Schrift Standardgröße", bundle: Bundle.app), action: #selector(menuFontReset), keyEquivalent: "0")
        view.addItem(.separator())
        // Belegbare Kürzel: das Menü zeigt die aktuelle Belegung, ausgelöst werden sie im Event-Monitor.
        let keys = Hotkeys.current
        let tiles = NSMenu(title: String(localized: "Kachel wählen", bundle: Bundle.app))
        for a in HotkeyAction.allCases {
            let item = NSMenuItem(title: a.title, action: #selector(menuHotkey(_:)), keyEquivalent: keys[a]?.menuEquivalent ?? "")
            item.keyEquivalentModifierMask = keys[a]?.flags ?? []
            item.representedObject = a.rawValue
            if a.tileIndex != nil { tiles.addItem(item) } else { view.addItem(item) }
            if a == .lastSession { view.addItem(withTitle: String(localized: "Kachel wählen", bundle: Bundle.app), action: nil, keyEquivalent: "").submenu = tiles }
        }
        view.addItem(.separator())
        view.addItem(withTitle: String(localized: "Suche", bundle: Bundle.app), action: #selector(menuPalette), keyEquivalent: "p")
        main.addItem(withTitle: String(localized: "Ansicht", bundle: Bundle.app), action: nil, keyEquivalent: "").submenu = view

        let session = NSMenu(title: String(localized: "Session", bundle: Bundle.app))
        for item in sessionMenuItems(for: nil, shortcuts: true) { session.addItem(item) }
        main.addItem(withTitle: String(localized: "Session", bundle: Bundle.app), action: nil, keyEquivalent: "").submenu = session

        let windows = NSMenu(title: String(localized: "Fenster", bundle: Bundle.app))
        windows.addItem(withTitle: String(localized: "Im Dock ablegen", bundle: Bundle.app), action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windows.addItem(withTitle: String(localized: "Zoomen", bundle: Bundle.app), action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        main.addItem(withTitle: String(localized: "Fenster", bundle: Bundle.app), action: nil, keyEquivalent: "").submenu = windows
        NSApp.windowsMenu = windows
        NSApp.mainMenu = main
    }

    /// ⌘N öffnet immer den Gruppen-Dialog, auch mit Fokus im Baum. Ohne Dialog startet nur ⌘⏎ (oder das Plus im Baum).
    @objc private func menuNewSession() { openNewSession(groupId: nil) }
    @objc private func menuNewSessionHere() { newSessionInFocusedFolder() }
    /// Ohne fokussierte Session gibt es keinen Ordner: dann wie ⌘N.
    private func newSessionInFocusedFolder() {
        guard let s = workspace.focused.flatMap({ workspace.session($0) }), !s.cwd.isEmpty else { openNewSession(groupId: nil); return }
        startSession(group: workspace.group(forSession: s.id), cwd: s.cwd)
    }
    /// ⌘⇧N: Palette im Remote-Modus, `@host` verbindet per ssh mit dessen tmux, `@host:` wählt die Session.
    @objc private func menuRemote() { togglePalette(prefix: "@") }

    /// Remote-Kachel: Gruppe je Host, der Schlüssel trägt das ssh-Präfix wie Shells das ihre.
    private func startRemote(host: String, tmuxSession: String?) {
        let g = store.group(forHost: host) ?? { let g = store.makeGroup(host: host); store.add(g); return g }()
        let id = Session.remotePrefix + UUID().uuidString.lowercased()
        store.attach(sessionId: id, to: g.id)
        SSHConfig.recordUse(host)
        var session = Session(id: id, cwd: g.cwd, startedAt: Date().timeIntervalSince1970 * 1000, sessionId: id, name: "")
        session.host = host
        session.tmuxSession = tmuxSession
        registry.add(session)
        workspace.select([id], add: !workspace.selected.isEmpty)
    }

    /// ⌘T: Login-Shell im Ordner der fokussierten Session, sonst im Startordner.
    @objc private func menuNewShell() {
        startShell(group: nil, cwd: workspace.focused.flatMap { workspace.session($0) }?.cwd ?? Settings.startFolder)
    }

    /// Terminal ohne Claude, erkennbar am Schlüssel-Präfix.
    private func startShell(group: Group?, cwd: String) {
        startSession(group: group, cwd: cwd, sessionId: Session.newShellId())
    }
    @objc private func menuAbout() { sheets.togglePanel(.about) }
    @objc private func menuStats() { sheets.togglePanel(.stats) }
    @objc private func menuWhatsNew() { showWhatsNew(version: Settings.version, fallback: String(localized: "Keine Einträge gefunden.", bundle: Bundle.app)) }
    @objc private func menuTemporaryInstance() { Profile.launchTemporary() }
    @objc private func menuSettings() {
        let model = SettingsModel()
        model.accounts = accounts
        model.onApply = { [weak self] in
            guard let self else { return }
            buildMenu()
            applyAppearance()
            Task { await self.registry?.pollNow() }
            Task { await self.updateChecker.checkNow() }
        }
        model.onClose = { [weak self, weak model] in model?.stopRecording(); self?.sheets.dismiss() }
        sheets.present(SettingsView(model: model), onCancel: { model.onClose?() }, onPrimary: { model.onClose?() })
    }
    @objc private func menuFontBigger() { Settings.terminalFontSize += 1; applyAppearance() }
    @objc private func menuFontSmaller() { Settings.terminalFontSize -= 1; applyAppearance() }
    @objc private func menuFontReset() { Settings.terminalFontSize = Settings.defaultFontSize; applyAppearance() }

    /// Darstellung aus den Einstellungen übernehmen, ohne Neustart: Leiste, Baum, Kacheln, Terminals, Palette.
    private func applyAppearance() {
        let themeChanged = Theme.current.id != Settings.colorTheme
        if themeChanged { Theme.current = ColorTheme.named(Settings.colorTheme) }
        // Die Palette baut ihre Texte beim Erzeugen: nach einem Sprachwechsel muss sie neu entstehen.
        let languageChanged = paletteLanguage != Settings.language
        if themeChanged || languageChanged || Theme.scale != CGFloat(Settings.uiScale) {
            Theme.scale = CGFloat(Settings.uiScale)
            if palette.isVisible { palette.dismiss() }
            palette = PaletteWindow()
            paletteLanguage = Settings.language
        }
        for c in windows { c.applyAppearance(themeChanged: themeChanged) }
        if themeChanged { sheets.applyTheme() }
        attach?.applyTerminalSettings()
        reloadViews()
    }
    /// ⌘A: in einem Textfeld die übliche Textauswahl, sonst alle Sessions der Gruppen, in denen schon etwas ausgewählt ist.
    @objc private func menuSelectAll() {
        if let t = NSApp.keyWindow?.firstResponder as? NSText { t.selectAll(nil); return }
        let chosen = Set(workspace.selected)
        toggleSelection(store.groups.filter { !chosen.isDisjoint(with: $0.sessionIds) }.flatMap(\.sessionIds), shift: false)
    }
    /// ⌘⇧A: alle Sessions aller Gruppen.
    @objc private func menuSelectEverything() { toggleSelection(store.groups.flatMap(\.sessionIds), shift: true) }

    /// Nochmal dasselbe Kürzel, solange die Auswahl unverändert ist: zurück zur Auswahl davor.
    private func toggleSelection(_ ids: [String], shift: Bool) {
        if let u = selectAllUndo, u.shift == shift, u.after == Set(workspace.selected) {
            selectAllUndo = nil
            workspace.select(u.before, add: false)
            if let f = u.focus { workspace.setFocus(f) }
            return
        }
        guard !ids.isEmpty else { NSSound.beep(); return }
        let before = workspace.selected, focus = workspace.focused
        workspace.select(ids, add: false)
        selectAllUndo = (shift, before, focus, Set(workspace.selected))
    }
    @objc private func menuLayout(_ item: NSMenuItem) {
        if let m = (item.representedObject as? String).flatMap(LayoutMode.init) { workspace.setMode(m) }
    }
    @objc private func menuAuto() { workspace.toggleAuto() }
    @objc private func menuSidebar() {
        sidebarScroll.isHidden.toggle()
        split.adjustSubviews()
        split.needsDisplay = true   // sonst bleibt der alte Trenner als Linie stehen, wo der Baum endete
    }
    @objc private func menuToggleGroups() { sidebar.toggleAllGroups() }
    @objc private func menuHotkey(_ sender: NSMenuItem) {
        if let raw = sender.representedObject as? String, let a = HotkeyAction(rawValue: raw) { perform(a) }
    }

    /// Kürzel der Arbeitsfläche erledigt `WorkspaceView.perform`, hier bleiben die mit Baum, Vorschau oder Dialog.
    private func perform(_ action: HotkeyAction) {
        if action != .previewNext, action != .previewPrev { endPreview(commit: false) }
        guard !workspace.perform(action) else { return }
        switch action {
        case .previewNext: stepPreview(1)
        case .previewPrev: stepPreview(-1)
        case .nextWaiting: focusNextWaiting()
        case .focusSidebar: focusSidebar()
        case .focusWorkspace: focusWorkspace()
        case .openEditor: openEditor()
        case .renameSession: if let key = workspace.focused { renameSession(key) } else { NSSound.beep() }
        case .cycleSort: cycleSort()
        case .toggleGrouping: toggleGrouping()
        default: break
        }
    }

    private func cycleSort() { Settings.sidebarSort = Settings.sidebarSort.next; syncSidebar() }
    private func toggleGrouping() { Settings.sidebarGrouped.toggle(); Feedback.play(.toggle); syncSidebar() }

    /// Springt zur nächsten Session, die wartet, egal ob ihre Gruppe eingeklappt oder ihre Kachel schon offen ist.
    private func focusNextWaiting() {
        let ids = waitingIds()
        guard !ids.isEmpty else { NSSound.beep(); return }
        reveal(workspace.focused.flatMap { ids.firstIndex(of: $0) }.map { ids[($0 + 1) % ids.count] } ?? ids[0])
    }

    /// Tastatur-Besitzer vor der Vorschau: Esc gibt sie ihm zurück.
    private weak var previewResponder: NSResponder?

    private func stepPreview(_ step: Int) {
        if workspace.preview == nil { previewResponder = window.firstResponder }
        guard let key = sidebar.sessionId(after: workspace.preview ?? workspace.focused, step: step) else { NSSound.beep(); return }
        workspace.setPreview(key)
        sidebar.reveal(key)
    }

    private func endPreview(commit: Bool) {
        guard let key = workspace.preview else { return }
        if commit { workspace.select([key], add: false); return }
        workspace.setPreview(nil)
        if let r = previewResponder as? NSView, r.window === window { window.makeFirstResponder(r) }
        else if let f = workspace.focused { workspace.setFocus(f) }
    }

    /// Ordner der Fokus-Session im eingestellten Editor öffnen. Über die Login-Shell, damit `code` und Co. im PATH liegen.
    private func openEditor() {
        let cmd = Settings.editorCommand.trimmingCharacters(in: .whitespacesAndNewlines)
        let cwd = workspace.focused.flatMap { workspace.session($0) }?.cwd ?? ""
        guard !cmd.isEmpty, !cwd.isEmpty else { NSSound.beep(); return }
        do { _ = try ProcessRunner.spawn("/bin/zsh", ["-lc", cmd + " ."], environment: cli?.environment, cwd: cwd) }
        catch { sheets.report(String(describing: error)) }
    }

    private func focusSidebar() {
        if sidebarScroll.isHidden { menuSidebar() }
        window.makeFirstResponder(sidebar)
    }

    /// Fokussierte Kachel bekommt die Tastatur; ist rechts nichts offen, die erste Session im Baum.
    private func focusWorkspace() {
        if let f = workspace.focused { workspace.setFocus(f); return }
        let first = store.groups.flatMap(\.sessionIds).first { workspace.session($0) != nil }
        workspace.select(first.map { [$0] } ?? [], add: false)
    }

    @objc private func menuPalette() { togglePalette() }
    @objc private func menuFindAll() { togglePalette(prefix: "/") }
    /// ⌘W: fokussierte Session beenden und entfernen, wie das X an der Kachel. Das Fenster bleibt offen.
    @objc private func menuCloseSession() {
        guard NSApp.keyWindow === window, let key = workspace.focused else { NSSound.beep(); return }
        closeSession(key)
    }

    // MARK: Palette

    private func togglePalette(prefix: String = "") {
        if palette.isVisible {
            guard !prefix.isEmpty else { palette.switchToCommandMode(); return }
            palette.dismiss(runHighlightReset: false)
        }
        sheets.dismiss()
        // Kein Hintergrund-Timer mehr: Snapshots erst hier auf den aktuellen Stand bringen, direkt vorm Zeigen.
        attach?.refreshSnapshots()
        var src = PaletteWindow.Source()
        let sessions = workspace.sessions
        let bufferSessions = store.groups.flatMap { g in g.sessionIds.compactMap { sessions[$0] }.map { ($0, group: g, lines: attach?.lines(for: $0.id) ?? []) } }
        src.sessions = bufferSessions
        src.groups = store.groups
        // getBufferAsData + UTF-8-Dekodierung je Terminal kostet spürbar: erst holen, wenn die `/`-Suche sie
        // tatsächlich braucht (siehe `PaletteWindow.terminalMatches`), nicht bei jedem Öffnen der Palette.
        src.buffers = { [weak self] in
            guard let self else { return [] }
            return bufferSessions.compactMap { s, g, _ in
                self.attach.terminal(for: s.id).map { (s, group: g, lines: String(decoding: $0.getBufferAsData(kind: .active), as: UTF8.self).components(separatedBy: "\n")) }
            }
        }
        src.onFindInSession = { [weak self] key, term, index in self?.findInSession(key, term: term, index: index) }
        // Liest `~/.ssh/config` samt Include-Globs: erst beim `@`-Modus, nicht bei jedem Öffnen.
        src.hosts = { SSHConfig.recent + SSHConfig.hosts().filter { !SSHConfig.recent.contains($0) } }
        src.onConnect = { [weak self] host, name in self?.startRemote(host: host, tmuxSession: name) }
        src.remoteSessions = { [weak self] host, done in
            guard let env = self?.attach.cli.environment else { done(nil); return }
            Task { done(await SSHConfig.tmuxSessions(host: host, environment: env)) }
        }
        src.onFocusSession = { [weak self] key in self?.workspace.select([key], add: false) }
        src.onFitGroup = { [weak self] gid in
            guard let self, let g = store.group(id: gid) else { return }
            workspace.select(g.sessionIds, add: false)
        }
        let focusedSession = workspace.focused.flatMap { sessions[$0] }
        var commands: [(String, () -> Void)] = LayoutMode.allCases.map { m in (String(localized: "Layout: \(m.title)", bundle: Bundle.app), { [weak self] in self?.workspace.setMode(m) }) }
        commands += [
            (String(localized: "Trennlinien zurücksetzen (gleich verteilt)", bundle: Bundle.app), { [weak self] in self?.workspace.resetRatios() }),
            (String(localized: "Zoom ein/aus (fokussierte)", bundle: Bundle.app), { [weak self] in self?.workspace.toggleZen() }),
            (String(localized: "Neue Session", bundle: Bundle.app), { [weak self] in self?.openNewSession(groupId: nil) }),
            (String(localized: "Neues Terminal ohne Claude", bundle: Bundle.app), { [weak self] in self?.menuNewShell() }),
            (String(localized: "Remote verbinden (ssh → tmux)", bundle: Bundle.app), { [weak self] in self?.menuRemote() }),
            (String(localized: "Session stoppen (fokussierte)", bundle: Bundle.app), { [weak self] in if let s = focusedSession { self?.stopSession(s) } }),
            (String(localized: "Session fortsetzen (fokussierte)", bundle: Bundle.app), { [weak self] in if let s = focusedSession { self?.attach.attachNow(s); self?.workspace.select([s.id], add: false) } }),
            (String(localized: "Session umbenennen (fokussierte)", bundle: Bundle.app), { [weak self] in if let s = focusedSession { self?.renameSession(s.id) } }),
            (String(localized: "Session schließen (fokussierte)", bundle: Bundle.app), { [weak self] in if let s = focusedSession { self?.closeSession(s.id) } }),
            (String(localized: "Gruppe bearbeiten (der fokussierten Session)", bundle: Bundle.app), { [weak self] in
                if let s = focusedSession, let g = self?.workspace.group(forSession: s.id) { self?.openEditGroup(g.id) } }),
            (String(localized: "Statistik", bundle: Bundle.app), { [weak self] in self?.sheets.togglePanel(.stats) }),
            (String(localized: "Reload", bundle: Bundle.app), { [weak self] in Task { await self?.registry.pollNow(); self?.workspace.relayout() } }),
            (String(localized: "Konto hinzufügen (aktuell angemeldetes)", bundle: Bundle.app), { [weak self] in self?.addAccount() }),
        ]
        commands += accounts.accounts.map { a in (String(localized: "Zu Konto wechseln: \(a.title)", bundle: Bundle.app), { [weak self] in self?.switchAccount(a.id) }) }
        commands += store.groups.map { g in (String(localized: "Alle Sessions von \(g.name)", bundle: Bundle.app), { [weak self] in self?.workspace.select(g.sessionIds, add: false) }) }
        src.commands = commands
        palette.source = src
        palette.open(over: window, prefix: prefix)
    }

    /// Treffer aus der Suche über alle Terminals: Session zeigen, Suchleiste dort mit dem Begriff öffnen und
    /// bis zum gewählten Treffer weiterspringen. Danach blättern ⌘G / ⌘⇧G wie gewohnt.
    private func findInSession(_ key: String, term: String, index: Int) {
        workspace.select([key], add: false)
        DispatchQueue.main.async { [weak self] in
            guard let self, let t = attach.terminal(for: key), t.window === window else { return }
            window.makeFirstResponder(t)
            t.clearSearch()
            NSPasteboard(name: .find).copy(term)
            let show = NSMenuItem()
            show.tag = NSTextFinder.Action.showFindInterface.rawValue
            t.performTextFinderAction(show)
            for _ in 0..<index { t.findNext(term) }
        }
    }

    // MARK: Sessions

    /// Klick auf die Update-Pille in der Statusleiste: Changelog-Zeilen der neuen Version, Download öffnet den Browser.
    private func showUpdateAvailable() {
        guard let m = updateChecker.available else { return }
        sheets.confirm(String(localized: "Kadrell \(m.version) verfügbar", bundle: Bundle.app), m.notes.joined(separator: "\n"), button: String(localized: "Herunterladen", bundle: Bundle.app), destructive: false) {
            guard let url = URL(string: m.url) else { return }
            NSWorkspace.shared.open(url)
        }
    }

    func stopSession(_ s: Session) {
        guard attach.isAttached(s.id) else { NSSound.beep(); return }
        sheets.confirm(String(localized: "Session „\(s.title)“ stoppen?", bundle: Bundle.app), String(localized: "Claude wird beendet, die Kachel bleibt. Ein Klick setzt die Konversation fort.", bundle: Bundle.app), button: String(localized: "Stoppen", bundle: Bundle.app), ask: .stopSession) { [weak self] in
            self?.attach.stop(s.id)
        }
    }

    func closeSession(_ key: String, force: Bool = false) {
        guard let s = workspace.session(key) else { return }
        let info = s.isRemote ? String(localized: "Die Verbindung wird getrennt, die tmux-Session auf \(s.host ?? "") läuft weiter.", bundle: Bundle.app)
            : s.isShell ? String(localized: "Die Shell und alles, was darin läuft, wird beendet.", bundle: Bundle.app)
            : String(localized: "Claude wird beendet und die Kachel entfernt. Die Konversation bleibt erhalten: claude --resume \(s.sessionId)", bundle: Bundle.app)
        sheets.confirm(String(localized: "„\(s.title)“ beenden und entfernen?", bundle: Bundle.app), info, button: String(localized: "Entfernen", bundle: Bundle.app), skip: force || s.isRemote, ask: .closeSession) { [weak self] in
            guard let self else { return }
            Feedback.play(.close)
            attach.detach(key)
            store.removeSession(key)
            registry.remove([key])
        }
    }

    /// Baum und Arbeitsfläche teilen eine Reihenfolge: in derselben Gruppe tauscht die Session den Platz,
    /// über Gruppengrenzen hinweg rückt die ganze Gruppe an die Stelle der Zielgruppe.
    private func moveSession(_ id: String, to target: String) {
        guard let g = store.group(forSession: id), let t = store.group(forSession: target) else { return }
        if g.id == t.id { store.moveSession(id, to: target) } else { store.moveGroup(g.id, to: t.id) }
        reloadViews()
    }

    func closeGroup(_ gid: String, force: Bool = false) {
        guard let g = store.group(id: gid) else { return }
        let members = g.sessionIds.compactMap { workspace.session($0) }
        if members.isEmpty {
            store.remove(id: gid)
            reloadViews()
            return
        }
        sheets.confirm(String(localized: "Gruppe „\(g.name)“ mit \(members.count) Session(s) schließen?", bundle: Bundle.app),
                String(localized: "Claude wird in allen Sessions beendet und die Gruppe entfernt. Die Konversationen bleiben erhalten.", bundle: Bundle.app), button: String(localized: "Schließen", bundle: Bundle.app), skip: force, ask: .closeGroup) { [weak self] in
            guard let self else { return }
            Feedback.play(.close)
            for s in members { attach.detach(s.id) }
            store.remove(id: gid)
            registry.remove(Set(members.map(\.id)))
        }
    }

    /// Host-Gruppe: kein Claude dort, das „+“ öffnet die Auswahl der tmux-Sessions des Hosts.
    private func pickRemoteSession(in g: Group) -> Bool {
        guard let host = g.host else { return false }
        togglePalette(prefix: "@\(host):")
        return true
    }

    /// ⌘ auf dem „+“ einer Gruppe: Terminal ohne Claude im Ordner der Gruppe, direkt in derselben Gruppe.
    private func openNewTerminal(groupId: String) {
        guard let g = store.group(id: groupId), !pickRemoteSession(in: g) else { return }
        startShell(group: g, cwd: g.cwd)
    }

    private func openNewSession(groupId: String?) {
        if let gid = groupId, let g = store.group(id: gid) {
            if !pickRemoteSession(in: g) { startSession(group: g, cwd: g.cwd) }
            return
        }
        let sessions = workspace.sessions
        let counts = Dictionary(uniqueKeysWithValues: store.groups.map { ($0.id, $0.sessionIds.filter { sessions[$0] != nil }.count) })
        // Repos unter dem Startordner und neben allen bekannten Projekten; bis der Scan steht, gilt der gespeicherte Stand.
        let model = NewSessionModel(groups: store.groups, counts: counts)
        let known = store.groups.map(\.cwd) + FolderIndex.shared.uses.keys
        FolderIndex.shared.refresh(roots: [Settings.startFolder] + known.map { ($0 as NSString).deletingLastPathComponent }) { [weak model] in
            model?.refreshIfUntouched()
        }
        let view = NewSessionView(model: model, onOpenSettings: { [weak self] in self?.menuSettings() }) { [weak self] g, cwd in
            self?.sheets.dismiss()
            self?.startSession(group: g, cwd: cwd)
        }
        sheets.present(view, onPrimary: { model.start() })
    }

    /// Kadrell vergibt die sessionId selbst (`claude --session-id`): die Kachel steht sofort, kein Warten auf `claude agents`.
    /// `show: false` startet Claude im Hintergrund, die Auswahl bleibt. `prompt` ist die erste Nachricht.
    @discardableResult
    func startSession(group: Group?, cwd: String, show: Bool = true, prompt: String? = nil, sessionId: String? = nil) -> String {
        let id = sessionId ?? UUID().uuidString.lowercased()
        var target = group ?? store.group(forCwd: cwd)
        if target == nil {
            let g = store.makeGroup(cwd: cwd)
            store.add(g)
            target = g
        }
        store.attach(sessionId: id, to: target!.id)
        FolderIndex.shared.recordUse(cwd)
        let session = Session(id: id, cwd: cwd, startedAt: Date().timeIntervalSince1970 * 1000, sessionId: id, name: "")
        if let prompt { attach.initialPrompts[id] = prompt }
        registry.add(session)
        Feedback.play(.open)
        if show { workspace.select([id], add: !workspace.selected.isEmpty) } else { attach.attachNow(session) }
        return id
    }

    /// Unverändert bestätigt bleibt der Name automatisch, sonst hält ein Enter Claudes Titel für immer fest.
    private func renameSession(_ key: String) {
        guard let s = workspace.session(key) else { return }
        let model = RenameSessionModel(session: s)
        model.onSave = { [weak self] name in
            guard let self else { return }
            sheets.dismiss()
            if s.customName == nil, name == s.title { return }
            registry.rename(key, to: name)
        }
        sheets.present(RenameSessionView(model: model), onPrimary: { model.save() })
    }

    private func openEditGroup(_ gid: String) {
        guard let g = store.group(id: gid) else { return }
        let model = EditGroupModel(group: g)
        model.onSave = { [weak self] updated in
            guard let self else { return }
            sheets.dismiss()
            store.update(updated)
            reloadViews()
        }
        sheets.present(EditGroupView(model: model), onPrimary: { model.save() })
    }
}

/// Roter Knopf: ein Zusatzfenster geht wirklich zu, das letzte Fenster wird nur versteckt. Kadrell läuft mit allen
/// Sessions weiter; Menüleisten-Icon oder Dock-Klick holen es zurück, ohne dass Sessions neu anhängen müssen.
extension AppDelegate: NSWindowDelegate {
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard windows.count > 1, let c = controller(for: sender) else { sender.orderOut(nil); return false }
        closeWindow(c)
        return true
    }

    /// Menü, Kürzel, Hook `session-focus` und Leiste folgen dem Fenster, das gerade vorn ist.
    func windowDidBecomeKey(_ notification: Notification) {
        guard let c = controller(for: notification.object as? NSWindow), c !== current else { return }
        current = c
        syncSidebar()
    }
}

final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}

/// Split-View mit 1-px-Trenner in der Linienfarbe des Themes; greifbar ist er 8 px breit.
/// Die Unteransichten würden den Klick sonst vorher schlucken, deshalb entscheidet der Hit-Test hier.
final class ThinSplitView: NSSplitView {
    static let grabWidth: CGFloat = 8
    override var dividerColor: NSColor { Theme.line }
    override var dividerThickness: CGFloat { 1 }

    private var grabRect: CGRect {
        guard arrangedSubviews.count > 1, !arrangedSubviews[0].isHidden else { return .zero }
        let x = arrangedSubviews[0].frame.maxX
        return CGRect(x: x - ThinSplitView.grabWidth / 2, y: 0, width: ThinSplitView.grabWidth + dividerThickness, height: bounds.height)
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let p = convert(point, from: superview)
        return grabRect.contains(p) ? self : super.hitTest(point)
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        let r = grabRect
        if !r.isEmpty { addCursorRect(r, cursor: .resizeLeftRight) }
    }

    override func mouseMoved(with event: NSEvent) { NSCursor.resizeLeftRight.set() }
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for t in trackingAreas where t.owner === self { removeTrackingArea(t) }
        addTrackingArea(NSTrackingArea(rect: grabRect, options: [.mouseMoved, .activeInKeyWindow], owner: self))
    }
    override func setFrameSize(_ newSize: NSSize) { super.setFrameSize(newSize); updateTrackingAreas() }
}
