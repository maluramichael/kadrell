import AppKit
import SwiftUI
import os

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "app")
    private var window: NSWindow!
    private let bar = StatusBarView(frame: .zero)
    private let split = ThinSplitView(frame: .zero)
    private let sidebarScroll = NSScrollView(frame: .zero)
    private let sidebar = SidebarView(frame: .zero)
    let workspace = WorkspaceView(frame: .zero)
    let store = GroupStore()
    private var cli: ClaudeCLI!
    var registry: SessionRegistry!
    var attach: AttachManager!
    private let usage = UsageService()
    private var palette: PaletteWindow!
    private var overlay: OverlayPanel?
    private var keyMonitor: Any?
    private var scrollMonitor: Any?
    private var fontScrollAccum: CGFloat = 0
    private var statusItem: NSStatusItem?
    /// Session, für die zuletzt `session-focus` gefeuert hat.
    private var hookFocus: String?
    /// Wartende Sessions beim letzten Abgleich: neu dazugekommene lösen `requestUserAttention` aus.
    private var lastWaitingIds: Set<String> = []
    /// Status angehängter Sessions beim letzten Abgleich: Wechsel spielen Sound und lassen den Punkt im Baum aufblitzen.
    private var lastStatuses: [String: SessionStatus] = [:]
    /// Fertig gewordene oder wartende Sessions, die du noch nicht angesehen hast. Weg erst, wenn du sie anklickst
    /// oder fokussierst; übersteht einen Neustart.
    private var unseen = Set(Profile.defaults.stringArray(forKey: "sessions.unseen") ?? []) {
        didSet { if unseen != oldValue { Profile.defaults.set(Array(unseen), forKey: "sessions.unseen") } }
    }
    /// ⌘A/⌘⇧A: Auswahl davor und danach, damit ein zweiter Druck zurückschaltet.
    private var selectAllUndo: (shift: Bool, before: [String], focus: String?, after: Set<String>)?
    var controlServer: ControlServer?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Nur eine Instanz pro Profil: hält schon eine das Profil, die nach vorn holen und selbst beenden.
        if NSClassFromString("XCTestCase") == nil, let other = Profile.acquire() {
            NSRunningApplication(processIdentifier: other)?.activate()
            NSApp.terminate(nil)
            return
        }
        Profile.sweepStale()
        // Dock-Icon direkt aus dem Bundle: LaunchServices hält für Debug-Builds am selben Pfad gern das alte, leere Icon.
        if let url = Bundle.main.url(forResource: "Kadrell", withExtension: "icns"), let img = NSImage(contentsOf: url) { NSApp.applicationIconImage = img }
        buildMenu()
        buildWindow()
        // Unter XCTest nur das Fenster, kein Polling.
        guard NSClassFromString("XCTestCase") == nil else { return }
        trapSignals()
        Task { await boot() }
        // Beim ersten Start die Hilfe zeigen: da steht alles, die Oberfläche selbst erklärt nichts.
        if !Profile.defaults.bool(forKey: "helpShown") {
            Profile.defaults.set(true, forKey: "helpShown")
            showAbout()
        }
        buildStatusItem()
    }

    /// Menüleisten-Icon mit Kurzstatus, holt das Fenster zurück. Bleibt sichtbar, solange Kadrell läuft,
    /// auch wenn das Fenster versteckt ist.
    private func buildStatusItem() {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.button?.image = NSImage(systemSymbolName: "terminal", accessibilityDescription: "Kadrell")
        item.button?.image?.isTemplate = true
        item.button?.action = #selector(statusItemClicked)
        item.button?.target = self
        statusItem = item
    }

    @objc private func statusItemClicked() {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
    }

    /// "3 warten · 5 arbeiten" im Menüleisten-Icon, leer ohne beschäftigte Sessions.
    private func updateStatusItem(_ sessions: [Session]) {
        let waiting = sessions.filter { $0.status == .waiting }.count
        let running = sessions.filter { $0.status == .running }.count
        statusItem?.button?.title = waiting == 0 && running == 0 ? "" : "  \(waiting) warten · \(running) arbeiten"
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }

    /// Dock-Klick, während das Fenster versteckt ist: zurückholen statt neu zu starten.
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
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
        window.makeKeyAndOrderFront(nil)
        NSApp.activate()
        Task {
            await registry.pollNow()
            let running = registry.sessions.filter { attach.isAttached($0.id) }
            let busy = running.filter { $0.status == .running || $0.status == .waiting }
            let list = running.map { s in
                "· \(s.title)" + (s.status == .running ? "  ARBEITET" : s.status == .waiting ? "  WARTET AUF ANTWORT" : "")
            }.joined(separator: "\n")
            let title = busy.isEmpty ? "Kadrell beenden?" : "Kadrell beenden? \(busy.count) Session(s) arbeiten gerade!"
            let info = "\(running.count) Claude-Prozess(e) werden sauber beendet. Laufende Arbeit bricht dabei ab. "
                + "Die Konversationen bleiben erhalten und werden beim nächsten Start fortgesetzt.\n\n\(list)"
            confirm(title, info, button: "Beenden", ask: .quit) { [weak self] in
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

    private func buildWindow() {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1400, height: 900),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Kadrell" + Profile.label
        window.appearance = Theme.appearance
        window.backgroundColor = Theme.bg
        window.minSize = NSSize(width: 800, height: 500)
        window.setFrameAutosaveName("KadrellMain")
        window.delegate = self
        let root = FlippedView(frame: window.contentRect(forFrameRect: window.frame))
        root.autoresizingMask = [.width, .height]
        bar.frame = NSRect(x: 0, y: 0, width: root.bounds.width, height: Theme.barHeight)
        bar.autoresizingMask = [.width, .maxYMargin]

        sidebarScroll.documentView = sidebar
        sidebarScroll.hasVerticalScroller = true
        sidebarScroll.autohidesScrollers = true
        sidebarScroll.drawsBackground = true
        sidebarScroll.backgroundColor = Theme.panel
        sidebar.autoresizingMask = [.width]
        sidebar.frame = NSRect(x: 0, y: 0, width: 260, height: 10)
        split.isVertical = true
        split.dividerStyle = .thin
        split.addArrangedSubview(sidebarScroll)
        split.addArrangedSubview(workspace)
        split.setHoldingPriority(.defaultLow + 1, forSubviewAt: 0)
        split.autosaveName = "KadrellSplit"
        split.delegate = self
        split.frame = NSRect(x: 0, y: Theme.barHeight, width: root.bounds.width, height: root.bounds.height - Theme.barHeight)
        split.autoresizingMask = [.width, .height]
        root.addSubview(split)
        root.addSubview(bar)
        window.contentView = root
        window.center()
        window.makeKeyAndOrderFront(nil)
        if split.arrangedSubviews[0].frame.width < 120 { split.setPosition(260, ofDividerAt: 0) }
        window.makeFirstResponder(workspace)
        NSApp.activate()

        palette = PaletteWindow()

        workspace.onChange = { [weak self] in self?.syncSidebar() }
        workspace.onFocusChange = { [weak self] key in self?.unseen.remove(key); self?.syncSidebar() }
        workspace.onActivate = { [weak self] key in
            guard self?.unseen.contains(key) == true else { return }
            self?.unseen.remove(key)
            self?.syncSidebar()
        }
        workspace.onCloseSession = { [weak self] key, force in self?.closeSession(key, force: force) }
        workspace.onEmptyClick = { [weak self] in self?.openNewSession(groupId: nil) }
        sidebar.onSelect = { [weak self] ids, mode in
            guard let self else { return }
            // Eine einzelne Session anklicken quittiert ihre Marke „neu“, eine ganze Gruppe nicht.
            if ids.count == 1, unseen.contains(ids[0]) { unseen.remove(ids[0]); syncSidebar() }
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
        sidebar.onMoveGroup = { [weak self] gid, target in self?.store.moveGroup(gid, to: target); self?.reloadViews() }
        workspace.onMoveSession = { [weak self] id, target in self?.moveSession(id, to: target) }
        sidebar.onContextMenu = { [weak self] id in self?.sessionMenu(for: id) }
        workspace.onContextMenu = { [weak self] id in self?.sessionMenu(for: id) }
        bar.onPickLayout = { [weak self] m in self?.workspace.setMode(m) }
        bar.onGridColumns = { [weak self] c in self?.workspace.setGridColumns(c) }
        bar.onSplit = { [weak self] c in self?.workspace.setSplit(c) }
        bar.onToggleZoom = { [weak self] in self?.workspace.toggleZen() }
        bar.onToggleAuto = { [weak self] in Feedback.play(.toggle); self?.workspace.toggleAuto() }
        bar.onToggleSync = { [weak self] in Feedback.play(.toggle); self?.workspace.toggleSync() }
        bar.onCycleSort = { [weak self] in self?.cycleSort() }
        bar.onSelectWaiting = { [weak self] in guard let self else { return }; workspace.select(waitingIds(), add: false) }

        // Belegbare Kürzel (Einstellungen) und F1 gehen vor, egal ob Terminal oder Fläche die Tastatur hat.
        // Dialoge sind eigene Fenster und bekommen ihre Tasten unverändert.
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            // Offene Hilfe hat selbst die Tastatur: F1 schließt sie wieder.
            if event.keyCode == 122, overlayIsAbout, event.window === overlay { dismissSheet(); return nil }
            guard event.window === self.window else { return event }
            // Dialog offen, aber nicht mehr Key (nach Dropdown, Klick daneben oder App-Wechsel): Esc landet am
            // Hauptfenster statt am Panel. Ohne das schließt nichts den Dialog, der Blur bleibt liegen und blockt
            // alle Klicks (Softlock). onCancel räumt den Backdrop mit ab.
            if event.keyCode == 53, let overlay, overlay.isVisible { overlay.onCancel?(); return nil }
            // Vorschau offen: ⏎ übernimmt die Session als Auswahl, Esc zeigt wieder die alte.
            if workspace.preview != nil, event.modifierFlags.intersection(Hotkey.modMask).isEmpty, [36, 76, 53].contains(event.keyCode) {
                endPreview(commit: event.keyCode != 53)
                return nil
            }
            if let action = Hotkeys.action(for: event) { self.perform(action); return nil }
            // ⌘⏎: neue Session im Ordner der fokussierten. Vor dem Terminal abgefangen.
            if event.keyCode == 36, event.modifierFlags.intersection([.command, .shift, .option, .control]) == .command { self.newSessionInFocusedFolder(); return nil }
            if event.keyCode == 122 { self.showAbout(); return nil }   // F1
            // Sync: dieselbe Taste an alle anderen Kacheln (siehe `forward`). ⌘V fügt überall ein, andere ⌘-Kürzel bleiben lokal.
            if let src = window.firstResponder as? KadrellTerminalView {
                let mods = event.modifierFlags.intersection(Hotkey.modMask)
                for t in workspace.syncTargets(except: src) {
                    if !mods.contains(.command) { KadrellTerminalView.forward(event, to: t) }
                    else if mods == .command, event.charactersIgnoringModifiers == "v" { t.paste(self) }
                }
            }
            return event
        }
        // ⌘ + Mausrad: Schriftgröße aller Terminals wie ⌘+/⌘-. Trackpad-Deltas sammeln, sonst springt es pro Wisch zweistellig.
        // Klick in ein Terminal quittiert die Marke „neu“, auch wenn es schon die Tastatur hatte.
        _ = NSEvent.addLocalMonitorForEvents(matching: .leftMouseDown) { [weak self] event in
            guard let self, event.window === window, !unseen.isEmpty,
                  var v = window.contentView?.hitTest(event.locationInWindow) else { return event }
            while !(v is KadrellTerminalView), let up = v.superview { v = up }
            if let key = attach?.terminals.first(where: { $0.value === v })?.key, unseen.contains(key) { unseen.remove(key); syncSidebar() }
            return event
        }
        scrollMonitor = NSEvent.addLocalMonitorForEvents(matching: .scrollWheel) { [weak self] event in
            guard let self, event.window === self.window else { return event }
            // Layout Scrollen: seitliches Wischen (bzw. ⇧ + Mausrad) über der Arbeitsfläche verschiebt die Spalten, nicht das Terminal.
            if workspace.mode == .scroll, abs(event.scrollingDeltaX) > abs(event.scrollingDeltaY),
               workspace.bounds.contains(workspace.convert(event.locationInWindow, from: nil)) {
                workspace.scrollBy(-event.scrollingDeltaX * (event.hasPreciseScrollingDeltas ? 1 : 10))
                return nil
            }
            guard event.modifierFlags.intersection([.command, .shift, .option, .control]) == .command else { return event }
            fontScrollAccum += event.hasPreciseScrollingDeltas ? event.scrollingDeltaY / 20 : event.scrollingDeltaY
            let steps = Int(fontScrollAccum)
            guard steps != 0 else { return nil }
            fontScrollAccum -= CGFloat(steps)
            Settings.terminalFontSize += Double(steps)
            applyAppearance()
            return nil
        }
    }

    private func boot() async {
        cli = await ClaudeCLI.resolve()
        attach = AttachManager(cli: cli)
        workspace.attach = attach
        sidebar.attach = attach
        attach.onChange = { [weak self] in self?.workspace.relayout() }
        // Einstellung: Kachel einer beendeten Session schließen statt mit „Klick setzt fort" stehen zu lassen.
        attach.onEnded = { [weak self] key in
            guard let self else { return }
            // `exit` in einem Terminal ohne Claude: nichts fortzusetzen, Kachel und Eintrag weg.
            if workspace.session(key)?.isShell == true { closeSession(key, force: true); return }
            guard Settings.closeTileOnExit, workspace.selected.contains(key) else { return }
            workspace.select([key], add: true)
            syncSidebar()
        }
        registry = SessionRegistry(cli: cli)
        registry.pids = { [weak attach] in attach?.pids ?? [:] }
        registry.onChange = { [weak self] sessions in self?.sessionsChanged(sessions) }
        // Fenster versteckt (Menüleisten-Betrieb) oder App im Hintergrund: seltener pollen, siehe `updatePollBackground`.
        for name in [NSApplication.didBecomeActiveNotification, NSApplication.didResignActiveNotification] {
            NotificationCenter.default.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.updatePollBackground() }
            }
        }
        NotificationCenter.default.addObserver(forName: NSWindow.didChangeOcclusionStateNotification, object: window, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.updatePollBackground() }
        }
        if !FileManager.default.isExecutableFile(atPath: cli.binary) { registry.fail("\(cli.binary): claude nicht gefunden") }
        else if let tooOld = await cli.checkVersion() { registry.fail(tooOld) }
        // Leer nicht abgleichen: das würde Gruppen alter Hintergrund-Sessions verwerfen, bevor sie übernommen sind.
        if !registry.sessions.isEmpty { sessionsChanged(registry.sessions) }
        Task {
            await registry.pollNow()
            await offerAdopt()
            registry.start()
        }
        startControlServer()
        usage.onChange = { [weak self] u in self?.bar.usage = u; self?.bar.needsDisplay = true }
        usage.start()
    }

    /// Fenster verdeckt/versteckt oder App nicht aktiv: `SessionRegistry` seltener pollen lassen.
    private func updatePollBackground() {
        registry?.setBackground(!NSApp.isActive || window?.isVisible != true)
    }

    private func sessionsChanged(_ sessions: [Session]) {
        store.assign(sessions)
        attach.sync(with: sessions)
        reloadViews()
        attach.enqueue(workspace.shownSessions())
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
            AppDelegate.log.error("agents: \(String(describing: error), privacy: .public)")
            registry.fail("\(cli.binary): \(CLIError.firstLine(of: error))")
            return
        }
        let elsewhere = agents.filter { !owned.contains($0.sessionId) }
        workspace.otherInteractiveCount = elsewhere.filter { $0.kind == "interactive" }.count
        // Übernahme in die Sessions-Liste nur das Standardprofil, sonst stiehlt ein Testprofil sie.
        guard Profile.name == nil else { return }
        let bg = elsewhere.filter(\.isRunningBackground)
        guard !bg.isEmpty else { return }
        let list = bg.map { "· \($0.name)\($0.status == "busy" ? " (arbeitet gerade)" : "")" }.joined(separator: "\n")
        confirm("\(bg.count) Hintergrund-Session(s) übernehmen?", "Kadrell startet Claude jetzt selbst statt mit claude --bg. Diese Sessions werden mit claude stop angehalten (laufende Arbeit bricht ab) und hier fortgesetzt:\n\(list)", button: "Übernehmen", destructive: false, ask: .adoptBackground) { [weak self] in
            guard let self else { return }
            Task {
                for a in bg {
                    guard let id = a.shortId else { continue }
                    do { try await self.cli.stop(id: id) } catch { self.report(error); continue }
                    self.registry.add(Session(id: id, cwd: a.cwd, startedAt: a.startedAt, sessionId: a.sessionId,
                                              name: Session.isAutoName(a.name, cwd: a.cwd) ? "" : a.name))
                }
            }
        }
    }

    func reloadViews() {
        workspace.lastError = registry?.lastError
        workspace.polled = registry?.polled ?? false
        workspace.reload(groups: store.groups, sessions: registry?.sessions ?? [])
        syncSidebar()
    }

    /// Baum und Leiste folgen der Arbeitsfläche (Auswahl, Fokus, Layout).
    private func syncSidebar() {
        sidebar.selected = Set(workspace.selected)
        sidebar.focused = workspace.preview ?? workspace.focused
        sidebar.showMessages = Settings.showLastMessage
        sidebar.showAge = Settings.sidebarShowAge
        sidebar.sort = Settings.sidebarSort
        sidebar.renderer = Settings.sidebarStyle.renderer
        sidebar.messages = registry?.lastMessages ?? [:]
        unseen = unseen.filter { workspace.sessions[$0] != nil }
        sidebar.unread = unseen
        sidebar.reload(groups: store.groups, sessions: Array(workspace.sessions.values))
        let sessions = workspace.sessions
        let focused = (workspace.preview ?? workspace.focused).flatMap { sessions[$0] }
        let fg = focused.flatMap { workspace.group(forSession: $0.id) }
        bar.crumb = focused.map { (fg?.name ?? "", $0.title) }
        if let cli, let key = workspace.focused, key != hookFocus, let s = sessions[key] {
            hookFocus = key
            Hooks.fire(.sessionFocus, s, environment: cli.environment)
        }
        bar.crumbGroupAttrs = fg.map { Theme.attrs(11.5, Theme.group($0.color)) }
        bar.errorText = registry?.lastError
        bar.sessionCount = sessions.count
        bar.openCount = workspace.selected.count
        bar.layoutMode = workspace.mode
        bar.gridColumns = Settings.gridColumns
        bar.split = workspace.focusedSplit
        bar.zoomed = workspace.zen
        bar.auto = workspace.auto
        bar.sync = workspace.sync
        bar.sort = sidebar.sort
        bar.attachText = "läuft \(attach?.attachedCount ?? 0)/\(sessions.count)"
        let waiting = waitingIds()
        NSApp.dockTile.badgeLabel = waiting.isEmpty ? nil : "\(waiting.count)"
        let waitingSet = Set(waiting)
        // Neu dazugekommene wartende Session, Fenster nicht im Vordergrund: kurz im Dock hüpfen, ohne Notification-Rechte.
        if !waitingSet.subtracting(lastWaitingIds).isEmpty, window?.isKeyWindow == false { NSApp.requestUserAttention(.informationalRequest) }
        lastWaitingIds = waitingSet
        let statuses = sessions.filter { attach?.isAttached($0.key) ?? false }.mapValues(\.status)
        let changed = Feedback.transitions(from: lastStatuses, to: statuses)
        lastStatuses = statuses
        // Klang und Marke „neu“ nur für das, was man gerade nicht sieht: andere Session oder Fenster im Hintergrund.
        let notLooking = { (id: String) in id != self.workspace.focused || self.window?.isKeyWindow == false }
        if changed.waiting.contains(where: notLooking) { Feedback.play(.waiting) } else if changed.done.contains(where: notLooking) { Feedback.play(.done) }
        let fresh = changed.waiting.union(changed.done).filter(notLooking)
        if !fresh.isSubset(of: unseen) { unseen.formUnion(fresh); sidebar.unread = unseen; sidebar.needsDisplay = true }
        // Wieder am Arbeiten: die Marke gilt der letzten Antwort, nicht der laufenden.
        let working = unseen.filter { statuses[$0] == .running }
        if !working.isEmpty { unseen.subtract(working); sidebar.unread = unseen; sidebar.needsDisplay = true }
        sidebar.flash(waiting: changed.waiting, done: changed.done)
        bar.waitingCount = waiting.count
        bar.needsDisplay = true
        updateStatusItem(Array(sessions.values))
    }

    /// Wartende Sessions in Baumreihenfolge, unabhängig von eingeklappten Gruppen: `waitingFor` kommt nur bei
    /// laufendem eigenen Prozess (siehe `SessionRegistry.merge`), `isAttached` ist die zusätzliche Absicherung.
    private func waitingIds() -> [String] {
        let sessions = workspace.sessions
        return store.groups.flatMap(\.sessionIds).filter { sessions[$0]?.status == .waiting && (attach?.isAttached($0) ?? false) }
    }

    // MARK: Menü

    private func buildMenu() {
        let main = NSMenu()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Über Kadrell", action: #selector(menuAbout), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Einstellungen …", action: #selector(menuSettings), keyEquivalent: ",")
        appMenu.addItem(withTitle: "Kommandozeilen-Tool installieren …", action: #selector(menuInstallCLI), keyEquivalent: "")
        appMenu.addItem(withTitle: "Neue Instanz mit temporärem Profil", action: #selector(menuTemporaryInstance), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Kadrell ausblenden", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Kadrell beenden", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        main.addItem(withTitle: "Kadrell", action: nil, keyEquivalent: "").submenu = appMenu

        let file = NSMenu(title: "Datei")
        file.addItem(withTitle: "Neue Session", action: #selector(menuNewSession), keyEquivalent: "n")
        file.addItem(withTitle: "Neue Session im selben Ordner", action: #selector(menuNewSessionHere), keyEquivalent: "\r")
        file.addItem(withTitle: "Neues Terminal ohne Claude", action: #selector(menuNewShell), keyEquivalent: "t")
        let remote = file.addItem(withTitle: "Remote verbinden …", action: #selector(menuRemote), keyEquivalent: "n")
        remote.keyEquivalentModifierMask = [.command, .shift]
        file.addItem(.separator())
        file.addItem(withTitle: "Session schließen", action: #selector(menuCloseSession), keyEquivalent: "w")
        main.addItem(withTitle: "Datei", action: nil, keyEquivalent: "").submenu = file

        let edit = NSMenu(title: "Bearbeiten")
        edit.addItem(withTitle: "Ausschneiden", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Kopieren", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Einsetzen", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Ganze Gruppen auswählen", action: #selector(menuSelectAll), keyEquivalent: "a")
        let selectEverything = NSMenuItem(title: "Alle Sessions auswählen", action: #selector(menuSelectEverything), keyEquivalent: "a")
        selectEverything.keyEquivalentModifierMask = [.command, .shift]
        edit.addItem(selectEverything)
        edit.addItem(.separator())
        // Suchleiste von SwiftTerm im Terminal mit der Tastatur, über die Responder-Kette.
        for (title, key, shift, action) in [("Im Terminal suchen …", "f", false, NSTextFinder.Action.showFindInterface),
                                            ("Weitersuchen", "g", false, .nextMatch), ("Rückwärts suchen", "g", true, .previousMatch)] {
            let item = NSMenuItem(title: title, action: #selector(NSResponder.performTextFinderAction(_:)), keyEquivalent: key)
            item.keyEquivalentModifierMask = shift ? [.command, .shift] : .command
            item.tag = action.rawValue
            edit.addItem(item)
        }
        let findAll = NSMenuItem(title: "In allen Terminals suchen …", action: #selector(menuFindAll), keyEquivalent: "f")
        findAll.keyEquivalentModifierMask = [.command, .shift]
        edit.addItem(findAll)
        main.addItem(withTitle: "Bearbeiten", action: nil, keyEquivalent: "").submenu = edit

        let view = NSMenu(title: "Ansicht")
        for m in LayoutMode.allCases {
            let item = NSMenuItem(title: m.title, action: #selector(menuLayout(_:)), keyEquivalent: "")
            item.representedObject = m.rawValue
            view.addItem(item)
        }
        view.addItem(withTitle: "Auto-Modus ein/aus", action: #selector(menuAuto), keyEquivalent: "")
        view.addItem(withTitle: "Baum ein/aus", action: #selector(menuSidebar), keyEquivalent: "b")
        let toggleGroups = NSMenuItem(title: "Alle Gruppen auf-/zuklappen", action: #selector(menuToggleGroups), keyEquivalent: "b")
        toggleGroups.keyEquivalentModifierMask = [.command, .shift]
        view.addItem(toggleGroups)
        view.addItem(.separator())
        view.addItem(withTitle: "Terminal-Schrift größer", action: #selector(menuFontBigger), keyEquivalent: "+")
        view.addItem(withTitle: "Terminal-Schrift kleiner", action: #selector(menuFontSmaller), keyEquivalent: "-")
        view.addItem(withTitle: "Terminal-Schrift Standardgröße", action: #selector(menuFontReset), keyEquivalent: "0")
        view.addItem(.separator())
        // Belegbare Kürzel: das Menü zeigt die aktuelle Belegung, ausgelöst werden sie im Event-Monitor.
        let keys = Hotkeys.current
        let tiles = NSMenu(title: "Kachel wählen")
        for a in HotkeyAction.allCases {
            let item = NSMenuItem(title: a.title, action: #selector(menuHotkey(_:)), keyEquivalent: keys[a]?.menuEquivalent ?? "")
            item.keyEquivalentModifierMask = keys[a]?.flags ?? []
            item.representedObject = a.rawValue
            if a.tileIndex != nil { tiles.addItem(item) } else { view.addItem(item) }
            if a == .lastSession { view.addItem(withTitle: "Kachel wählen", action: nil, keyEquivalent: "").submenu = tiles }
        }
        view.addItem(.separator())
        view.addItem(withTitle: "Suche", action: #selector(menuPalette), keyEquivalent: "p")
        main.addItem(withTitle: "Ansicht", action: nil, keyEquivalent: "").submenu = view

        let session = NSMenu(title: "Session")
        for item in sessionMenuItems(for: nil, shortcuts: true) { session.addItem(item) }
        main.addItem(withTitle: "Session", action: nil, keyEquivalent: "").submenu = session

        let windows = NSMenu(title: "Fenster")
        windows.addItem(withTitle: "Im Dock ablegen", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        windows.addItem(withTitle: "Zoomen", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        main.addItem(withTitle: "Fenster", action: nil, keyEquivalent: "").submenu = windows
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
    /// ⌘T: Login-Shell im Ordner der fokussierten Session, sonst im Startordner.
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

    @objc private func menuNewShell() {
        let cwd = workspace.focused.flatMap { workspace.session($0) }?.cwd ?? Settings.startFolder
        startSession(group: nil, cwd: cwd, sessionId: Session.shellPrefix + UUID().uuidString.lowercased())
    }
    @objc private func menuAbout() { showAbout() }
    @objc private func menuTemporaryInstance() { Profile.launchTemporary() }
    @objc private func menuSettings() {
        let model = SettingsModel()
        model.onApply = { [weak self] in
            guard let self else { return }
            buildMenu()
            applyAppearance()
            Task { await self.registry?.pollNow() }
        }
        model.onClose = { [weak self, weak model] in model?.stopRecording(); self?.dismissSheet() }
        present(SettingsView(model: model), onCancel: { model.onClose?() }, onPrimary: { model.onClose?() })
    }
    @objc private func menuFontBigger() { Settings.terminalFontSize += 1; applyAppearance() }
    @objc private func menuFontSmaller() { Settings.terminalFontSize -= 1; applyAppearance() }
    @objc private func menuFontReset() { Settings.terminalFontSize = Settings.defaultFontSize; applyAppearance() }

    /// Darstellung aus den Einstellungen übernehmen, ohne Neustart: Leiste, Baum, Kacheln, Terminals, Palette.
    private func applyAppearance() {
        let themeChanged = Theme.current.id != Settings.colorTheme
        if themeChanged {
            Theme.current = ColorTheme.named(Settings.colorTheme)
            window.appearance = Theme.appearance
            window.backgroundColor = Theme.bg
            sidebarScroll.backgroundColor = Theme.panel
            split.needsDisplay = true
        }
        if themeChanged || Theme.scale != CGFloat(Settings.uiScale) {
            Theme.scale = CGFloat(Settings.uiScale)
            if palette.isVisible { palette.dismiss() }
            palette = PaletteWindow()
        }
        let root = window.contentView!.bounds
        bar.frame = NSRect(x: 0, y: 0, width: root.width, height: Theme.barHeight)
        split.frame = NSRect(x: 0, y: Theme.barHeight, width: root.width, height: root.height - Theme.barHeight)
        bar.needsDisplay = true
        sidebar.needsDisplay = true
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
    }
    @objc private func menuToggleGroups() { sidebar.toggleAllGroups() }
    @objc private func menuHotkey(_ sender: NSMenuItem) {
        if let raw = sender.representedObject as? String, let a = HotkeyAction(rawValue: raw) { perform(a) }
    }

    private func perform(_ action: HotkeyAction) {
        if action != .previewNext, action != .previewPrev { endPreview(commit: false) }
        if let i = action.tileIndex { workspace.focusTile(i); return }
        switch action {
        case .focusLeft: workspace.moveFocus(.left)
        case .focusRight: workspace.moveFocus(.right)
        case .focusUp: workspace.moveFocus(.up)
        case .focusDown: workspace.moveFocus(.down)
        case .swapLeft: workspace.swapFocused(.left)
        case .swapRight: workspace.swapFocused(.right)
        case .swapUp: workspace.swapFocused(.up)
        case .swapDown: workspace.swapFocused(.down)
        case .resizeLeft: workspace.resizeFocused(.left)
        case .resizeRight: workspace.resizeFocused(.right)
        case .resizeUp: workspace.resizeFocused(.up)
        case .resizeDown: workspace.resizeFocused(.down)
        case .splitRight: workspace.setSplit("r")
        case .splitDown: workspace.setSplit("d")
        case .nextSession: workspace.cycleFocus(1)
        case .prevSession: workspace.cycleFocus(-1)
        case .lastSession: workspace.focusLast()
        case .previewNext: stepPreview(1)
        case .previewPrev: stepPreview(-1)
        case .nextWaiting: focusNextWaiting()
        case .zoom: workspace.toggleZen()
        case .nextLayout: workspace.setMode(workspace.mode.next)
        case .focusSidebar: focusSidebar()
        case .focusWorkspace: focusWorkspace()
        case .openEditor: openEditor()
        case .renameSession: if let key = workspace.focused { renameSession(key) } else { NSSound.beep() }
        case .cycleSort: cycleSort()
        case .syncInput: workspace.toggleSync()
        default: workspace.removeFocused()
        }
    }

    private func cycleSort() { Settings.sidebarSort = Settings.sidebarSort.next; syncSidebar() }

    /// Springt zur nächsten Session, die wartet, egal ob ihre Gruppe eingeklappt oder ihre Kachel schon offen ist.
    private func focusNextWaiting() {
        let ids = waitingIds()
        guard !ids.isEmpty else { NSSound.beep(); return }
        let next = workspace.focused.flatMap { ids.firstIndex(of: $0) }.map { ids[($0 + 1) % ids.count] } ?? ids[0]
        workspace.addMissing([next])
        workspace.setFocus(next)
        sidebar.reveal(next)
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
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/zsh")
        p.arguments = ["-lc", cmd + " ."]
        p.currentDirectoryURL = URL(fileURLWithPath: cwd)
        p.environment = cli?.environment
        p.standardInput = FileHandle.nullDevice
        do { try p.run() } catch { report(error) }
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

    private func showAbout() {
        if overlay?.isVisible == true, overlayIsAbout { dismissSheet(); return }
        present(AboutView(), plainReturn: true, onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { [weak self] in self?.dismissSheet() })
        overlayIsAbout = true   // erst nach present: das räumt über dismissSheet den alten Wert ab
    }
    private var overlayIsAbout = false
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
        dismissSheet()
        // Kein Hintergrund-Timer mehr: Snapshots erst hier auf den aktuellen Stand bringen, direkt vorm Zeigen.
        attach?.refreshSnapshots()
        var src = PaletteWindow.Source()
        let sessions = workspace.sessions
        src.sessions = store.groups.flatMap { g in g.sessionIds.compactMap { sessions[$0] }.map { ($0, group: g, lines: attach?.lines(for: $0.id) ?? []) } }
        src.groups = store.groups
        src.buffers = src.sessions.compactMap { s, g, _ in
            attach.terminal(for: s.id).map { (s, group: g, lines: String(decoding: $0.getBufferAsData(kind: .active), as: UTF8.self).components(separatedBy: "\n")) }
        }
        src.onFindInSession = { [weak self] key, term, index in self?.findInSession(key, term: term, index: index) }
        src.hosts = SSHConfig.recent + SSHConfig.hosts().filter { !SSHConfig.recent.contains($0) }
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
        src.commands = LayoutMode.allCases.map { m in ("Layout: \(m.title)", { [weak self] in self?.workspace.setMode(m) }) } + [
            ("Trennlinien zurücksetzen (gleich verteilt)", { [weak self] in self?.workspace.resetRatios() }),
            ("Zoom ein/aus (fokussierte)", { [weak self] in self?.workspace.toggleZen() }),
            ("Neue Session", { [weak self] in self?.openNewSession(groupId: nil) }),
            ("Neues Terminal ohne Claude", { [weak self] in self?.menuNewShell() }),
            ("Remote verbinden (ssh → tmux)", { [weak self] in self?.menuRemote() }),
            ("Session stoppen (fokussierte)", { [weak self] in if let s = focusedSession { self?.stopSession(s) } }),
            ("Session fortsetzen (fokussierte)", { [weak self] in if let s = focusedSession { self?.attach.attachNow(s); self?.workspace.select([s.id], add: false) } }),
            ("Session umbenennen (fokussierte)", { [weak self] in if let s = focusedSession { self?.renameSession(s.id) } }),
            ("Session schließen (fokussierte)", { [weak self] in if let s = focusedSession { self?.closeSession(s.id) } }),
            ("Gruppe bearbeiten (der fokussierten Session)", { [weak self] in
                if let s = focusedSession, let g = self?.workspace.group(forSession: s.id) { self?.openEditGroup(g.id) } }),
            ("Reload", { [weak self] in Task { await self?.registry.pollNow(); self?.workspace.relayout() } }),
        ] + store.groups.map { g in ("Alle Sessions von \(g.name)", { [weak self] in self?.workspace.select(g.sessionIds, add: false) }) }
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
            let pb = NSPasteboard(name: .find)
            pb.clearContents()
            pb.setString(term, forType: .string)
            let show = NSMenuItem()
            show.tag = NSTextFinder.Action.showFindInterface.rawValue
            t.performTextFinderAction(show)
            for _ in 0..<index { t.findNext(term) }
        }
    }

    // MARK: Sessions

    /// Rückfrage im App-Design. Nicht-destruktive bestätigt blankes ⏎, destruktive nur ⌘⏎. Esc bricht immer ab.
    /// `skip` (⌥+Klick) führt direkt aus. `ask` bietet „Nicht mehr fragen“ an; ist die Rückfrage abgeschaltet, läuft die Aktion sofort.
    func confirm(_ message: String, _ info: String, button: String, destructive: Bool = true, skip: Bool = false,
                         ask: Settings.Ask? = nil, then action: @escaping () -> Void) {
        if skip || ask?.enabled == false { action(); return }
        let run = { [weak self] in self?.dismissSheet(); action() }
        let cancel = { [weak self] in ask?.enabled = true; self?.dismissSheet() }
        present(ConfirmView(title: message, info: info, button: button, destructive: destructive, ask: ask,
                            onConfirm: run, onCancel: cancel),
                plainReturn: !destructive, onCancel: cancel, onPrimary: run)
    }

    private func report(_ error: Error) {
        AppDelegate.log.error("\(String(describing: error), privacy: .public)")
        confirm("Claude CLI meldet einen Fehler", String(describing: error), button: "OK", destructive: false) {}
    }

    func stopSession(_ s: Session) {
        guard attach.isAttached(s.id) else { NSSound.beep(); return }
        confirm("Session „\(s.title)“ stoppen?", "Claude wird beendet, die Kachel bleibt. Ein Klick setzt die Konversation fort.", button: "Stoppen", ask: .stopSession) { [weak self] in
            self?.attach.stop(s.id)
        }
    }

    func closeSession(_ key: String, force: Bool = false) {
        guard let s = workspace.session(key) else { return }
        let info = s.isRemote ? "Die Verbindung wird getrennt, die tmux-Session auf \(s.host ?? "") läuft weiter."
            : s.isShell ? "Die Shell und alles, was darin läuft, wird beendet."
            : "Claude wird beendet und die Kachel entfernt. Die Konversation bleibt erhalten: claude --resume \(s.sessionId)"
        confirm("„\(s.title)“ beenden und entfernen?", info, button: "Entfernen", skip: force || s.isRemote, ask: .closeSession) { [weak self] in
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
        confirm("Gruppe „\(g.name)“ mit \(members.count) Session(s) schließen?",
                "Claude wird in allen Sessions beendet und die Gruppe entfernt. Die Konversationen bleiben erhalten.", button: "Schließen", skip: force, ask: .closeGroup) { [weak self] in
            guard let self else { return }
            Feedback.play(.close)
            for s in members { attach.detach(s.id) }
            store.remove(id: gid)
            registry.remove(Set(members.map(\.id)))
        }
    }

    // MARK: Sheets

    /// Offene Hilfe (F1) nicht stillschweigend verdrängen: wer gerade ⏎ drückt, um sie zu schließen, soll nicht
    /// aus Versehen einen anderen Dialog bestätigen. Der neue Dialog kommt erst dran, wenn die Hilfe zu ist.
    private var pendingPresent: (() -> Void)?

    private func present<V: View>(_ view: V, plainReturn: Bool = true, onCancel: @escaping () -> Void, onPrimary: @escaping () -> Void) {
        if overlayIsAbout {
            pendingPresent = { [weak self] in self?.present(view, plainReturn: plainReturn, onCancel: onCancel, onPrimary: onPrimary) }
            return
        }
        dismissSheet()
        if palette.isVisible { palette.dismiss() }
        let p = OverlayPanel(rootView: view)
        p.onCancel = onCancel
        p.onPrimary = onPrimary
        p.primaryOnPlainReturn = plainReturn
        overlay = p
        p.open(over: window)
    }

    private func dismissSheet() {
        overlay?.dismiss()
        overlay = nil
        overlayIsAbout = false
        window.makeFirstResponder(workspace)
        if let pending = pendingPresent { pendingPresent = nil; pending() }
    }

    /// ⌘ auf dem „+“ einer Gruppe: Terminal ohne Claude im Ordner der Gruppe, direkt in derselben Gruppe.
    private func openNewTerminal(groupId: String) {
        guard let g = store.group(id: groupId) else { return }
        if let host = g.host { togglePalette(prefix: "@\(host):"); return }
        startSession(group: g, cwd: g.cwd, sessionId: Session.shellPrefix + UUID().uuidString.lowercased())
    }

    private func openNewSession(groupId: String?) {
        if let gid = groupId, let g = store.group(id: gid) {
            // Host-Gruppe: kein Claude dort, das „+“ öffnet die Auswahl der tmux-Sessions des Hosts.
            if let host = g.host { togglePalette(prefix: "@\(host):"); return }
            startSession(group: g, cwd: g.cwd); return
        }
        let sessions = workspace.sessions
        let counts = Dictionary(uniqueKeysWithValues: store.groups.map { ($0.id, $0.sessionIds.filter { sessions[$0] != nil }.count) })
        // Repos unter dem Startordner und neben allen bekannten Projekten; bis der Scan steht, gilt der gespeicherte Stand.
        let model = NewSessionModel(groups: store.groups, counts: counts)
        let known = store.groups.map(\.cwd) + FolderIndex.shared.uses.keys
        FolderIndex.shared.refresh(roots: [Settings.startFolder] + known.map { ($0 as NSString).deletingLastPathComponent }) { [weak model] in
            model?.refreshIfUntouched()
        }
        let view = NewSessionView(model: model) { [weak self] g, cwd in
            self?.dismissSheet()
            self?.startSession(group: g, cwd: cwd)
        }
        present(view, onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { model.start() })
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
            dismissSheet()
            if s.customName == nil, name == s.title { return }
            registry.rename(key, to: name)
        }
        present(RenameSessionView(model: model), onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { model.save() })
    }

    private func openEditGroup(_ gid: String) {
        guard let g = store.group(id: gid) else { return }
        let model = EditGroupModel(group: g)
        model.onSave = { [weak self] updated in
            guard let self else { return }
            dismissSheet()
            store.update(updated)
            reloadViews()
        }
        present(EditGroupView(model: model), onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { model.save() })
    }
}

/// Fenster schließen (roter Knopf) versteckt nur das Fenster, Kadrell läuft mit allen Sessions im Hintergrund
/// weiter. Menüleisten-Icon oder Dock-Klick holen es zurück, ohne dass Sessions neu anhängen müssen.
extension AppDelegate: NSWindowDelegate {
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        sender.orderOut(nil)
        return false
    }
}

/// Der Baum ist höchstens halb so breit wie das Fenster: beim Ziehen und wenn das Fenster schmaler wird.
extension AppDelegate: NSSplitViewDelegate {
    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        min(proposedMaximumPosition, splitView.bounds.width / 2)
    }

    /// Ziehen startet nur im effektiven Rechteck, ohne das hier ist es die 1-px-Linie: so breit wie die Griffzone.
    func splitView(_ splitView: NSSplitView, effectiveRect proposedEffectiveRect: NSRect, forDrawnRect drawnRect: NSRect, ofDividerAt dividerIndex: Int) -> NSRect {
        drawnRect.insetBy(dx: -ThinSplitView.grabWidth / 2, dy: 0)
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        let half = split.bounds.width / 2
        if !sidebarScroll.isHidden, sidebarScroll.frame.width > half + 1 { split.setPosition(half, ofDividerAt: 0) }
        // Der Trenner ist gewandert (Ziehen, UI-Größe, Baum ein/aus): Griffzone für Cursor und Mausbewegung nachziehen.
        split.updateTrackingAreas()
        window.invalidateCursorRects(for: split)
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
