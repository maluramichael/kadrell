import XCTest
@testable import Kadrell

/// `Kadrell ext-host`: Lua-Helper mit Prelude, Protokoll über JSON-Zeilen.
final class ExtHostTests: XCTestCase {
    private func start(_ initLua: String) throws -> HostPipe {
        let dir = try ExtFixture.make(name: "hello", initLua: initLua)
        let p = try HostPipe(dir: dir)
        p.send(["t": "hello", "api": 1, "name": "hello", "dir": dir.path, "storageDir": NSTemporaryDirectory(),
                "config": [:], "locale": "de", "sessions": []])
        return p
    }

    func testHelloReadyPingPong() throws {
        let p = try start("kadrell.log('geladen')")
        let log = p.next(timeout: 3)
        XCTAssertEqual(log?["t"] as? String, "log")
        XCTAssertEqual(log?["text"] as? String, "geladen")
        XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "ready")
        p.send(["t": "ping", "id": 7])
        let pong = p.next(timeout: 3)
        XCTAssertEqual(pong?["t"] as? String, "pong")
        XCTAssertEqual(pong?["id"] as? Int, 7)
    }

    func testPrintGoesToLog() throws {
        let p = try start("print('x', 1)")
        let log = p.next(timeout: 3)
        XCTAssertEqual(log?["t"] as? String, "log")
        XCTAssertEqual(log?["level"] as? String, "info")
        XCTAssertEqual(log?["text"] as? String, "x\t1")
        XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "ready")
    }

    func testLoadErrorIsReportedThenExit() throws {
        let p = try start("error('kaputt')")
        let log = p.next(timeout: 3)
        XCTAssertEqual(log?["t"] as? String, "log")
        XCTAssertEqual(log?["level"] as? String, "error")
        let text = log?["text"] as? String ?? ""
        XCTAssertTrue(text.contains("init.lua:1"), text)
        XCTAssertTrue(text.contains("kaputt"), text)
        XCTAssertTrue(p.waitForExit(timeout: 3))
        XCTAssertNotEqual(p.process.terminationStatus, 0)
    }

    func testBlockedFunctionsAreGone() throws {
        let p = try start("kadrell.log(tostring(os.execute)..tostring(io.popen)..tostring(package.loadlib))")
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "nilnilnil")
    }

    func testJsonRoundtrip() throws {
        let p = try start(#"kadrell.log(kadrell.json.encode(kadrell.json.decode('{"a":[1,2],"b":"ü"}')))"#)
        let text = p.next(timeout: 3)?["text"] as? String ?? ""
        XCTAssertTrue(text.contains(#""a":[1,2]"#), text)
        XCTAssertTrue(text.contains("ü"), text)
    }

    /// Tiefe C-Rekursion (verschachtelte pcall) ist ein Lua-Fehler, kein Absturz des Helpers: der Fehler wird
    /// durch alle Ebenen weitergereicht, init.lua lädt nicht, der Helper endet regulär mit Status 1.
    func testDeepRecursionIsALuaError() throws {
        let p = try start("local function f() local ok, err = pcall(f); error(err, 0) end\nf()")
        let log = p.next(timeout: 3)
        XCTAssertEqual(log?["level"] as? String, "error")
        let text = log?["text"] as? String ?? ""
        XCTAssertTrue(text.contains("stack overflow"), text)
        XCTAssertTrue(p.waitForExit(timeout: 3))
        XCTAssertEqual(p.process.terminationReason, .exit)
        XCTAssertEqual(p.process.terminationStatus, 1)
    }

    /// Rekursives __index in einem Event-Handler: Fehler ins Log, die Extension läuft weiter.
    func testRecursiveIndexInHandlerIsLogged() throws {
        let p = try start("""
            local t = setmetatable({}, { __index = function(t, k) return t[k] end })
            kadrell.on("app.ready", function() return t.x end)
            """)
        XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "ready")
        p.send(["t": "event", "name": "app.ready", "data": [:]])
        let log = p.next(timeout: 3)
        XCTAssertEqual(log?["level"] as? String, "error")
        let text = log?["text"] as? String ?? ""
        XCTAssertTrue(text.contains("stack overflow"), text)
        p.send(["t": "ping", "id": 3])
        let pong = p.next(timeout: 3)
        XCTAssertEqual(pong?["t"] as? String, "pong")
        XCTAssertEqual(pong?["id"] as? Int, 3)
    }

    func testStdinEofEndsHelper() throws {
        let p = try start("")
        XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "ready")
        p.closeStdin()
        XCTAssertTrue(p.waitForExit(timeout: 1))
    }
}
