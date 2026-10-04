import XCTest
@testable import Kadrell

/// `Kadrell ext-host`: Lua-Helper mit Prelude, Protokoll über JSON-Zeilen.
final class ExtHostTests: XCTestCase {
    private func start(_ initLua: String, storageDir: String = NSTemporaryDirectory()) throws -> HostPipe {
        let dir = try ExtFixture.make(name: "hello", initLua: initLua)
        let p = try HostPipe(dir: dir)
        p.send(["t": "hello", "api": 1, "name": "hello", "dir": dir.path, "storageDir": storageDir,
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

    /// Startet die Extension, wartet auf `ready` und schickt `app.ready`.
    private func startReady(_ initLua: String, storageDir: String = NSTemporaryDirectory()) throws -> HostPipe {
        let p = try start(initLua, storageDir: storageDir)
        XCTAssertEqual(p.next(timeout: 3)?["t"] as? String, "ready")
        p.send(["t": "event", "name": "app.ready", "data": [:]])
        return p
    }

    private func assertAlive(_ p: HostPipe, id: Int = 9, file: StaticString = #filePath, line: UInt = #line) {
        p.send(["t": "ping", "id": id])
        let pong = p.next(timeout: 3)
        XCTAssertEqual(pong?["t"] as? String, "pong", file: file, line: line)
        XCTAssertEqual(pong?["id"] as? Int, id, file: file, line: line)
    }

    func testExecInsideHandler() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function()
                local r = kadrell.exec({"/bin/echo", "hi"})
                kadrell.log(r.status .. ":" .. r.stdout)
            end)
            """)
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "0:hi\n")
    }

    /// Das Kind erbt den Protokollkanal nicht: ein Schreibversuch auf die niedrigen fds darf nichts im Protokoll auslösen.
    func testExecChildCannotWriteIntoProtocol() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function()
                kadrell.exec({"/bin/sh", "-c", "for fd in 3 4 5 6 7 8 9 10 11 12; do echo junk >&$fd; done 2>/dev/null; true"})
                kadrell.log("danach")
            end)
            """)
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "danach")
        assertAlive(p)
    }

    func testExecMissingBinary() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function()
                kadrell.log(kadrell.exec({"/nope"}).status)
            end)
            """)
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "127")
        assertAlive(p)
    }

    func testExecReturnsStderrSeparately() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function()
                local r = kadrell.exec({"/bin/sh", "-c", "echo e >&2; exit 3"})
                kadrell.log(r.status .. "|" .. r.stdout .. "|" .. r.stderr)
            end)
            """)
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "3||e\n")
    }

    func testExecResolvesViaPath() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function() kadrell.log(kadrell.exec({"echo", "x"}).stdout) end)
            """)
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "x\n")
    }

    func testHandlerErrorIsLoggedNotFatal() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function() error("boom") end)
            """)
        let log = p.next(timeout: 3)
        XCTAssertEqual(log?["level"] as? String, "error")
        XCTAssertTrue((log?["text"] as? String ?? "").contains("boom"))
        assertAlive(p)
    }

    func testCallOutsideHandlerFails() throws {
        let p = try start("kadrell.exec({\"/bin/echo\"})")
        let log = p.next(timeout: 3)
        XCTAssertEqual(log?["level"] as? String, "error")
        XCTAssertTrue((log?["text"] as? String ?? "").contains("nur in Handlern"), "\(String(describing: log))")
        XCTAssertTrue(p.waitForExit(timeout: 3))
        XCTAssertNotEqual(p.process.terminationStatus, 0)
    }

    func testTimerFires() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function() kadrell.after(0.1, function() kadrell.log("t") end) end)
            """)
        XCTAssertEqual(p.next(timeout: 1)?["text"] as? String, "t")
    }

    func testEveryRepeatsAndCancels() throws {
        let p = try startReady("""
            local n = 0
            kadrell.on("app.ready", function()
                local h
                h = kadrell.every(0.05, function()
                    n = n + 1
                    kadrell.log("e" .. n)
                    if n == 2 then h:cancel() end
                end)
            end)
            kadrell.on("check", function() kadrell.log("n=" .. n) end)
            """)
        XCTAssertEqual(p.next(timeout: 1)?["text"] as? String, "e1")
        XCTAssertEqual(p.next(timeout: 1)?["text"] as? String, "e2")
        usleep(300_000)
        p.send(["t": "event", "name": "check", "data": [:]])
        XCTAssertEqual(p.next(timeout: 1)?["text"] as? String, "n=2")
    }

    func testRunRoundtrip() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function()
                local r = kadrell.run("ls", "--json")
                kadrell.log(r.status .. r.stdout)
            end)
            """)
        let run = p.next(timeout: 3)
        XCTAssertEqual(run?["t"] as? String, "run")
        XCTAssertEqual(run?["argv"] as? [String], ["ls", "--json"])
        p.send(["t": "result", "id": try XCTUnwrap(run?["id"] as? Int), "status": 0, "stdout": "{}", "stderr": ""])
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "0{}")
    }

    /// Zahlen im argv gehen als Strings raus, sonst verwürfe Kadrell die Zeile und die Coroutine wartete ewig.
    func testRunConvertsArgumentsToStrings() throws {
        let p = try startReady(#"kadrell.on("app.ready", function() kadrell.run("select", "-t", 1) end)"#)
        XCTAssertEqual(p.next(timeout: 3)?["argv"] as? [String], ["select", "-t", "1"])
    }

    func testExecConvertsArgumentsToStrings() throws {
        let p = try startReady(#"kadrell.on("app.ready", function() kadrell.log(kadrell.exec({"/bin/echo", 1, true}).stdout) end)"#)
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "1 true\n")
    }

    func testSessionsParsesLsJson() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function() kadrell.log(kadrell.sessions()[1].key) end)
            """)
        let run = p.next(timeout: 3)
        XCTAssertEqual(run?["argv"] as? [String], ["ls", "--json"])
        p.send(["t": "result", "id": try XCTUnwrap(run?["id"] as? Int), "status": 0, "stdout": #"[{"key":"a1"}]"#, "stderr": ""])
        XCTAssertEqual(p.next(timeout: 3)?["text"] as? String, "a1")
    }

    func testHttpRoundtrip() throws {
        let server = try OneShotHTTPServer(response: "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nX-A: b\r\nConnection: close\r\n\r\nhi")
        let p = try startReady("""
            kadrell.on("app.ready", function()
                local r = kadrell.http{url = "http://127.0.0.1:\(server.port)/x", method = "POST", body = "q", headers = {["X-T"] = "1"}}
                kadrell.log(r.status .. ":" .. r.body .. ":" .. r.headers["X-A"])
            end)
            """)
        XCTAssertEqual(p.next(timeout: 5)?["text"] as? String, "200:hi:b")
        let request = server.request()
        XCTAssertTrue(request.hasPrefix("POST /x"), request)
        XCTAssertTrue(request.lowercased().contains("x-t: 1"), request)
    }

    func testStorageSurvivesRestart() throws {
        let storage = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-st-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: storage, withIntermediateDirectories: true)
        let first = try startReady(#"kadrell.on("app.ready", function() kadrell.storage.set("k", {a = 1}); kadrell.log("ok") end)"#, storageDir: storage.path)
        XCTAssertEqual(first.next(timeout: 3)?["text"] as? String, "ok")
        first.closeStdin()
        XCTAssertTrue(first.waitForExit(timeout: 2))
        let second = try startReady(#"kadrell.on("app.ready", function() kadrell.log(kadrell.storage.get("k").a) end)"#, storageDir: storage.path)
        XCTAssertEqual(second.next(timeout: 3)?["text"] as? String, "1")
    }

    func testPanelAndStatusAreForwarded() throws {
        let p = try startReady("""
            kadrell.on("app.ready", function()
                kadrell.panel.set{title = "T"}
                kadrell.status.clear()
                kadrell.panel.clear()
            end)
            """)
        let panel = p.next(timeout: 3)
        XCTAssertEqual(panel?["t"] as? String, "panel")
        XCTAssertEqual((panel?["tree"] as? [String: Any])?["title"] as? String, "T")
        let status = p.next(timeout: 3)
        XCTAssertEqual(status?["t"] as? String, "status")
        XCTAssertTrue(status?["item"] is NSNull, "\(String(describing: status))")
        XCTAssertTrue(p.next(timeout: 3)?["tree"] is NSNull)
    }
}

/// Beantwortet genau eine HTTP-Anfrage auf 127.0.0.1 (freier Port) mit dem vorgegebenen Text.
final class OneShotHTTPServer: @unchecked Sendable {
    let port: UInt16
    private let lock = NSLock()
    private var received = ""

    init(response: String) throws {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        let bound = withUnsafeMutablePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, len) == 0 && getsockname(fd, $0, &len) == 0 && listen(fd, 1) == 0 }
        }
        guard bound else { throw NSError(domain: "OneShotHTTPServer", code: Int(errno)) }
        port = UInt16(bigEndian: addr.sin_port)
        Thread {
            let conn = accept(fd, nil, nil)
            var buf = [UInt8](repeating: 0, count: 8192)
            let n = read(conn, &buf, buf.count)
            self.lock.withLock { self.received = String(decoding: buf.prefix(max(n, 0)), as: UTF8.self) }
            _ = response.withCString { write(conn, $0, strlen($0)) }
            close(conn)
            close(fd)
        }.start()
    }

    func request() -> String {
        for _ in 0..<100 {
            if let r = lock.withLock({ received.isEmpty ? nil : received }) { return r }
            usleep(20_000)
        }
        return ""
    }
}
