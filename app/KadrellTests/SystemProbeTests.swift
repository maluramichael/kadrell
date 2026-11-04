import XCTest
@testable import Kadrell

/// `kadrell.system`, `kadrell.footer` und `app.locale` im ext-host, dazu die neuen Manifest-Typen und der eigene
/// Extensions-Ordner.
final class SystemProbeTests: XCTestCase {
    private func start(_ initLua: String) throws -> HostPipe {
        let dir = try ExtFixture.make(name: "probe", initLua: initLua)
        let p = try HostPipe(dir: dir)
        p.send(["t": "hello", "api": 1, "name": "probe", "dir": dir.path, "storageDir": NSTemporaryDirectory(),
                "config": [:], "locale": "de", "sessions": []])
        return p
    }

    /// Nächste Nachricht vom Typ `t`, andere (ready, log) werden übersprungen.
    private func next(_ p: HostPipe, _ t: String, timeout: TimeInterval = 3) -> [String: Any]? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            guard let m = p.next(timeout: deadline.timeIntervalSinceNow) else { return nil }
            if m["t"] as? String == t { return m }
        }
        return nil
    }

    private func logs(_ p: HostPipe, count: Int) -> [String] {
        (0..<count).compactMap { _ in next(p, "log")?["text"] as? String }
    }

    // MARK: Rechnen

    func testLoadIsBusyShareOfNewTicks() {
        let old = [SystemProbe.Ticks(busy: 100, total: 1000), SystemProbe.Ticks(busy: 0, total: 0), SystemProbe.Ticks(busy: 50, total: 50)]
        let new = [SystemProbe.Ticks(busy: 150, total: 1100), SystemProbe.Ticks(busy: 0, total: 0), SystemProbe.Ticks(busy: 40, total: 60)]
        XCTAssertEqual(SystemProbe.load(from: old, to: new), [0.5, 0, 0], "ohne neue Ticks oder mit Rücksprung zählt 0")
        XCTAssertNil(SystemProbe.load(from: old, to: Array(new.prefix(2))), "andere Kernzahl: kein Vergleich")
        let fields = SystemProbe.cpuFields([0.5, 0.25])
        XCTAssertEqual(fields["total"] as? Double, 0.375)
    }

    func testRealTicksAndDisk() throws {
        let ticks = try XCTUnwrap(SystemProbe.ticks())
        XCTAssertEqual(ticks.count, ProcessInfo.processInfo.processorCount)
        let disk = try XCTUnwrap(SystemProbe.disk("/"))
        let total = try XCTUnwrap(disk["total"] as? Double), free = try XCTUnwrap(disk["free"] as? Double)
        XCTAssertGreaterThan(total, 0)
        XCTAssertLessThanOrEqual(free, total)
        XCTAssertNil(SystemProbe.disk("/gibt/es/nicht"))
    }

    func testRealMem() throws {
        let mem = try XCTUnwrap(SystemProbe.mem())
        let total = try XCTUnwrap(mem["total"] as? Double), used = try XCTUnwrap(mem["used"] as? Double)
        let free = try XCTUnwrap(mem["free"] as? Double), fraction = try XCTUnwrap(mem["fraction"] as? Double)
        XCTAssertGreaterThan(total, 0)
        XCTAssertLessThanOrEqual(used, total)
        XCTAssertEqual(used, total - free, accuracy: 1)
        XCTAssertEqual(fraction, used / total, accuracy: 1e-12)
        XCTAssertLessThanOrEqual(fraction, 1)
    }

    // MARK: Lua

    func testCpuAndDiskFromLua() throws {
        let p = try start("""
        kadrell.on("app.ready", function()
            local c = kadrell.system.cpu()
            kadrell.log("cores " .. #c.cores .. " total " .. tostring(c.total >= 0 and c.total <= 1))
            local d = kadrell.system.disk("/")
            kadrell.log("disk " .. tostring(d.total > 0 and d.free <= d.total and d.fraction >= 0 and d.fraction <= 1))
            local none, err = kadrell.system.disk("/gibt/es/nicht")
            kadrell.log("missing " .. tostring(none) .. " " .. tostring(err ~= nil))
        end)
        """)
        XCTAssertEqual(next(p, "ready")?["t"] as? String, "ready")
        p.send(["t": "event", "name": "app.ready", "data": [:]])
        XCTAssertEqual(logs(p, count: 3), ["cores \(ProcessInfo.processInfo.processorCount) total true", "disk true", "missing nil true"])
    }

    func testMemFromLua() throws {
        let p = try start("""
        kadrell.on("app.ready", function()
            local m = kadrell.system.mem()
            kadrell.log("mem " .. tostring(m.total > 0 and m.used <= m.total and m.fraction >= 0 and m.fraction <= 1))
        end)
        """)
        XCTAssertEqual(next(p, "ready")?["t"] as? String, "ready")
        p.send(["t": "event", "name": "app.ready", "data": [:]])
        XCTAssertEqual(logs(p, count: 1), ["mem true"])
    }

    func testSystemOutsideHandlerFails() throws {
        let p = try start("local ok, err = pcall(kadrell.system.cpu)\nkadrell.log(tostring(ok) .. ' ' .. tostring(err):match('only allowed inside handlers') )")
        XCTAssertEqual(next(p, "log")?["text"] as? String, "false only allowed inside handlers")
    }

    func testFooterSetSendsOnlyTheLastList() throws {
        let p = try start("""
        kadrell.footer.set({ { id = "a", text = "1" } })
        kadrell.footer.set({ { id = "a", text = "2", bars = { 0.5, 1 } } })
        """)
        let m = try XCTUnwrap(next(p, "footer"))
        let items = try XCTUnwrap(m["items"] as? [[String: Any]])
        XCTAssertEqual(items.first?["text"] as? String, "2")
        XCTAssertEqual(items.first?["bars"] as? [Double], [0.5, 1])
        XCTAssertNil(next(p, "footer", timeout: 0.3), "nur ein Stand pro Dispatch")
    }

    func testFooterClearSendsNull() throws {
        let p = try start("kadrell.footer.clear()")
        let m = try XCTUnwrap(next(p, "footer"))
        XCTAssertTrue(m["items"] is NSNull)
    }

    func testLocaleEventUpdatesBeforeHandlers() throws {
        let p = try start(#"kadrell.on("app.locale", function(d) kadrell.log(kadrell.locale .. "/" .. d.locale) end)"#)
        XCTAssertNotNil(next(p, "ready"))
        p.send(["t": "event", "name": "app.locale", "data": ["locale": "en"]])
        XCTAssertEqual(next(p, "log")?["text"] as? String, "en/en")
    }

    // MARK: Manifest und Ordner

    func testChoiceAndNumberSettingsDecode() throws {
        let data = Data("""
        {"name":"x","apiVersion":1,"settings":[
          {"key":"cpu","type":"choice","label":"CPU","default":"compact","options":[{"value":"compact","label":"Kompakt"},"detailed"]},
          {"key":"every","type":"number","label":"Sekunden","default":5}]}
        """.utf8)
        let m = try JSONDecoder().decode(ExtensionManifest.self, from: data)
        XCTAssertEqual(m.settings[0].options, [.init(value: "compact", label: "Kompakt"), .init(value: "detailed", label: "detailed")])
        XCTAssertEqual(m.settings[1].default, .number(5))
        XCTAssertNil(m.settings[1].options)
    }

    func testNumberParsing() {
        XCTAssertEqual(ExtensionSettings.number(.string(" 2,5 ")), 2.5)
        XCTAssertEqual(ExtensionSettings.number(.string("60")), 60)
        XCTAssertEqual(ExtensionSettings.number(.number(3)), 3)
        XCTAssertNil(ExtensionSettings.number(.string("")))
        XCTAssertNil(ExtensionSettings.number(.string("viel")))
        XCTAssertNil(ExtensionSettings.number(.string("inf")))
        XCTAssertEqual(ExtensionDetail.numberText(.number(5)), "5")
        XCTAssertEqual(ExtensionDetail.numberText(.number(2.5)), "2.5")
    }

    func testCustomExtensionsDir() {
        XCTAssertNil(ExtensionCatalog.customDir(arguments: ["Kadrell", "--profile", "tmp"], environment: [:]))
        XCTAssertEqual(ExtensionCatalog.customDir(arguments: ["Kadrell", "--extensions-dir", "/tmp/ext/"], environment: [:])?.path, "/tmp/ext")
        XCTAssertEqual(ExtensionCatalog.customDir(arguments: [], environment: ["KADRELL_EXTENSIONS_DIR": "~/x"])?.path, NSHomeDirectory() + "/x")
        XCTAssertEqual(ExtensionCatalog.customDir(arguments: ["--extensions-dir", "/a"], environment: ["KADRELL_EXTENSIONS_DIR": "/b"])?.path, "/a",
                       "das Argument geht vor")
        XCTAssertNil(ExtensionCatalog.customDir(arguments: [], environment: ["KADRELL_EXTENSIONS_DIR": "  "]))
    }
}
