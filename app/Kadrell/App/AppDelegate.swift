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
    private let workspace = WorkspaceView(frame: .zero)
    private let store = GroupStore()
    private var cli: ClaudeCLI!
    private var registry: SessionRegistry!
    private var attach: AttachManager!
    private let usage = UsageService()
    private var palette: PaletteWindow!
    private var overlay: OverlayPanel?
    private var keyMonitor: Any?
    /// Platzhalter-Sessions, sofort sichtbar, bis `claude --bg` und der nächste Poll durch sind.
    private var pending: [Session] = []
    private var pendingGroups: [String: Group] = [:]

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Nur eine Instanz: läuft schon ein Kadrell (egal aus welchem Pfad), das nach vorn holen und selbst beenden.
        let others = NSRunningApplication.runningApplications(withBundleIdentifier: "de.malura.kadrell")
            .filter { $0.processIdentifier != ProcessInfo.processInfo.processIdentifier }
        if let other = others.first, NSClassFromString("XCTestCase") == nil {
            other.activate()
            NSApp.terminate(nil)
            return
        }
        // Dock-Icon direkt aus dem Bundle: LaunchServices hält für Debug-Builds am selben Pfad gern das alte, leere Icon.
        if let url = Bundle.main.url(forResource: "Kadrell", withExtension: "icns"), let img = NSImage(contentsOf: url) { NSApp.applicationIconImage = img }
        buildMenu()
        buildWindow()
        // Unter XCTest nur das Fenster, kein Polling.
        guard NSClassFromString("XCTestCase") == nil else { return }
        Task { await boot() }
        // Beim ersten Start die Hilfe zeigen: da steht alles, die Oberfläche selbst erklärt nichts.
        if !UserDefaults.standard.bool(forKey: "helpShown") {
            UserDefaults.standard.set(true, forKey: "helpShown")
            showAbout()
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    func applicationWillTerminate(_ notification: Notification) {
        // Attach-Clients bekommen SIGHUP, die Hintergrund-Sessions laufen weiter (verifiziert).
        attach?.detachAll()
    }

    // MARK: Aufbau

    private func buildWindow() {
        window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 1400, height: 900),
                          styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
        window.title = "Kadrell"
        window.appearance = NSAppearance(named: .darkAqua)
        window.backgroundColor = Theme.bg
        window.minSize = NSSize(width: 800, height: 500)
        window.setFrameAutosaveName("KadrellMain")
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
        workspace.onCloseSession = { [weak self] key, force in self?.closeSession(key, force: force) }
        workspace.onActivateSession = { [weak self] s in self?.resume(s) }
        sidebar.onSelect = { [weak self] ids, mode in
            guard let self else { return }
            switch mode {
            case .replace: workspace.select(ids, add: false)
            case .toggle: workspace.select(ids, add: true)
            case .add: workspace.addMissing(ids)
            }
        }
        sidebar.onActivateSession = { [weak self] s in self?.resume(s) }
        sidebar.onNewSession = { [weak self] gid in self?.openNewSession(groupId: gid) }
        sidebar.onEditGroup = { [weak self] gid in self?.openEditGroup(gid) }
        sidebar.onCloseGroup = { [weak self] gid, force in self?.closeGroup(gid, force: force) }
        sidebar.onCloseSession = { [weak self] key, force in self?.closeSession(key, force: force) }
        bar.onToggleLayout = { [weak self] in guard let self else { return }; workspace.setMode(workspace.mode.other) }

        // ⌘Esc schließt die Fokus-Kachel, F1 die Hilfe, egal ob Terminal oder Fläche die Tastatur hat. Esc allein geht an Claude.
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, event.window === self.window else { return event }
            if event.keyCode == 53, event.modifierFlags.contains(.command) { self.workspace.removeFocused(); return nil }
            if event.keyCode == 122 { self.showAbout(); return nil }   // F1
            return event
        }
    }

    private func boot() async {
        cli = await ClaudeCLI.resolve()
        attach = AttachManager(cli: cli)
        workspace.attach = attach
        sidebar.attach = attach
        attach.onChange = { [weak self] in self?.workspace.relayout() }
        // Ctrl-C/Ctrl-D im Terminal beendet den Attach-Client: dann ist die Session gemeint, nicht nur der Client.
        attach.onClientExit = { [weak self] key in
            guard let self, let s = workspace.session(key), s.canAttach else { return }
            closeSession(key, force: true)
        }
        registry = SessionRegistry(cli: cli)
        registry.onChange = { [weak self] sessions in self?.sessionsChanged(sessions) }
        registry.onError = { error in AppDelegate.log.error("agents: \(String(describing: error), privacy: .public)") }
        registry.start()
        usage.onChange = { [weak self] u in self?.bar.usage = u; self?.bar.needsDisplay = true }
        usage.start()
    }

    private func sessionsChanged(_ sessions: [Session]) {
        store.assign(sessions)
        attach.sync(with: sessions)
        reloadViews()
        attach.enqueue(workspace.sessionsByPriority())
    }

    private func reloadViews() {
        workspace.reload(groups: displayGroups(), sessions: (registry?.sessions ?? []) + pending)
        syncSidebar()
    }

    /// Gruppen wie im Store, plus Platzhalter: in die Gruppe mit gleichem Ordner, sonst eine vorläufige Gruppe.
    private func displayGroups() -> [Group] {
        var groups = store.groups
        for p in pending {
            if let i = groups.firstIndex(where: { $0.cwd == p.cwd }) { groups[i].sessionIds.append(p.id); continue }
            var g = pendingGroups[p.cwd] ?? store.makeGroup(cwd: p.cwd)
            pendingGroups[p.cwd] = g
            g.sessionIds = [p.id]
            groups.append(g)
        }
        return groups
    }

    /// Baum und Leiste folgen der Arbeitsfläche (Auswahl, Fokus, Layout).
    private func syncSidebar() {
        sidebar.selected = Set(workspace.selected)
        sidebar.focused = workspace.focused
        sidebar.reload(groups: displayGroups(), sessions: Array(workspace.sessions.values))
        let sessions = workspace.sessions
        let focused = workspace.focused.flatMap { sessions[$0] }
        let fg = focused.flatMap { workspace.group(forSession: $0.id) }
        bar.crumb = focused.map { (fg?.name ?? "", $0.title) }
        bar.crumbGroupAttrs = fg.map { Theme.attrs(11.5, NSColor(hexString: $0.color)) }
        bar.sessionCount = sessions.count
        bar.openCount = workspace.selected.count
        bar.layoutMode = workspace.mode
        let attachable = sessions.values.filter(\.canAttach).count
        bar.attachText = "attach \(attach?.attachedCount ?? 0)/\(attachable)"
        bar.needsDisplay = true
    }

    // MARK: Menü

    private func buildMenu() {
        let main = NSMenu()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Über Kadrell", action: #selector(menuAbout), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Einstellungen …", action: #selector(menuSettings), keyEquivalent: ",")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Kadrell ausblenden", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Kadrell beenden", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        main.addItem(withTitle: "Kadrell", action: nil, keyEquivalent: "").submenu = appMenu

        let file = NSMenu(title: "Datei")
        file.addItem(withTitle: "Neue Session", action: #selector(menuNewSession), keyEquivalent: "n")
        file.addItem(withTitle: "Neue Session in dieser Gruppe", action: #selector(menuNewSessionHere), keyEquivalent: "\r")
        main.addItem(withTitle: "Datei", action: nil, keyEquivalent: "").submenu = file

        let edit = NSMenu(title: "Bearbeiten")
        edit.addItem(withTitle: "Ausschneiden", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Kopieren", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Einsetzen", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Alles auswählen", action: #selector(menuSelectAll), keyEquivalent: "a")
        main.addItem(withTitle: "Bearbeiten", action: nil, keyEquivalent: "").submenu = edit

        let view = NSMenu(title: "Ansicht")
        view.addItem(withTitle: "Grid", action: #selector(menuGrid), keyEquivalent: "1")
        view.addItem(withTitle: "Stack", action: #selector(menuStack), keyEquivalent: "2")
        view.addItem(withTitle: "Kachel allein (zen)", action: #selector(menuZen), keyEquivalent: "\r").keyEquivalentModifierMask = [.command, .shift]
        view.addItem(withTitle: "Baum ein/aus", action: #selector(menuSidebar), keyEquivalent: "b")
        view.addItem(.separator())
        func arrow(_ title: String, _ key: Int, _ sel: Selector) {
            let item = view.addItem(withTitle: title, action: sel, keyEquivalent: String(Character(UnicodeScalar(key)!)))
            item.keyEquivalentModifierMask = [.command, .option]
        }
        arrow("Fokus links", NSLeftArrowFunctionKey, #selector(menuFocusLeft))
        arrow("Fokus rechts", NSRightArrowFunctionKey, #selector(menuFocusRight))
        arrow("Fokus oben", NSUpArrowFunctionKey, #selector(menuFocusUp))
        arrow("Fokus unten", NSDownArrowFunctionKey, #selector(menuFocusDown))
        view.addItem(.separator())
        view.addItem(withTitle: "Suche", action: #selector(menuPalette), keyEquivalent: "p")
        main.addItem(withTitle: "Ansicht", action: nil, keyEquivalent: "").submenu = view

        let session = NSMenu(title: "Session")
        session.addItem(withTitle: "Stoppen", action: #selector(menuStop), keyEquivalent: "w")
        main.addItem(withTitle: "Session", action: nil, keyEquivalent: "").submenu = session
        NSApp.mainMenu = main
    }

    @objc private func menuNewSession() { openNewSession(groupId: nil) }
    /// ⌘⏎: gleich los im Ordner der fokussierten Session, ohne Fokus wie ⌘N.
    @objc private func menuNewSessionHere() {
        guard let key = workspace.focused, let s = workspace.session(key) else { openNewSession(groupId: nil); return }
        let g = workspace.group(forSession: key)
        startSession(group: g, cwd: g?.cwd ?? s.cwd)
    }
    @objc private func menuAbout() { showAbout() }
    @objc private func menuSettings() {
        let model = SettingsModel()
        model.onDone = { [weak self] in self?.dismissSheet() }
        present(SettingsView(model: model), onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { model.save() })
    }
    /// ⌘A: in einem Textfeld die übliche Textauswahl, sonst alle Sessions rechts öffnen.
    @objc private func menuSelectAll() {
        if let t = NSApp.keyWindow?.firstResponder as? NSText { t.selectAll(nil); return }
        workspace.select(store.groups.flatMap(\.sessionIds), add: false)
    }
    @objc private func menuGrid() { workspace.setMode(.grid) }
    @objc private func menuStack() { workspace.setMode(.stack) }
    @objc private func menuZen() { workspace.toggleZen() }
    @objc private func menuSidebar() {
        sidebarScroll.isHidden.toggle()
        split.adjustSubviews()
    }
    @objc private func menuFocusLeft() { workspace.moveFocus(.left) }
    @objc private func menuFocusRight() { workspace.moveFocus(.right) }
    @objc private func menuFocusUp() { workspace.moveFocus(.up) }
    @objc private func menuFocusDown() { workspace.moveFocus(.down) }

    private func showAbout() {
        if overlay?.isVisible == true, overlayIsAbout { dismissSheet(); return }
        overlayIsAbout = true
        present(AboutView(), plainReturn: true, onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { [weak self] in self?.dismissSheet() })
    }
    private var overlayIsAbout = false
    @objc private func menuPalette() { togglePalette() }
    @objc private func menuStop() {
        guard let key = workspace.focused, let s = workspace.session(key) else { NSSound.beep(); return }
        stopSession(s)
    }

    // MARK: Palette

    private func togglePalette() {
        if palette.isVisible { palette.switchToCommandMode(); return }
        dismissSheet()
        var src = PaletteWindow.Source()
        let sessions = workspace.sessions
        src.sessions = store.groups.flatMap { g in g.sessionIds.compactMap { sessions[$0] }.map { ($0, group: g, lines: attach?.lines(for: $0.id) ?? []) } }
        src.groups = store.groups
        src.onFocusSession = { [weak self] key in self?.workspace.select([key], add: false) }
        src.onFitGroup = { [weak self] gid in
            guard let self, let g = store.group(id: gid) else { return }
            workspace.select(g.sessionIds, add: false)
        }
        let focusedSession = workspace.focused.flatMap { sessions[$0] }
        src.commands = [
            ("Grid", { [weak self] in self?.workspace.setMode(.grid) }),
            ("Stack", { [weak self] in self?.workspace.setMode(.stack) }),
            ("Neue Session", { [weak self] in self?.openNewSession(groupId: nil) }),
            ("Alle anhängen", { [weak self] in self?.attachAll() }),
            ("Session stoppen (fokussierte)", { [weak self] in if let s = focusedSession { self?.stopSession(s) } }),
            ("Session schließen (fokussierte)", { [weak self] in if let s = focusedSession { self?.closeSession(s.id) } }),
            ("Gruppe bearbeiten (der fokussierten Session)", { [weak self] in
                if let s = focusedSession, let g = self?.workspace.group(forSession: s.id) { self?.openEditGroup(g.id) } }),
            ("Reload", { [weak self] in Task { await self?.registry.pollNow(); self?.workspace.relayout() } }),
        ] + store.groups.map { g in ("Alle Sessions von \(g.name)", { [weak self] in self?.workspace.select(g.sessionIds, add: false) }) }
        palette.source = src
        palette.open(over: window)
    }

    private func attachAll() {
        for s in workspace.sessions.values where s.canAttach { attach.attachNow(s) }
    }

    // MARK: Sessions

    /// Rückfrage im App-Design. ⏎ bestätigt, Esc bricht ab. `skip` (⌘+Klick) führt direkt aus.
    private func confirm(_ message: String, _ info: String, button: String, destructive: Bool = true, skip: Bool = false, then action: @escaping () -> Void) {
        if skip { action(); return }
        let run = { [weak self] in self?.dismissSheet(); action() }
        present(ConfirmView(title: message, info: info, button: button, destructive: destructive,
                            onConfirm: run, onCancel: { [weak self] in self?.dismissSheet() }),
                plainReturn: true, onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: run)
    }

    private func report(_ error: Error) {
        AppDelegate.log.error("\(String(describing: error), privacy: .public)")
        confirm("Claude CLI meldet einen Fehler", String(describing: error), button: "OK", destructive: false) {}
    }

    private func stopSession(_ s: Session) {
        guard let id = s.shortId else { NSSound.beep(); return }
        confirm("Session „\(s.title)“ stoppen?", "Die Konversation bleibt erhalten und lässt sich fortsetzen.", button: "Stoppen") { [weak self] in
            guard let self else { return }
            attach.detach(s.id)
            Task {
                do { try await self.cli.stop(id: id); await self.registry.pollNow() } catch { self.report(error) }
            }
        }
    }

    private func closeSession(_ key: String, force: Bool = false) {
        guard let s = workspace.session(key), !s.isPending else { return }
        guard let id = s.shortId else {
            confirm("Session „\(s.title)“ läuft in einem anderen Terminal", "Interaktive Sessions kann Kadrell nicht stoppen.", button: "OK", destructive: false) {}
            return
        }
        confirm("Session „\(s.title)“ stoppen und entfernen?", "claude stop \(id) und claude rm \(id). Das lässt sich nicht rückgängig machen.", button: "Entfernen", skip: force) { [weak self] in
            guard let self else { return }
            attach.detach(key)
            Task {
                do {
                    if !s.isDone { try? await self.cli.stop(id: id) }
                    try await self.cli.remove(id: id)
                    self.store.removeSession(key)
                    await self.registry.pollNow()
                } catch { self.report(error) }
            }
        }
    }

    /// Beendete Sessions: `--bg --resume`; Einträge ohne Prozess: `respawn`. Danach anhängen und zeigen.
    private func resume(_ s: Session) {
        Task {
            do {
                if s.isStale, let id = s.shortId { try await cli.respawn(id: id) }
                else { _ = try await cli.resume(sessionId: s.sessionId, cwd: s.cwd) }
                if let fresh = await registry.waitFor(timeout: 10, { $0.id == s.id && $0.canAttach }) {
                    workspace.select([fresh.id], add: false)
                }
            } catch { report(error) }
        }
    }

    private func closeGroup(_ gid: String, force: Bool = false) {
        guard let g = store.group(id: gid) else { return }
        let members = g.sessionIds.compactMap { workspace.session($0) }
        let bg = members.filter { $0.shortId != nil }
        if members.isEmpty {
            store.remove(id: gid)
            reloadViews()
            return
        }
        confirm("Gruppe „\(g.name)“ mit \(members.count) Session(s) schließen?",
                bg.isEmpty ? "Die Gruppe wird aus Kadrell entfernt." : "\(bg.count) Session(s) werden gestoppt und mit claude rm gelöscht. Das lässt sich nicht rückgängig machen.", button: "Schließen", skip: force) { [weak self] in
            guard let self else { return }
            for s in members { attach.detach(s.id) }
            Task {
                // stop hält nur an (die Session taucht sonst beim nächsten Poll als neue Gruppe wieder auf), rm löscht.
                for s in bg {
                    guard let id = s.shortId else { continue }
                    if !s.isDone { try? await self.cli.stop(id: id) }
                    try? await self.cli.remove(id: id)
                }
                self.store.remove(id: gid)
                await self.registry.pollNow()
                self.reloadViews()
            }
        }
    }

    // MARK: Sheets

    private func present<V: View>(_ view: V, plainReturn: Bool = false, onCancel: @escaping () -> Void, onPrimary: @escaping () -> Void) {
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
    }

    private func openNewSession(groupId: String?) {
        if let gid = groupId, let g = store.group(id: gid) { startSession(group: g, cwd: g.cwd); return }
        let sessions = workspace.sessions
        let counts = Dictionary(uniqueKeysWithValues: store.groups.map { ($0.id, $0.sessionIds.filter { sessions[$0] != nil }.count) })
        let model = NewSessionModel(groups: store.groups, counts: counts, preselected: groupId.flatMap { store.group(id: $0) })
        let view = NewSessionView(model: model) { [weak self] g, cwd in
            self?.dismissSheet()
            if g == nil { Settings.startFolder = cwd }   // letzte Ordnerwahl merken: beim nächsten ⌘N steht sie schon da
            self?.startSession(group: g, cwd: cwd)
        }
        present(view, onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { model.start() })
    }

    private func startSession(group: Group?, cwd: String) {
        let t0 = Date().timeIntervalSince1970 * 1000 - 2000
        let known = Set(workspace.sessions.keys)
        // Sofort sichtbar: Platzhalter im Baum und rechts als Kachel, bis die echte Session da ist.
        let placeholder = Session.pending(cwd: cwd)
        pending.append(placeholder)
        reloadViews()
        workspace.select([placeholder.id], add: !workspace.selected.isEmpty)
        Task {
            do {
                let shortId = try await cli.start(cwd: cwd, prompt: "")
                guard let fresh = await registry.waitFor(timeout: 10, { s in
                    (shortId != nil && s.shortId == shortId) || (s.isBackground && !known.contains(s.id) && s.cwd == cwd && s.startedAt >= t0)
                }) else {
                    pending.removeAll { $0.id == placeholder.id }
                    reloadViews()
                    report(CLIError(command: "claude --bg", status: 0, output: "Die neue Session in \(cwd) ist nach 10 s nicht in `claude agents` aufgetaucht."))
                    return
                }
                pending.removeAll { $0.id == placeholder.id }
                pendingGroups[cwd] = nil
                var target = group ?? store.group(forCwd: cwd)
                if target == nil {
                    let g = store.makeGroup(cwd: cwd)
                    store.add(g)
                    target = g
                }
                store.attach(sessionId: fresh.id, to: target!.id)
                reloadViews()   // räumt den Platzhalter aus der Auswahl
                workspace.select([fresh.id], add: !workspace.selected.contains(fresh.id))
            } catch {
                pending.removeAll { $0.id == placeholder.id }
                reloadViews()
                report(error)
            }
        }
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
