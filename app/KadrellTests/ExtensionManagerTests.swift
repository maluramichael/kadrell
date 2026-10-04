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
        // Eindeutiger Name: der Teardown löscht den Schlüsselbund-Eintrag, nie den einer echten Extension „a“.
        let a = "kadrell-test-\(UUID().uuidString.lowercased())"
        let manifest: [String: Any] = ["name": a, "apiVersion": 1, "settings": [
            ["key": "token", "type": "secret", "label": "Token"],
            ["key": "mine", "type": "bool", "label": "Mine", "default": true],
        ]]
        try make(a, "kadrell.log('token=' .. tostring(kadrell.config.token) .. ' mine=' .. tostring(kadrell.config.mine))",
                 manifest: manifest)
        let ext = try XCTUnwrap(manager.found.first)
        addTeardownBlock { await ExtensionSettings.set(.null, for: ext, key: "token") }
        await ExtensionSettings.set(.string("abc"), for: ext, key: "token")
        manager.setEnabled(a, true)
        let passed = await until(5) { self.logContains(a, "token=abc mine=true") }
        XCTAssertTrue(passed, "\(manager.log(a))")
        await ExtensionSettings.set(.null, for: ext, key: "token")
        let left = await Keychain.read(service: ExtensionSettings.keychainService, account: ExtensionSettings.account(a, "token"))
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

    /// Panel über 2000 Knoten: wie ein Absturz, Grund nennt die Grenze, Panel bleibt leer. Der Prozess stirbt sofort
    /// (SIGKILL), auch wenn er danach hängt; ein sanftes Stoppen bräuchte hier die Sekunde bis zum SIGTERM.
    func testTooLargePanelCountsAsCrash() async throws {
        try make("a", """
        kadrell.on("app.ready", function()
            local children = {}
            for i = 1, 2001 do children[i] = { type = "text", text = "x" } end
            kadrell.panel.set({ title = "A", children = children })
            kadrell.after(0, function() while true do end end)
        end)
        """)
        manager.setEnabled("a", true)
        manager.emit("app.ready", .object([:]))
        let failed = await until(3) { if case .failed = self.manager.state("a") { true } else { false } }
        XCTAssertTrue(failed, "\(manager.state("a"))")
        let killed = await until(0.5) { self.helpers(in: self.catalog) == 0 }
        XCTAssertTrue(killed, "Helper lebt nach dem zu großen Panel weiter")
        guard case .failed(let reason) = manager.state("a") else { return }
        XCTAssertEqual(reason, String(localized: "Panel zu groß (\(2001) Knoten, höchstens \(PanelValidation.maxNodes))", bundle: Bundle.app))
        XCTAssertTrue(manager.panels.isEmpty)
    }

    /// A2: Abschalten während des Starts, sofort wieder an. Das späte ready des alten Prozesses darf den neuen Start
    /// nicht verhindern: am Ende läuft genau ein neuer Prozess, und das Panel kommt von ihm.
    func testDisableDuringStartThenEnable() async throws {
        let name = "kadrell-test-\(UUID().uuidString.lowercased())"
        try make(name, """
        local n = (kadrell.storage.get("n") or 0) + 1
        kadrell.storage.set("n", n)
        local t = os.clock()
        while os.clock() - t < 0.4 do end
        kadrell.on("app.ready", function() kadrell.panel.set({ title = "run " .. n, children = {} }) end)
        """)
        manager.emit("app.ready", .object([:]))
        manager.setEnabled(name, true)
        let launched = await until(3) { self.helpers(in: self.catalog) == 1 }
        XCTAssertTrue(launched)
        XCTAssertEqual(manager.state(name), .starting)
        manager.setEnabled(name, false)
        // Hauptthread blockieren: das ready des alten Prozesses liegt danach schon in der Main-Queue, vor dem neuen Start.
        usleep(1_000_000)
        manager.setEnabled(name, true)
        let running = await until(5) { self.manager.panels.first?.tree.title == "run 2" && self.helpers(in: self.catalog) == 1 }
        XCTAssertTrue(running, "state \(manager.state(name)), panels \(manager.panels.map(\.tree.title)), helpers \(helpers(in: catalog))")
        XCTAssertEqual(manager.state(name), .running)
    }

    /// A2: ein kadrell.run, das nach dem Abschalten noch ankommt, wird nicht mehr ausgeführt.
    func testRunAfterDisableIsNotExecuted() async throws {
        try make("a", """
        kadrell.on("app.ready", function()
            local t = os.clock()
            while os.clock() - t < 0.4 do end
            kadrell.run("late")
        end)
        """)
        manager.setEnabled("a", true)
        manager.emit("app.ready", .object([:]))
        await until(3) { self.manager.state("a") == .running }
        manager.setEnabled("a", false)
        let gone = await until(4) { self.helpers(in: self.catalog) == 0 }
        XCTAssertTrue(gone)
        try await Task.sleep(for: .milliseconds(200))
        XCTAssertFalse(calls.contains(["late"]), "\(calls)")
    }

    /// Spec: session.focus auch beim Start. Eine später eingeschaltete Extension erfährt die fokussierte Session.
    func testLateStarterGetsCurrentFocus() async throws {
        try make("a", #"kadrell.on("session.focus", function(d) kadrell.log("focus " .. d.session.key) end)"#)
        manager.emit("app.ready", .object([:]))
        manager.emit("session.focus", .object(["session": .object(["key": .string("s1")])]))
        manager.setEnabled("a", true)
        let got = await until(3) { self.logContains("a", "focus s1") }
        XCTAssertTrue(got, "\(manager.log("a"))")
    }

    /// FSEvents: der Katalog verschwindet und entsteht zweimal hintereinander neu, der Watcher sieht jedes Mal den neuen.
    func testCatalogDeletedAndRecreatedTwice() async throws {
        let dir = try make("a", "kadrell.log('v1')")
        for round in 1...2 {
            try FileManager.default.removeItem(at: catalog)
            let empty = await until(2) { self.manager.found.isEmpty }
            XCTAssertTrue(empty, "Runde \(round): Löschen nicht gesehen")
            let fresh = try ExtFixture.make(name: "a", initLua: "kadrell.log('v\(round + 1)')")
            try FileManager.default.moveItem(at: fresh.deletingLastPathComponent(), to: catalog)
            let seen = await until(2) { self.manager.found.map(\.name) == ["a"] }
            XCTAssertTrue(seen, "Runde \(round): Neuanlage nicht gesehen")
        }
        manager.setEnabled("a", true)
        await until(3) { self.manager.state("a") == .running }
        try Data("kadrell.log('changed')".utf8).write(to: dir.appendingPathComponent("init.lua"))
        let reloaded = await until(2) { self.logContains("a", "changed") }
        XCTAssertTrue(reloaded, "\(manager.log("a"))")
    }

    /// Das Quick-start-Beispiel von https://kadrell.malura.de/extensions, Byte für Byte wie auf der Seite.
    /// Ändert sich die Seite oder dieser Test allein, stimmt die Doku nicht mehr.
    func testDocsQuickStartShowsPanelAndStatus() async throws {
        let manifest = """
        {
          "name": "hello",
          "version": "0.1.0",
          "description": "Counts sessions in the right sidebar and the status bar",
          "apiVersion": 1
        }
        """
        let initLua = """
        local function count()
          local n = 0
          for _, group in ipairs(kadrell.sessions().groups) do n = n + #group.sessions end
          return n
        end

        local function render()
          local text = count() .. " sessions"
          kadrell.panel.set{
            title = "Hello",
            children = {
              { type = "text", text = text, color = "muted" },
              { type = "button", label = "Refresh", action = "refresh" },
            },
          }
          kadrell.status.set{ text = text, color = "ok", action = "refresh" }
        end

        kadrell.on("app.ready", render)
        kadrell.on("session.new", render)
        kadrell.on("session.remove", render)
        kadrell.on("ui.action", render)
        """
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-ext-\(UUID().uuidString)")
        let dir = root.appendingPathComponent("hello")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data(manifest.utf8).write(to: dir.appendingPathComponent("kadrell.json"))
        try Data(initLua.utf8).write(to: dir.appendingPathComponent("init.lua"))
        catalog = root
        var sessions = 2
        manager = ExtensionManager(environment: environment, control: { _ in
            // Form von `kadrell ls --json`: Gruppen mit Sessions.
            .ok(#"{"layout":"grid","groups":[{"sessions":[\#(Array(repeating: "{}", count: sessions).joined(separator: ","))]}]}"#)
        }, sessions: { [] }, catalogDir: root)
        manager.start()
        XCTAssertNil(manager.found.first?.problem)
        manager.setEnabled("hello", true)
        manager.emit("app.ready", .object([:]))
        let shown = await until(3) { self.manager.statusItems.first?.text == "2 sessions" }
        XCTAssertTrue(shown, "\(manager.log("hello"))")
        XCTAssertEqual(manager.panels.first?.tree.title, "Hello")
        XCTAssertEqual(manager.panels.first?.tree.nodes, [.text("2 sessions", .muted), .button(label: "Refresh", action: "refresh")])
        XCTAssertEqual(manager.statusItems.first?.color, .ok)
        sessions = 3
        manager.action("hello", id: "refresh")
        let refreshed = await until(3) { self.manager.statusItems.first?.text == "3 sessions" }
        XCTAssertTrue(refreshed, "\(manager.log("hello"))")
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
