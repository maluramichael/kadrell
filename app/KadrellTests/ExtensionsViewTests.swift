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
        let manifest: [String: Any] = ["name": "a", "apiVersion": 1, "settings": [
            ["key": "greeting", "type": "string", "label": "Gruß", "default": "hi"],
        ]]
        try add("a", "kadrell.log('greeting=' .. tostring(kadrell.config.greeting))", manifest: manifest)
        let model = startModel()
        let ext = try XCTUnwrap(model.items.first)
        addTeardownBlock { await ExtensionSettings.set(.null, for: ext, key: "greeting") }
        model.select(0)

        await model.save("greeting", .string("servus"), in: ext)
        XCTAssertEqual(manager.state("a"), .off, "abgeschaltete Extension startet nicht")
        XCTAssertEqual(model.stored["greeting"], .string("servus"))

        model.handleKey(" ")
        let first = await until(5) { self.logContains("a", "greeting=servus") && self.manager.state("a") == .running }
        XCTAssertTrue(first, "\(manager.log("a"))")
        await model.save("greeting", .string("moin"), in: ext)
        let restarted = await until(5) { self.logContains("a", "greeting=moin") }
        XCTAssertTrue(restarted, "\(manager.log("a"))")
    }

    /// Ein gesetztes Geheimnis kommt nur als „gesetzt“ im Modell an, nie sein Wert.
    func testSecretIsNeverExposed() async throws {
        let manifest: [String: Any] = ["name": "a", "apiVersion": 1, "settings": [
            ["key": "token", "type": "secret", "label": "Token"],
        ]]
        try add("a", "", manifest: manifest)
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
