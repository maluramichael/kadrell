import XCTest
import SwiftUI
import Combine
@testable import Kadrell

/// F4-Dialog: Modell über einem echten Manager mit eigenem Temp-Katalog (nie der echte Extensions-Ordner).
/// Mit `KADRELL_RENDER_OUT=<pfad>.png` legt der Render-Test je Sprache ein Bild für die Sichtprüfung ab.
@MainActor
final class ExtensionsViewTests: XCTestCase {
    private var manager: ExtensionManager!
    private var catalog: URL!
    private var savedEnabled: [String] = []
    private let environment = ["HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]

    override func setUp() async throws {
        try await super.setUp()
        savedEnabled = Settings.enabledExtensions
        Settings.enabledExtensions = []
        catalog = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-cat-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: catalog, withIntermediateDirectories: true)
    }

    override func tearDown() async throws {
        if let manager {
            manager.shutdownAll()
            await until(5) { manager.found.allSatisfy { manager.state($0.name) == .off } }
        }
        try? FileManager.default.removeItem(at: catalog)
        Settings.enabledExtensions = savedEnabled
        manager = nil
        try await super.tearDown()
    }

    @discardableResult
    private func until(_ seconds: TimeInterval, _ condition: () -> Bool) async -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while !condition(), Date() < deadline { try? await Task.sleep(for: .milliseconds(20)) }
        return condition()
    }

    private func add(_ name: String, _ initLua: String, manifest: [String: Any]? = nil) throws {
        let dir = try ExtFixture.make(name: name, initLua: initLua, manifest: manifest)
        try FileManager.default.moveItem(at: dir, to: catalog.appendingPathComponent(name))
        try? FileManager.default.removeItem(at: dir.deletingLastPathComponent())
    }

    /// Ordner ohne Manifest: wird gefunden, hat ein Problem und lädt nicht.
    private func addBroken(_ name: String) throws {
        try FileManager.default.createDirectory(at: catalog.appendingPathComponent(name), withIntermediateDirectories: true)
    }

    private func startModel() -> ExtensionsModel {
        manager = ExtensionManager(environment: environment, control: { _ in .ok("") }, sessions: { [] }, catalogDir: catalog)
        manager.start()
        return ExtensionsModel(manager: manager)
    }

    private func logContains(_ name: String, _ text: String) -> Bool { manager.log(name).contains { $0.contains(text) } }

    func testModelListsFoundAndToggles() throws {
        try add("a", "")
        try addBroken("b")
        let model = startModel()
        XCTAssertEqual(model.items.map(\.name), ["a", "b"])
        XCTAssertTrue(model.canToggle(model.items[0]))
        XCTAssertFalse(model.canToggle(model.items[1]), "kaputte Extension lässt sich nicht einschalten")
        XCTAssertNotNil(model.items[1].problem)
    }

    func testSpaceTogglesSelected() throws {
        try add("a", "")
        try add("c", "")
        let model = startModel()
        model.select(0)
        XCTAssertTrue(model.handleKey(" "))
        XCTAssertEqual(manager.state("a"), .starting)
        XCTAssertTrue(model.isEnabled("a"))
        XCTAssertTrue(model.handleKey(" "))
        XCTAssertFalse(model.isEnabled("a"))

        XCTAssertTrue(model.handleKey(.downArrow))
        XCTAssertEqual(model.selected, 1)
        XCTAssertTrue(model.handleKey(.downArrow))
        XCTAssertEqual(model.selected, 1, "bleibt am Ende stehen")
        XCTAssertTrue(model.handleKey(.upArrow))
        XCTAssertEqual(model.selected, 0)
        XCTAssertTrue(model.handleKey("l"))
        XCTAssertTrue(model.showLog)
        XCTAssertFalse(model.handleKey("x"), "fremde Tasten gehen weiter")
    }

    /// Aus dem Tasten-Monitor: nur Tasten ohne Modifier, Pfeile über ihren Code.
    func testKeyFromEvent() throws {
        func event(_ chars: String, _ code: UInt16, _ mods: NSEvent.ModifierFlags = []) throws -> NSEvent {
            try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: 0, windowNumber: 0, context: nil,
                                           characters: chars, charactersIgnoringModifiers: chars, isARepeat: false, keyCode: code))
        }
        XCTAssertEqual(ExtensionsModel.key(for: try event(" ", KeyCode.space)), .space)
        XCTAssertEqual(ExtensionsModel.key(for: try event("r", 15)), "r")
        XCTAssertEqual(ExtensionsModel.key(for: try event("\u{F700}", KeyCode.up, [.numericPad, .function])), .upArrow)
        XCTAssertNil(ExtensionsModel.key(for: try event("r", 15, .command)), "⌘R bleibt ein Kürzel der App")
    }

    func testReloadKeyRestartsRunning() async throws {
        try add("a", "kadrell.log('start')")
        let model = startModel()
        model.select(0)
        model.handleKey(" ")
        let running = await until(5) { self.manager.state("a") == .running }
        XCTAssertTrue(running, "\(manager.log("a"))")
        XCTAssertTrue(model.handleKey("r"))
        XCTAssertEqual(manager.state("a"), .reloading)
    }

    /// Einstellung gespeichert: eingeschaltete Extension startet neu und sieht den neuen Wert, abgeschaltete bleibt aus.
    func testSavingSettingRestartsEnabledExtension() async throws {
        let (model, ext) = try await greetingExtension()
        let a = ext.name
        model.handleKey(" ")
        await until(5) { self.manager.state(a) == .off }

        await model.save("greeting", .string("servus"), in: ext)
        XCTAssertEqual(manager.state(a), .off, "abgeschaltete Extension startet nicht")
        XCTAssertEqual(model.stored["greeting"], .string("servus"))

        model.handleKey(" ")
        let first = await until(5) { self.logContains(a, "greeting=servus") && self.manager.state(a) == .running }
        XCTAssertTrue(first, "\(manager.log(a))")
        await model.save("greeting", .string("moin"), in: ext)
        let restarted = await until(5) { self.logContains(a, "greeting=moin") }
        XCTAssertTrue(restarted, "\(manager.log(a))")
    }

    /// Eindeutiger Name: Schlüsselbund und Defaults hängen am Namen, ein Test darf nie die Werte einer echten Extension „a“ löschen.
    private func uniqueName() -> String { "kadrell-test-\(UUID().uuidString.lowercased())" }

    /// Eine Extension mit zwei Textfeldern, die beim Start ihren Gruß ins Log schreibt; eingeschaltet und laufend.
    private func greetingExtension() async throws -> (ExtensionsModel, FoundExtension) {
        let name = uniqueName()
        let manifest: [String: Any] = ["name": name, "apiVersion": 1, "settings": [
            ["key": "greeting", "type": "string", "label": "Gruß", "default": "hi"],
            ["key": "other", "type": "string", "label": "Anderes", "default": ""],
        ]]
        try add(name, "kadrell.log('greeting=' .. tostring(kadrell.config.greeting))", manifest: manifest)
        let model = startModel()
        let ext = try XCTUnwrap(model.items.first)
        addTeardownBlock {
            await ExtensionSettings.set(.null, for: ext, key: "greeting")
            await ExtensionSettings.set(.null, for: ext, key: "other")
        }
        model.handleKey(" ")
        let running = await until(5) { self.logContains(name, "greeting=hi") && self.manager.state(name) == .running }
        XCTAssertTrue(running, "\(manager.log(name))")
        await model.load()
        return (model, ext)
    }

    private func greetings(_ name: String) -> Int { manager.log(name).filter { $0.hasPrefix("greeting=") }.count }

    /// Der Dialog wie vom `SheetPresenter` geöffnet: OverlayPanel über einem Elternfenster, Modell kennt sein Fenster.
    private func openDialog(_ model: ExtensionsModel) async throws -> (panel: OverlayPanel, fields: [NSTextField]) {
        let parent = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 1200, height: 1000), styleMask: [.titled], backing: .buffered, defer: false)
        parent.isReleasedWhenClosed = false
        parent.orderFront(nil)
        let panel = OverlayPanel(rootView: ExtensionsView(model: model))
        panel.open(over: parent)
        model.window = panel
        addTeardownBlock { @MainActor [weak panel] in
            panel?.dismiss()
            parent.close()
        }
        try await Task.sleep(for: .milliseconds(300))
        func fields(_ v: NSView) -> [NSTextField] { ((v as? NSTextField).map { $0.isEditable ? [$0] : [] } ?? []) + v.subviews.flatMap(fields) }
        // Das Suchfeld des „Entdecken"-Bereichs ist kein Einstellungsfeld der Extension.
        let setting = fields(try XCTUnwrap(panel.contentView)).filter { $0.placeholderString?.contains("GitHub") != true }
        return (panel, setting)
    }

    /// Tippt in das Feld wie über die Tastatur (Feldeditor), ohne zu bestätigen.
    @discardableResult
    private func type(_ text: String, into field: NSTextField, in window: NSWindow) throws -> NSTextView {
        window.makeFirstResponder(field)
        let editor = try XCTUnwrap(window.firstResponder as? NSTextView, "Feldeditor hat die Tastatur")
        editor.selectAll(nil)
        editor.insertText(text, replacementRange: editor.selectedRange())
        return editor
    }

    /// Ruling 14: ⏎ speichert, Schließen verwirft den offenen Entwurf, ohne Speichern und ohne Neustart.
    func testClosingDiscardsUncommittedDraft() async throws {
        let (model, ext) = try await greetingExtension()
        let (panel, fields) = try await openDialog(model)
        XCTAssertEqual(fields.count, 2)
        XCTAssertFalse(panel.firstResponder is NSText, "beim Öffnen hat kein Feld die Tastatur")

        try type("servus", into: fields[0], in: panel).insertNewline(nil)
        let saved = await until(5) { model.stored["greeting"] == .string("servus") && self.logContains(ext.name, "greeting=servus") }
        XCTAssertTrue(saved, "⏎ speichert und startet neu: \(manager.log(ext.name))")
        await until(5) { self.manager.state(ext.name) == .running }
        let starts = greetings(ext.name)

        try type("halb getippt", into: fields[0], in: panel)
        try await Task.sleep(for: .milliseconds(100))
        // Wie SheetPresenter.dismiss: erst das Modell vom Fenster lösen, dann ausblenden.
        model.window = nil
        panel.dismiss()
        // Ob AppKit beim Abbau noch einen Fokusverlust meldet, ist nicht festgelegt: hier erzwungen, er darf nichts speichern.
        panel.makeFirstResponder(nil)
        try await Task.sleep(for: .milliseconds(600))
        let values = await ExtensionSettings.values(for: ext)
        XCTAssertEqual(values["greeting"], .string("servus"), "Entwurf nicht gespeichert")
        XCTAssertEqual(greetings(ext.name), starts, "kein Neustart: \(manager.log(ext.name))")
        XCTAssertEqual(manager.state(ext.name), .running)
    }

    /// Tab ins nächste Feld speichert das verlassene wie ⏎.
    func testMovingToAnotherFieldCommits() async throws {
        let (model, ext) = try await greetingExtension()
        let (panel, fields) = try await openDialog(model)
        try type("moin", into: fields[0], in: panel)
        try await Task.sleep(for: .milliseconds(100))
        panel.makeFirstResponder(fields[1])
        XCTAssertTrue(panel.firstResponder is NSText)
        let saved = await until(5) { self.logContains(ext.name, "greeting=moin") }
        XCTAssertTrue(saved, "\(manager.log(ext.name))")
    }

    /// Klick auf eine Zeile beendet das Textfeld: die Tasten gehören wieder der Liste, der Entwurf ist verworfen.
    func testRowClickEndsEditingAndDiscardsDraft() async throws {
        let (model, ext) = try await greetingExtension()
        let (panel, fields) = try await openDialog(model)
        let starts = greetings(ext.name)
        try type("verworfen", into: fields[0], in: panel)
        try await Task.sleep(for: .milliseconds(100))
        model.click(0)
        XCTAssertFalse(panel.firstResponder is NSText, "Feldeditor hat die Tastatur nicht mehr")
        try await Task.sleep(for: .milliseconds(500))
        let values = await ExtensionSettings.values(for: ext)
        XCTAssertEqual(values["greeting"], .string("hi"))
        XCTAssertEqual(greetings(ext.name), starts)
    }

    /// Hat ein Textfeld die Tastatur, gehen Leertaste, R und L ins Feld; sonst an den Dialog.
    func testPanelKeysPassThroughWhileEditing() throws {
        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 300, height: 100), styleMask: [.titled], backing: .buffered, defer: false)
        panel.isReleasedWhenClosed = false
        defer { panel.close() }
        let host = NSHostingView(rootView: Text(verbatim: "x"))
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 200, height: 22))
        let root = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        root.addSubview(host)
        root.addSubview(field)
        panel.contentView = root
        func event(_ chars: String, _ code: UInt16) -> NSEvent {
            NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: panel.windowNumber, context: nil,
                             characters: chars, charactersIgnoringModifiers: chars, isARepeat: false, keyCode: code)!
        }
        let keys = [event(" ", KeyCode.space), event("r", 15), event("l", 37)]

        panel.makeFirstResponder(field)
        XCTAssertTrue(panel.firstResponder is NSTextView, "Feldeditor")
        for e in keys { XCTAssertNil(ExtensionsModel.panelKey(e, in: panel), "\(e.characters ?? "")") }

        panel.makeFirstResponder(host)
        XCTAssertFalse(panel.firstResponder is NSText)
        XCTAssertEqual(keys.compactMap { ExtensionsModel.panelKey($0, in: panel) }, [" ", "r", "l"])
        panel.makeFirstResponder(nil)
        XCTAssertEqual(keys.compactMap { ExtensionsModel.panelKey($0, in: panel) }, [" ", "r", "l"])
    }

    /// Ruling 15: F4 bei offenem F1 kommt erst nach dem Schließen dran, muss dann aber als F4-Panel gelten.
    func testDeferredPanelKeepsItsKind() throws {
        try add("a", "")
        let model = startModel()
        let sheets = SheetPresenter(host: { nil }, palette: { nil })
        sheets.extensionsModel = model
        sheets.togglePanel(.about)
        sheets.togglePanel(.extensions)
        XCTAssertEqual(sheets.openPanel, .about, "F4 wartet, bis F1 zu ist")
        XCTAssertNil(model.window)
        sheets.dismiss()
        XCTAssertEqual(sheets.openPanel, .extensions)
        XCTAssertNotNil(model.window, "Formular nimmt Werte an")
        sheets.dismiss()
        XCTAssertNil(sheets.openPanel)
        XCTAssertNil(model.window, "nach dem Schließen speichert das Formular nichts mehr")
    }

    /// Ein gesetztes Geheimnis kommt nur als „gesetzt“ im Modell an, nie sein Wert.
    func testSecretIsNeverExposed() async throws {
        let name = uniqueName()
        let manifest: [String: Any] = ["name": name, "apiVersion": 1, "settings": [
            ["key": "token", "type": "secret", "label": "Token"],
        ]]
        try add(name, "", manifest: manifest)
        let model = startModel()
        let ext = try XCTUnwrap(model.items.first)
        addTeardownBlock { await ExtensionSettings.set(.null, for: ext, key: "token") }
        model.select(0)
        await model.load()
        XCTAssertFalse(model.secretsSet.contains("token"))
        await model.save("token", .string("geheim-123"), in: ext)
        XCTAssertTrue(model.secretsSet.contains("token"))
        XCTAssertNil(model.stored["token"])
        XCTAssertFalse(model.stored.values.contains(.string("geheim-123")))
    }

    /// Bis zu 50 Logzeilen pro Sekunde: das Modell meldet höchstens alle 0,2 s eine Änderung.
    func testChangesAreCoalesced() async throws {
        try add("a", "")
        let model = startModel()
        var count = 0
        let sub = model.objectWillChange.sink { count += 1 }
        defer { sub.cancel() }
        for _ in 0..<100 { manager.onChange() }
        try await Task.sleep(for: .milliseconds(500))
        XCTAssertGreaterThanOrEqual(count, 1)
        XCTAssertLessThanOrEqual(count, 4, "100 Meldungen in 0,5 s ergeben nur wenige Neuzeichnungen")
    }

    func testDialogRenderSmoke() async throws {
        let settings: [String: Any] = ["name": "a", "version": "1.2.0", "apiVersion": 1,
                                       "description": "Zeigt offene Pull-Requests", "permissions": ["exec", "http"],
                                       "settings": [["key": "repo", "type": "string", "label": "Repository", "default": "kadrell"],
                                                    ["key": "token", "type": "secret", "label": "API-Token"],
                                                    ["key": "drafts", "type": "bool", "label": "Entwürfe zeigen", "default": false]]]
        try add("a", "kadrell.log('bereit')", manifest: settings)
        try add("b", "error('kaputt')", manifest: ["name": "b", "version": "0.3.0", "apiVersion": 1, "description": "Stürzt beim Start ab"])
        try add("c", "", manifest: ["name": "c", "version": "2.0.0", "apiVersion": 1, "description": "Abgeschaltet"])
        try addBroken("d")
        let model = startModel()
        manager.setEnabled("a", true)
        manager.setEnabled("b", true)
        let ready = await until(5) {
            guard case .failed = self.manager.state("b") else { return false }
            return self.manager.state("a") == .running
        }
        XCTAssertTrue(ready, "a: \(manager.state("a")), b: \(manager.state("b"))")
        model.select(0)
        await model.load()
        model.showLog = true

        let out = ProcessInfo.processInfo.environment["KADRELL_RENDER_OUT"]
        // Zustandstexte kommen aus Bundle.app, also die Sprache wie in der App umschalten, nicht nur \.locale.
        defer {
            Profile.defaults.removeObject(forKey: Settings.languageKey)
            Localization.apply(.system)
        }
        for language in [Localization.Language.de, .en] {
            Settings.language = language
            let host = NSHostingView(rootView: ExtensionsView(model: model).environment(\.locale, Localization.locale))
            host.layout()
            let size = host.fittingSize
            XCTAssertGreaterThan(size.width, 400, "\(language)")
            XCTAssertGreaterThan(size.height, 300, "\(language)")
            guard let out else { continue }
            host.frame = NSRect(origin: .zero, size: size)
            guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { return XCTFail("kein Bitmap") }
            host.cacheDisplay(in: host.bounds, to: rep)
            guard let png = rep.representation(using: .png, properties: [:]) else { return XCTFail("kein PNG") }
            try png.write(to: URL(fileURLWithPath: out.replacingOccurrences(of: ".png", with: language == .de ? ".png" : "-en.png")))
        }
    }
}
