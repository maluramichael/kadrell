import Foundation

/// `Kadrell ext-host <ordner>`: der Helper-Prozess einer Extension. Eine Lua-Instanz auf dem Main-Thread,
/// Nachrichten als JSON-Zeilen über stdin/stdout. Lua sieht nur die C-Schicht (`LuaShim`), nie Swift-Frames.
enum ExtHost {
    /// Protokollkanal: eine Kopie des ursprünglichen stdout. fd 1 zeigt danach auf stderr, damit `io.write` oder
    /// ein anderer Schreiber auf stdout das Protokoll nicht zerschießt, sondern im Log der Extension landet.
    nonisolated(unsafe) private static var out: Int32 = -1
    /// Der Lua-Zustand; nur auf dem Main-Thread benutzt.
    nonisolated(unsafe) private static var lua: OpaquePointer?
    private static let outLock = NSLock()

    static func run(dir: String) -> Never {
        // close-on-exec: Prozesse aus kadrell.exec dürfen den Protokollkanal nicht erben.
        out = fcntl(STDOUT_FILENO, F_DUPFD_CLOEXEC, 0)
        dup2(STDERR_FILENO, STDOUT_FILENO)
        // Jedes Ende über exit() (EOF auf stdin, shutdown, os.exit, Ladefehler) nimmt laufende exec-Kinder mit.
        atexit { ProcessRunner.killChildren(of: getpid()) }
        guard let state = kl_new(64 << 20) else { fail("Lua startet nicht") }
        let L = state
        lua = L
        kl_set_sender { json, len in
            guard let json else { return }
            ExtHost.route(json, len)
        }
        guard let prelude = Bundle.main.path(forResource: "prelude", ofType: "lua") else { fail("prelude.lua fehlt") }
        if let err = call({ kl_run_file(L, prelude, $0, $1) }) { fail(err) }

        // Lua läuft auf dem Main-Thread: nur der hat 8 MB Stack. Lua begrenzt C-Rekursion auf 200 Ebenen und rechnet
        // mit normalem Stack, auf den 512 KB eines GCD-Workers endet tiefe Rekursion in SIGBUS statt in „C stack overflow“.
        // Eigener Lese-Thread: bei EOF endet der Prozess hier, auch wenn Lua gerade in einer Endlosschleife hängt.
        Thread {
            while let line = readLine() {
                DispatchQueue.main.async { dispatch(line) }
            }
            exit(0)
        }.start()
        // Nicht dispatchMain(): das beendet den Main-Thread, die Main-Queue liefe dann wieder auf einem GCD-Worker.
        CFRunLoopRun()
        exit(0)
    }

    /// Nur auf dem Main-Thread: Lua bekommt eine Zeile von Kadrell oder ein Ergebnis aus dem Helper.
    private static func dispatch(_ line: String) {
        guard let L = lua else { return }
        if let err = call({ kl_dispatch(L, line, $0, $1) }) { FileHandle.standardError.write(Data((err + "\n").utf8)) }
    }

    /// Ergebnis einer Hintergrundarbeit zurück in Lua, immer über die Main-Queue.
    private static func deliver(id: Int, _ fields: [String: Any]) {
        let msg = fields.merging(["t": "result", "id": id]) { $1 }
        guard let data = try? JSONSerialization.data(withJSONObject: msg) else { return }
        let line = String(decoding: data, as: UTF8.self)
        DispatchQueue.main.async { dispatch(line) }
    }

    /// Lua-Ausgang: exec, http und timer erledigt der Helper selbst, alles andere geht an Kadrell.
    private static func route(_ json: UnsafePointer<CChar>, _ len: Int) {
        let data = Data(bytes: json, count: len)
        guard let msg = try? JSONSerialization.jsonObject(with: data) as? [String: Any], let id = msg["id"] as? Int else {
            return writeLine(json, len)
        }
        switch msg["t"] as? String {
        case "exec": exec(id: id, msg)
        case "http": http(id: id, msg)
        case "timer": timer(id: id, after: (msg["after"] as? Double) ?? 0)
        default: writeLine(json, len)
        }
    }

    private static func timer(id: Int, after: Double) {
        let line = #"{"t":"timer","id":\#(id)}"#
        DispatchQueue.main.asyncAfter(deadline: .now() + max(after, 0)) { dispatch(line) }
    }

    /// Ohne „/“ im Namen sucht `env` über PATH, sonst bräuchte Process einen absoluten Pfad. Startfehler heißen wie in der Shell 127.
    private static func exec(id: Int, _ msg: [String: Any]) {
        let argv = msg["argv"] as? [String] ?? []
        let cwd = msg["cwd"] as? String, timeout = (msg["timeout"] as? Double) ?? 30
        Task.detached {
            var fields: [String: Any]
            do {
                guard let first = argv.first else { throw CocoaError(.fileNoSuchFile) }
                let viaEnv = !first.contains("/")
                let r = try await ProcessRunner.runCapturingStderr(viaEnv ? "/usr/bin/env" : first, viaEnv ? argv : Array(argv.dropFirst()),
                                                    environment: ProcessInfo.processInfo.environment, cwd: cwd,
                                                    timeout: timeout)
                fields = ["status": Int(r.status), "stdout": r.output, "stderr": r.stderr]
            } catch {
                fields = ["status": 127, "stdout": "", "stderr": error.localizedDescription]
            }
            deliver(id: id, fields)
        }
    }

    private static func http(id: Int, _ msg: [String: Any]) {
        // Nur http/https: file:// und andere Schemata würden lokale Dateien oder Dienste erreichbar machen (SSRF).
        guard let url = (msg["url"] as? String).flatMap(URL.init(string:)),
              let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else {
            return deliver(id: id, ["status": 0, "headers": [:] as [String: String], "body": "", "error": "URL muss http oder https sein"])
        }
        var req = URLRequest(url: url)
        req.httpMethod = msg["method"] as? String
        req.timeoutInterval = (msg["timeout"] as? Double) ?? 30
        req.httpBody = (msg["body"] as? String).map { Data($0.utf8) }
        for (k, v) in msg["headers"] as? [String: String] ?? [:] { req.setValue(v, forHTTPHeaderField: k) }
        let request = req
        Task.detached {
            do {
                let (data, resp) = try await URLSession.shared.data(for: request)
                let http = resp as? HTTPURLResponse
                let headers = (http?.allHeaderFields ?? [:]).reduce(into: [String: String]()) { $0["\($1.key)"] = "\($1.value)" }
                deliver(id: id, ["status": http?.statusCode ?? 0, "headers": headers, "body": String(decoding: data, as: UTF8.self)])
            } catch {
                deliver(id: id, ["status": 0, "headers": [:] as [String: String], "body": "", "error": error.localizedDescription])
            }
        }
    }

    /// Ruft die C-Schicht mit einem Fehlerpuffer auf, liefert den Fehlertext oder nil.
    private static func call(_ fn: (UnsafeMutablePointer<CChar>, Int) -> Int32) -> String? {
        var err = [CChar](repeating: 0, count: 8192)
        let rc = err.withUnsafeMutableBufferPointer { fn($0.baseAddress!, $0.count) }
        guard rc != 0 else { return nil }
        return String(decoding: err.prefix { $0 != 0 }.map(UInt8.init(bitPattern:)), as: UTF8.self)
    }

    private static func writeLine(_ json: UnsafePointer<CChar>, _ len: Int) {
        var line = Data(bytes: json, count: len)
        line.append(0x0A)
        outLock.withLock {
            line.withUnsafeBytes { buf in
                var done = 0
                while done < buf.count {
                    let n = write(out, buf.baseAddress! + done, buf.count - done)
                    if n < 0 && errno == EINTR { continue }
                    if n <= 0 { return }
                    done += n
                }
            }
        }
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data(("ext-host: " + message + "\n").utf8))
        exit(1)
    }
}
