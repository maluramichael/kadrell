import XCTest
@testable import Kadrell

/// `ExtensionManager` mit echtem `ext-host`, eigenem Temp-Katalog und gefälschtem Steuerweg.
@MainActor
final class ExtensionManagerTests: XCTestCase {
    private var manager: ExtensionManager!
    private var catalog: URL!
    private var calls: [[String]] = []
    private var savedEnabled: [String] = []
    // Eigene Umgebung: die des Test-Hosts trägt die XCTest-Injektion (DYLD_*), die soll der Helper nicht erben.
    private let environment = ["HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]

    override func setUp() async throws {
        try await super.setUp()
        savedEnabled = Settings.enabledExtensions
    }

    override func tearDown() async throws {
        manager?.shutdownAll()
        if let catalog {
            let gone = await until(5) { self.helpers(in: catalog) == 0 }
            XCTAssertTrue(gone, "kein Helper bleibt nach shutdownAll übrig")
            try? FileManager.default.removeItem(at: catalog)
        }
        Settings.enabledExtensions = savedEnabled
        manager = nil
        catalog = nil
        calls = []
        try await super.tearDown()
    }

    @discardableResult
    private func until(_ seconds: TimeInterval, _ condition: () -> Bool) async -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while !condition(), Date() < deadline { try? await Task.sleep(for: .milliseconds(20)) }
        return condition()
    }

    /// Legt die Extension in einem frischen Katalog an und startet einen Manager darauf.
    @discardableResult
    private func make(_ name: String, _ initLua: String, manifest: [String: Any]? = nil) throws -> URL {
        let dir = try ExtFixture.make(name: name, initLua: initLua, manifest: manifest)
        startManager(catalog: dir.deletingLastPathComponent())
        return dir
    }

    private func startManager(catalog: URL) {
        self.catalog = catalog
        manager = ExtensionManager(environment: environment, control: { [unowned self] argv in
            calls.append(argv)
            return .ok("fake")
        }, sessions: { [] }, catalogDir: catalog)
        manager.start()
    }

    private func logContains(_ name: String, _ text: String) -> Bool { manager.log(name).contains { $0.contains(text) } }

    /// Laufende Helper für Extensions unter `dir`, gezählt über die Prozessliste.
    private func helpers(in dir: URL) -> Int {
        let ps = Process(), out = Pipe()
        ps.executableURL = URL(fileURLWithPath: "/bin/ps")
        ps.arguments = ["-axww", "-o", "args="]
        ps.standardOutput = out
        guard (try? ps.run()) != nil else { return -1 }
        let data = out.fileHandleForReading.readDataToEndOfFile()
        ps.waitUntilExit()
        return String(decoding: data, as: UTF8.self).split(separator: "\n").filter { $0.contains("ext-host " + dir.path) }.count
    }

    private let panelOnReady = """
    kadrell.on("app.ready", function() kadrell.panel.set({ title = "A", children = {} }) end)
    """

    func testEnableStartsAndPanelArrives() async throws {
        try make("a", panelOnReady)
        manager.setEnabled("a", true)
        manager.emit("app.ready", .object([:]))
        let arrived = await until(3) { self.manager.panels.first?.tree.title == "A" }
        XCTAssertTrue(arrived, "\(manager.log("a"))")
        XCTAssertEqual(manager.state("a"), .running)
        XCTAssertTrue(Settings.enabledExtensions.contains("a"))
    }

    func testDisableClearsPanelImmediately() async throws {
        try make("a", panelOnReady)
        manager.setEnabled("a", true)
        manager.emit("app.ready", .object([:]))
        await until(3) { !self.manager.panels.isEmpty }
        manager.setEnabled("a", false)
        XCTAssertTrue(manager.panels.isEmpty)
        XCTAssertEqual(manager.state("a"), .off)
        XCTAssertFalse(Settings.enabledExtensions.contains("a"))
    }

    func testRunGoesThroughControl() async throws {
        try make("a", """
        kadrell.on("app.ready", function()
            local r = kadrell.run("ls")
            kadrell.log("status " .. r.status .. " " .. r.stdout)
        end)
        """)
        manager.setEnabled("a", true)
        manager.emit("app.ready", .object([:]))
        let answered = await until(3) { self.logContains("a", "status 0 fake") }
        XCTAssertTrue(answered, "\(manager.log("a"))")
        XCTAssertEqual(calls, [["ls"]])
    }

    func testFileChangeReloads() async throws {
        let dir = try make("a", "kadrell.log('v1')")
        manager.setEnabled("a", true)
        let started = await until(3) { self.manager.state("a") == .running && self.logContains("a", "v1") }
        XCTAssertTrue(started, "\(manager.log("a"))")
        // Ohne .atomic: in die bestehende Datei schreiben, wie es manche Editoren tun.
        try Data("kadrell.log('v2')".utf8).write(to: dir.appendingPathComponent("init.lua"))
        let reloaded = await until(2) { self.manager.state("a") == .running && self.logContains("a", "v2") }
        XCTAssertTrue(reloaded, "\(manager.log("a"))")
    }

    /// Review Focus 4: Ordner einer laufenden Extension verschwindet.
    func testDeletedFolderStopsCleanly() async throws {
        let dir = try make("a", "kadrell.log('hi')")
        manager.setEnabled("a", true)
        await until(3) { self.manager.state("a") == .running }
        XCTAssertEqual(helpers(in: catalog), 1)
        try FileManager.default.removeItem(at: dir)
        let gone = await until(3) { self.manager.found.isEmpty && self.helpers(in: self.catalog) == 0 }
        XCTAssertTrue(gone, "found: \(manager.found.map(\.name))")
        XCTAssertEqual(manager.state("a"), .off)
    }

    /// Umbenannt: alter Name verschwindet samt Prozess, der neue erscheint aus (Name passt nicht mehr zum Manifest).
    func testRenamedFolderStopsAndAppearsOff() async throws {
        let dir = try make("a", "kadrell.log('hi')")
        manager.setEnabled("a", true)
        await until(3) { self.manager.state("a") == .running }
        try FileManager.default.moveItem(at: dir, to: catalog.appendingPathComponent("b"))
        let moved = await until(3) { self.manager.found.map(\.name) == ["b"] && self.helpers(in: self.catalog) == 0 }
        XCTAssertTrue(moved, "found: \(manager.found.map(\.name))")
        XCTAssertEqual(manager.state("b"), .off)
        XCTAssertNotNil(manager.found.first?.problem)
    }

    func testSecretSettingIsPassedInConfig() async throws {
        let manifest: [String: Any] = ["name": "a", "apiVersion": 1, "settings": [
            ["key": "token", "type": "secret", "label": "Token"],
            ["key": "mine", "type": "bool", "label": "Mine", "default": true],
        ]]
        try make("a", "kadrell.log('token=' .. tostring(kadrell.config.token) .. ' mine=' .. tostring(kadrell.config.mine))",
                 manifest: manifest)
        let ext = try XCTUnwrap(manager.found.first)
        addTeardownBlock { await ExtensionSettings.set(.null, for: ext, key: "token") }
        await ExtensionSettings.set(.string("abc"), for: ext, key: "token")
        manager.setEnabled("a", true)
        let passed = await until(5) { self.logContains("a", "token=abc mine=true") }
        XCTAssertTrue(passed, "\(manager.log("a"))")
        await ExtensionSettings.set(.null, for: ext, key: "token")
        let left = await Keychain.read(service: ExtensionSettings.keychainService, account: ExtensionSettings.account("a", "token"))
        XCTAssertNil(left, "null löscht den Eintrag im Schlüsselbund")
    }

    /// Der Extensions-Ordner entsteht erst nach dem Start: wird trotzdem gesehen, auch Änderungen darin.
    func testCatalogCreatedLater() async throws {
        let fixture = try ExtFixture.make(name: "a", initLua: "kadrell.log('v1')")
        startManager(catalog: FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-cat-\(UUID().uuidString)"))
        XCTAssertTrue(manager.found.isEmpty)
        try FileManager.default.moveItem(at: fixture.deletingLastPathComponent(), to: catalog)
        let seen = await until(2) { self.manager.found.map(\.name) == ["a"] }
        XCTAssertTrue(seen)
        manager.setEnabled("a", true)
        await until(3) { self.manager.state("a") == .running }
        try Data("kadrell.log('v2')".utf8).write(to: catalog.appendingPathComponent("a/init.lua"))
        let reloaded = await until(2) { self.logContains("a", "v2") }
        XCTAssertTrue(reloaded, "\(manager.log("a"))")
    }

    /// Panel über 2000 Knoten: wie ein Absturz, Grund nennt die Grenze, Panel bleibt leer.
    func testTooLargePanelCountsAsCrash() async throws {
        try make("a", """
        kadrell.on("app.ready", function()
            local children = {}
            for i = 1, 2001 do children[i] = { type = "text", text = "x" } end
            kadrell.panel.set({ title = "A", children = children })
        end)
        """)
        manager.setEnabled("a", true)
        manager.emit("app.ready", .object([:]))
        let failed = await until(3) { if case .failed = self.manager.state("a") { true } else { false } }
        XCTAssertTrue(failed, "\(manager.state("a"))")
        guard case .failed(let reason) = manager.state("a") else { return }
        XCTAssertEqual(reason, String(localized: "Panel zu groß (\(2001) Knoten, höchstens \(PanelValidation.maxNodes))", bundle: Bundle.app))
        XCTAssertTrue(manager.panels.isEmpty)
    }

    func testSessionEvents() {
        func s(_ id: String, _ status: String) -> Session {
            var s = Session(id: id, cwd: "/tmp", startedAt: 0, sessionId: id, name: id)
            s.rawStatus = status
            return s
        }
        let old = ["a": s("a", "idle"), "b": s("b", "busy"), "c": s("c", "idle")]
        let events = AppDelegate.sessionEvents(from: old, to: [s("a", "idle"), s("b", "waiting"), s("d", "busy")])
        XCTAssertEqual(events.map { "\($0.event) \($0.session.id)" }, ["session.status b", "session.new d", "session.remove c"])
    }
}
