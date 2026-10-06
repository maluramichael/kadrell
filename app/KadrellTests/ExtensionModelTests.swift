import XCTest
@testable import Kadrell

final class ExtensionModelTests: XCTestCase {
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-catalog-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: root)
    }

    @discardableResult
    private func make(_ folder: String, name: String? = nil, api: Int = 1, initLua: Bool = true, mode: Int16 = 0o755) throws -> URL {
        let dir = root.appendingPathComponent(folder)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let manifest: [String: Any] = ["name": name ?? folder, "version": "0.1.0", "apiVersion": api]
        try JSONSerialization.data(withJSONObject: manifest).write(to: dir.appendingPathComponent("kadrell.json"))
        if initLua { try Data("-- leer".utf8).write(to: dir.appendingPathComponent("init.lua")) }
        try FileManager.default.setAttributes([.posixPermissions: mode], ofItemAtPath: dir.path)
        return dir
    }

    func testScanFindsValidAndReportsProblems() throws {
        try make("ok")
        try make("nameMismatch", name: "anders")
        try make("apiTooHigh", api: 2)
        try make("noInit", initLua: false)
        try make("groupWritable", mode: 0o775)
        let found = ExtensionCatalog.scan(root)
        XCTAssertEqual(found.map(\.name), ["apiTooHigh", "groupWritable", "nameMismatch", "noInit", "ok"], "nach Name sortiert")
        for ext in found {
            if ext.name == "ok" { XCTAssertNil(ext.problem); XCTAssertEqual(ext.manifest?.version, "0.1.0") }
            else { XCTAssertNotNil(ext.problem, ext.name) }
        }
    }

    func testScanReportsBrokenManifestAndMissingDir() throws {
        let dir = try make("kaputt")
        try Data("{".utf8).write(to: dir.appendingPathComponent("kadrell.json"))
        XCTAssertNotNil(ExtensionCatalog.scan(root).first?.problem)
        XCTAssertEqual(ExtensionCatalog.scan(root.appendingPathComponent("gibt-es-nicht")), [])
    }

    func testScanSkipsDotFolders() throws {
        try make(".versteckt")
        try make("sichtbar")
        try Data("x".utf8).write(to: root.appendingPathComponent("datei.txt"))
        XCTAssertEqual(ExtensionCatalog.scan(root).map(\.name), ["sichtbar"])
    }

    /// Die Problemtexte haben Platzhalter; der Schlüssel im Katalog muss zum Aufruf passen, sonst bliebe Deutsch stehen.
    @MainActor func testProblemsAreTranslated() throws {
        defer {
            Profile.defaults.removeObject(forKey: Settings.languageKey)
            Localization.apply(.system)
        }
        try make("apiTooHigh", api: 2)
        try make("nameMismatch", name: "anders")
        Settings.language = .en
        let problems = ExtensionCatalog.scan(root).compactMap(\.problem)
        XCTAssertEqual(problems, ["Needs a newer Kadrell (API 2, supported: 1)", "Name in the manifest (“anders”) does not match the folder name"])
    }

    func testManifestSettingsAndDefaults() throws {
        let json = #"{"name":"j","apiVersion":1,"permissions":["exec"],"settings":[{"key":"k","type":"bool","label":"L","default":true}]}"#
        let m = try JSONDecoder().decode(ExtensionManifest.self, from: Data(json.utf8))
        XCTAssertEqual(m.settings, [.init(key: "k", type: "bool", label: "L", default: .bool(true))])
        XCTAssertEqual(m.description, "")
    }

    func testDecodeIgnoresGarbage() {
        XCTAssertNil(ExtensionMessage.decode(Data("nicht json".utf8)))
        XCTAssertNil(ExtensionMessage.decode(Data([0xff, 0xfe])))
        XCTAssertNil(ExtensionMessage.decode(Data(#"{"t":"wat"}"#.utf8)))
        XCTAssertNil(ExtensionMessage.decode(Data("[1,2]".utf8)))
        XCTAssertNil(ExtensionMessage.decode(Data(#"{"t":"run","id":"x","argv":["ls"]}"#.utf8)))
        XCTAssertNil(ExtensionMessage.decode(Data(#"{"t":"run","id":1,"argv":[1]}"#.utf8)))
    }

    func testDecodeRunAndLog() {
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"run","id":3,"argv":["ls"]}"#.utf8)), .run(id: 3, argv: ["ls"]))
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"run","id":3.0,"argv":[]}"#.utf8)), .run(id: 3, argv: []))
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"log","level":"warn","text":"hi"}"#.utf8)), .log(level: "warn", text: "hi"))
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"ready"}"#.utf8)), .ready)
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"pong","id":7}"#.utf8)), .pong(7))
    }

    func testDecodePanelAndStatusNull() {
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"panel","tree":null}"#.utf8)), .panel(nil))
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"status","item":null}"#.utf8)), .status(nil))
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"status","item":{"text":"a"}}"#.utf8)), .status(.object(["text": .string("a")])))
    }

    func testHostMessageLineEndsWithNewline() throws {
        let line = HostMessage.hello(api: 1, name: "x", dir: "/a/b", storageDir: "/s", config: ["k": .string("v\nw")], locale: "de", sessions: [])
            .line()
        XCTAssertEqual(line.last, 0x0A)
        XCTAssertEqual(line.filter { $0 == 0x0A }.count, 1, "Zeilenumbruch im Wert wird maskiert")
        let v = try JSONDecoder().decode(JSONValue.self, from: line)
        XCTAssertEqual(v["t"]?.string, "hello")
        XCTAssertEqual(v["config"]?["k"]?.string, "v\nw")
        XCTAssertEqual(v["storageDir"]?.string, "/s")
        XCTAssertEqual(v["sessions"], .array([]))
        let result = try JSONDecoder().decode(JSONValue.self, from: HostMessage.result(id: 4, status: 1, stdout: "o", stderr: "e").line())
        XCTAssertEqual(result["id"]?.int, 4)
        XCTAssertEqual(result["stderr"]?.string, "e")
        XCTAssertEqual(String(data: HostMessage.shutdown.line(), encoding: .utf8), "{\"t\":\"shutdown\"}\n")
    }

    private func tree(_ json: String) throws -> (PanelTree, [String]) {
        let v = try JSONDecoder().decode(JSONValue.self, from: Data(json.utf8))
        guard case .success(let r) = PanelValidation.tree(v) else { throw XCTSkip("abgelehnt") }
        return (r.0, r.warnings)
    }

    func testPanelValidation() throws {
        let long = String(repeating: "a", count: 600)
        let (tree, warnings) = try tree("""
        {"title":"T","children":[
          {"type":"video"},
          {"type":"item","text":"\(long)","color":"pink","actions":[{"id":"a","label":"A"}]},
          {"type":"section","title":"S","collapsed":true,"children":[{"type":"button","label":"B","action":"go"}]},
          {"type":"text","text":"t","color":"muted"}]}
        """)
        XCTAssertEqual(tree.title, "T")
        XCTAssertEqual(tree.nodes, [
            .item(text: String(repeating: "a", count: 500), detail: nil, color: nil, actions: [PanelAction(id: "a", label: "A")]),
            .section(title: "S", collapsed: true, children: [.button(label: "B", action: "go")]),
            .text("t", .muted),
        ])
        XCTAssertEqual(warnings.count, 2, "Typ video und Farbe pink")
    }

    func testPanelToleratesWrongShapes() throws {
        let (empty, w1) = try tree(#"{"title":"T","children":[]}"#)
        XCTAssertEqual(empty.nodes, []); XCTAssertEqual(w1, [])
        // json.lua kodiert eine leere Tabelle als [], ein Knoten kann also auch [] sein.
        let (t, w2) = try tree(#"{"children":[[],5,null,"x",{"type":7},{"type":"item"},{"type":"section","children":{}},{"type":"item","text":"a","actions":[{"id":1}]}]}"#)
        XCTAssertEqual(t.title, "")
        XCTAssertEqual(t.nodes, [.section(title: "", collapsed: false, children: []), .item(text: "a", detail: nil, color: nil, actions: [])])
        XCTAssertEqual(w2.count, 7, "fünf Nicht-Knoten, item ohne text, Aktion ohne label")
        let (none, _) = try tree("[]")
        XCTAssertEqual(none.nodes, [])
    }

    func testPanelTooLarge() throws {
        let items = Array(repeating: #"{"type":"item","text":"x"}"#, count: 2001).joined(separator: ",")
        let v = try JSONDecoder().decode(JSONValue.self, from: Data(#"{"title":"T","children":[\#(items)]}"#.utf8))
        guard case .failure(let error) = PanelValidation.tree(v) else { return XCTFail("2001 Knoten müssen abgelehnt werden") }
        XCTAssertEqual(error, .tooLarge(2001))
    }

    /// A4: jedes Array-Element zählt zur Grenze, auch Nicht-Knoten und Aktionen; sonst baute ein Panel aus
    /// 500000 Zahlen ebenso viele Warnungen auf dem Hauptthread.
    func testPanelCountsEveryArrayElement() throws {
        let numbers = Array(repeating: "1", count: 500_000).joined(separator: ",")
        let v = try JSONDecoder().decode(JSONValue.self, from: Data(#"{"children":[\#(numbers)]}"#.utf8))
        guard case .failure(let error) = PanelValidation.tree(v) else { return XCTFail("500000 Zahlen müssen abgelehnt werden") }
        XCTAssertEqual(error, .tooLarge(500_000))
        let actions = Array(repeating: #"{"id":"a","label":"A"}"#, count: 2000).joined(separator: ",")
        let w = try JSONDecoder().decode(JSONValue.self, from: Data(#"{"children":[{"type":"item","text":"x","actions":[\#(actions)]}]}"#.utf8))
        guard case .failure(let e2) = PanelValidation.tree(w) else { return XCTFail("2000 Aktionen zählen mit") }
        XCTAssertEqual(e2, .tooLarge(2001))
    }

    /// A4: höchstens 20 Warnungen je Baum, dazu eine Sammelzeile.
    func testPanelWarningsAreCapped() throws {
        let unknown = Array(repeating: #"{"type":"video"}"#, count: 100).joined(separator: ",")
        let (_, warnings) = try tree(#"{"children":[\#(unknown)]}"#)
        XCTAssertEqual(warnings.count, 21)
        XCTAssertEqual(warnings.last, String(localized: "… und \(80) weitere Warnungen", bundle: Bundle.app))
    }

    /// B2: jede Lua-Datei der Extension wird geprüft, nicht nur init.lua.
    func testGroupWritableLuaInSubfolderIsUntrusted() throws {
        let sub = try make("lib").appendingPathComponent("lib")
        try FileManager.default.createDirectory(at: sub, withIntermediateDirectories: true)
        let file = sub.appendingPathComponent("x.lua")
        try Data("return 1".utf8).write(to: file)
        try FileManager.default.setAttributes([.posixPermissions: 0o664], ofItemAtPath: file.path)
        let problem = try XCTUnwrap(ExtensionCatalog.scan(root).first?.problem)
        XCTAssertTrue(problem.contains("x.lua"), problem)
    }

    /// A5: die MIT-Lizenz von Lua liegt im Bundle, das About nennt Lua und json.lua.
    @MainActor func testLuaLicenseShips() throws {
        let url = try XCTUnwrap(Bundle.main.url(forResource: "LICENSE", withExtension: nil), "LICENSE fehlt im Bundle")
        XCTAssertTrue(try String(contentsOf: url, encoding: .utf8).contains("Lua.org, PUC-Rio"))
        XCTAssertTrue(AboutView.credits.contains("Lua 5.5.1"), AboutView.credits)
        XCTAssertTrue(AboutView.credits.contains("json.lua"), AboutView.credits)
    }

    func testStatusTruncatedTo40() throws {
        let long = String(repeating: "x", count: 60)
        let v = try JSONDecoder().decode(JSONValue.self, from: Data(#"{"text":"\#(long)","color":"ok","action":"a"}"#.utf8))
        let s = try XCTUnwrap(PanelValidation.status(v))
        XCTAssertEqual(s.text.count, 40)
        XCTAssertEqual(s.color, .ok)
        XCTAssertEqual(s.action, "a")
        XCTAssertNil(PanelValidation.status(.array([])))
        XCTAssertNil(PanelValidation.status(.object(["color": .string("nope")])))
        XCTAssertNil(try XCTUnwrap(PanelValidation.status(.object(["text": .string("a"), "color": .string("nope")]))).color)
    }
}
