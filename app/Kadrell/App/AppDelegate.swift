import AppKit
import SwiftUI
import os

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "app")
    private var window: NSWindow!
    private let bar = StatusBarView(frame: .zero)
    private let canvas = CanvasView(frame: .zero)
    private let store = GroupStore()
    private var cli: ClaudeCLI!
    private var registry: SessionRegistry!
    private var attach: AttachManager!
    private var palette: PaletteWindow!
    private var overlay: OverlayPanel?
    private var sessionCounter = 0
    private var firstLoad = true

    func applicationDidFinishLaunching(_ notification: Notification) {
        buildMenu()
        buildWindow()
        // Unter XCTest nur das Fenster, kein Polling.
        guard NSClassFromString("XCTestCase") == nil else { return }
        Task { await boot() }
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
        canvas.frame = NSRect(x: 0, y: Theme.barHeight, width: root.bounds.width, height: root.bounds.height - Theme.barHeight)
        canvas.autoresizingMask = [.width, .height]
        root.addSubview(canvas)
        root.addSubview(bar)
        window.contentView = root
        window.center()
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(canvas)
        NSApp.activate()

        palette = PaletteWindow()
        palette.onHighlight = { [weak self] keys in self?.canvas.setHighlight(keys) }

        canvas.onFocusChange = { [weak self] _ in self?.updateBar() }
        canvas.onViewChange = { [weak self] in self?.updateBar() }
        canvas.onNewSession = { [weak self] gid in self?.openNewSession(groupId: gid) }
        canvas.onEditGroup = { [weak self] gid in self?.openEditGroup(gid) }
        canvas.onCloseGroup = { [weak self] gid in self?.closeGroup(gid) }
        canvas.onCloseSession = { [weak self] key in self?.closeSession(key) }
        canvas.onActivateSession = { [weak self] s in self?.resume(s) }
        bar.onNew = { [weak self] in self?.openNewSession(groupId: nil) }
        bar.onPalette = { [weak self] in self?.togglePalette() }
        bar.onFit = { [weak self] in self?.canvas.fitAll() }
    }

    private func boot() async {
        cli = await ClaudeCLI.resolve()
        attach = AttachManager(cli: cli)
        canvas.attach = attach
        attach.onChange = { [weak self] in self?.canvas.applyLayout() }
        attach.onEscape = { [weak self] in self?.canvas.escapeStep() }
        registry = SessionRegistry(cli: cli)
        registry.onChange = { [weak self] sessions in self?.sessionsChanged(sessions) }
        registry.onError = { error in AppDelegate.log.error("agents: \(String(describing: error), privacy: .public)") }
        registry.start()
    }

    private func sessionsChanged(_ sessions: [Session]) {
        store.assign(sessions)
        attach.sync(with: sessions)
        canvas.reload(groups: store.groups, sessions: sessions)
        if firstLoad {
            firstLoad = false
            canvas.fitAll(animated: false)
            attach.enqueue(canvas.sessionsByDistanceToCenter())
        } else {
            attach.enqueue(canvas.sessionsByDistanceToCenter())
        }
        updateBar()
    }

    private func updateBar() {
        let sessions = canvas.sessions
        let focused = canvas.focusedKey.flatMap { sessions[$0] }
        let fg = focused.flatMap { canvas.group(forSession: $0.id) }
        bar.crumb = focused.map { (fg?.name ?? "", $0.name) }
        bar.crumbGroupAttrs = fg.map { Theme.attrs(11.5, NSColor(hexString: $0.color)) }
        bar.sessionCount = sessions.count
        var counts: [SessionStatus: Int] = [:]
        for s in sessions.values { counts[s.status, default: 0] += 1 }
        bar.counts = counts
        bar.zoomPercent = Int((canvas.scale * 100).rounded())
        let attachable = sessions.values.filter(\.canAttach).count
        bar.attachText = "attach \(attach?.attachedCount ?? 0)/\(attachable)"
        bar.needsDisplay = true
    }

    // MARK: Menü

    private func buildMenu() {
        let main = NSMenu()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Über Kadrell", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: "")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Kadrell ausblenden", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        appMenu.addItem(.separator())
        appMenu.addItem(withTitle: "Kadrell beenden", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        main.addItem(withTitle: "Kadrell", action: nil, keyEquivalent: "").submenu = appMenu

        let file = NSMenu(title: "Datei")
        file.addItem(withTitle: "Neue Session", action: #selector(menuNewSession), keyEquivalent: "n")
        main.addItem(withTitle: "Datei", action: nil, keyEquivalent: "").submenu = file

        let edit = NSMenu(title: "Bearbeiten")
        edit.addItem(withTitle: "Ausschneiden", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Kopieren", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Einsetzen", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Alles auswählen", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        main.addItem(withTitle: "Bearbeiten", action: nil, keyEquivalent: "").submenu = edit

        let view = NSMenu(title: "Ansicht")
        let fit = view.addItem(withTitle: "Fit alles", action: #selector(menuFit), keyEquivalent: "f")
        fit.keyEquivalentModifierMask = []
        view.addItem(withTitle: "Suche", action: #selector(menuPalette), keyEquivalent: "p")
        main.addItem(withTitle: "Ansicht", action: nil, keyEquivalent: "").submenu = view

        let session = NSMenu(title: "Session")
        session.addItem(withTitle: "Stoppen", action: #selector(menuStop), keyEquivalent: "w")
        main.addItem(withTitle: "Session", action: nil, keyEquivalent: "").submenu = session
        NSApp.mainMenu = main
    }

    @objc private func menuNewSession() { openNewSession(groupId: nil) }
    @objc private func menuFit() { canvas.fitAll() }
    @objc private func menuPalette() { togglePalette() }
    @objc private func menuStop() {
        guard let key = canvas.focusedKey, let s = canvas.session(key) else { NSSound.beep(); return }
        stopSession(s)
    }

    // MARK: Palette

    private func togglePalette() {
        if palette.isVisible { palette.switchToCommandMode(); return }
        dismissSheet()
        var src = PaletteWindow.Source()
        let sessions = canvas.sessions
        src.sessions = store.groups.flatMap { g in g.sessionIds.compactMap { sessions[$0] }.map { ($0, group: g, lines: attach?.lines(for: $0.id) ?? []) } }
        src.groups = store.groups
        src.onFocusSession = { [weak self] key in self?.canvas.focus(key) }
        src.onFitGroup = { [weak self] gid in self?.canvas.fitGroup(gid) }
        let focusedSession = canvas.focusedKey.flatMap { sessions[$0] }
        src.commands = [
            ("Fit alles", { [weak self] in self?.canvas.fitAll() }),
            ("Neue Session", { [weak self] in self?.openNewSession(groupId: nil) }),
            ("Alle anhängen", { [weak self] in self?.attachAll() }),
            ("Session stoppen (fokussierte)", { [weak self] in if let s = focusedSession { self?.stopSession(s) } }),
            ("Session schließen (fokussierte)", { [weak self] in if let s = focusedSession { self?.closeSession(s.id) } }),
            ("Gruppe bearbeiten (der fokussierten Session)", { [weak self] in
                if let s = focusedSession, let g = self?.canvas.group(forSession: s.id) { self?.openEditGroup(g.id) } }),
            ("Reload", { [weak self] in Task { await self?.registry.pollNow(); self?.canvas.applyLayout() } }),
        ] + store.groups.map { g in ("Zoom auf Gruppe \(g.name)", { [weak self] in self?.canvas.fitGroup(g.id) }) }
        palette.source = src
        palette.open(over: window)
    }

    private func attachAll() {
        for s in canvas.sessions.values where s.canAttach { attach.attachNow(s) }
    }

    // MARK: Sessions

    /// Rückfrage im App-Design. ⏎ bestätigt, Esc bricht ab.
    private func confirm(_ message: String, _ info: String, button: String, destructive: Bool = true, then action: @escaping () -> Void) {
        let run = { [weak self] in self?.dismissSheet(); action() }
        present(ConfirmView(title: message, info: info, button: button, destructive: destructive,
                            onConfirm: run, onCancel: { [weak self] in self?.dismissSheet() }),
                onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: run)
    }

    private func report(_ error: Error) {
        AppDelegate.log.error("\(String(describing: error), privacy: .public)")
        confirm("Claude CLI meldet einen Fehler", String(describing: error), button: "OK", destructive: false) {}
    }

    private func stopSession(_ s: Session) {
        guard let id = s.shortId else { NSSound.beep(); return }
        confirm("Session „\(s.name)“ stoppen?", "Die Konversation bleibt erhalten und lässt sich fortsetzen.", button: "Stoppen") { [weak self] in
            guard let self else { return }
            attach.detach(s.id)
            Task {
                do { try await self.cli.stop(id: id); await self.registry.pollNow() } catch { self.report(error) }
            }
        }
    }

    private func closeSession(_ key: String) {
        guard let s = canvas.session(key) else { return }
        guard let id = s.shortId else {
            confirm("Session „\(s.name)“ läuft in einem anderen Terminal", "Interaktive Sessions kann Kadrell nicht stoppen.", button: "OK", destructive: false) {}
            return
        }
        confirm("Session „\(s.name)“ stoppen und entfernen?", "claude stop \(id) und claude rm \(id). Das lässt sich nicht rückgängig machen.", button: "Entfernen") { [weak self] in
            guard let self else { return }
            let gid = canvas.group(forSession: key)?.id
            attach.detach(key)
            Task {
                do {
                    if !s.isDone { try? await self.cli.stop(id: id) }
                    try await self.cli.remove(id: id)
                    self.store.removeSession(key)
                    await self.registry.pollNow()
                    if let gid, self.store.group(id: gid) != nil { self.canvas.fitGroup(gid) }
                } catch { self.report(error) }
            }
        }
    }

    private func resume(_ s: Session) {
        Task {
            do {
                _ = try await cli.resume(sessionId: s.sessionId, cwd: s.cwd)
                if let fresh = await registry.waitFor(timeout: 10, { $0.id == s.id && $0.canAttach }) {
                    canvas.focus(fresh.id)
                }
            } catch { report(error) }
        }
    }

    private func closeGroup(_ gid: String) {
        guard let g = store.group(id: gid) else { return }
        let members = g.sessionIds.compactMap { canvas.session($0) }
        let bg = members.filter { $0.shortId != nil && !$0.isDone }
        confirm("Gruppe „\(g.name)“ mit \(members.count) Session(s) schließen?",
                bg.isEmpty ? "Die Gruppe wird aus Kadrell entfernt." : "\(bg.count) Hintergrund-Session(s) werden mit claude stop gestoppt.", button: "Schließen") { [weak self] in
            guard let self else { return }
            for s in members { attach.detach(s.id) }
            Task {
                for s in bg { if let id = s.shortId { try? await self.cli.stop(id: id) } }
                self.store.remove(id: gid)
                await self.registry.pollNow()
                self.canvas.reload(groups: self.store.groups, sessions: self.registry.sessions)
                self.canvas.fitAll()
            }
        }
    }

    // MARK: Sheets

    private func present<V: View>(_ view: V, onCancel: @escaping () -> Void, onPrimary: @escaping () -> Void) {
        dismissSheet()
        if palette.isVisible { palette.dismiss() }
        let p = OverlayPanel(rootView: view)
        p.onCancel = onCancel
        p.onPrimary = onPrimary
        overlay = p
        p.open(over: window)
    }

    private func dismissSheet() {
        overlay?.dismiss()
        overlay = nil
        window.makeFirstResponder(canvas)
    }

    private func openNewSession(groupId: String?) {
        let sessions = canvas.sessions
        let counts = Dictionary(uniqueKeysWithValues: store.groups.map { ($0.id, $0.sessionIds.filter { sessions[$0] != nil }.count) })
        let model = NewSessionModel(groups: store.groups, counts: counts, preselected: groupId.flatMap { store.group(id: $0) })
        let view = NewSessionView(model: model) { [weak self] g, cwd, prompt in self?.dismissSheet(); self?.startSession(group: g, cwd: cwd, prompt: prompt) }
        present(view, onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { model.start() })
    }

    private func startSession(group: Group?, cwd: String, prompt: String) {
        sessionCounter += 1
        let name = ClaudeCLI.shortName(prompt: prompt, cwd: cwd, counter: sessionCounter)
        Task {
            do {
                let shortId = try await cli.start(cwd: cwd, name: name, prompt: prompt)
                guard let fresh = await registry.waitFor(timeout: 10, { s in
                    (shortId != nil && s.shortId == shortId) || (s.name == name && s.cwd.hasSuffix(URL(fileURLWithPath: cwd).lastPathComponent))
                }) else {
                    report(CLIError(command: "claude --bg", status: 0, output: "Session „\(name)“ ist nach 10 s nicht in `claude agents` aufgetaucht."))
                    return
                }
                var target = group ?? store.group(forCwd: cwd)
                if target == nil {
                    let g = store.makeGroup(cwd: cwd)
                    store.add(g)
                    target = g
                }
                store.attach(sessionId: fresh.id, to: target!.id)
                canvas.reload(groups: store.groups, sessions: registry.sessions)
                updateBar()
                canvas.focus(fresh.id)
            } catch { report(error) }
        }
    }

    private func openEditGroup(_ gid: String) {
        guard let g = store.group(id: gid) else { return }
        let model = EditGroupModel(group: g)
        model.onSave = { [weak self] updated in
            guard let self else { return }
            dismissSheet()
            store.update(updated)
            canvas.reload(groups: store.groups, sessions: registry.sessions)
            updateBar()
            canvas.fitGroup(updated.id)
        }
        present(EditGroupView(model: model), onCancel: { [weak self] in self?.dismissSheet() }, onPrimary: { model.save() })
    }
}

final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}
