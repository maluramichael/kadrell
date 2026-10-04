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

    func testStdinEofEndsHelper() throws {
        let p = try start("")
        XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "ready")
        p.closeStdin()
        XCTAssertTrue(p.waitForExit(timeout: 1))
    }
}
