import Foundation

/// `Kadrell ext-host <ordner>`: der Helper-Prozess einer Extension. Eine Lua-Instanz auf der seriellen Queue `lua`,
/// Nachrichten als JSON-Zeilen über stdin/stdout. Lua sieht nur die C-Schicht (`LuaShim`), nie Swift-Frames.
enum ExtHost {
    /// Protokollkanal: eine Kopie des ursprünglichen stdout. fd 1 zeigt danach auf stderr, damit `io.write` oder
    /// ein anderer Schreiber auf stdout das Protokoll nicht zerschießt, sondern im Log der Extension landet.
    nonisolated(unsafe) private static var out: Int32 = -1
    private static let outLock = NSLock()

    static func run(dir: String) -> Never {
        out = dup(STDOUT_FILENO)
        dup2(STDERR_FILENO, STDOUT_FILENO)
        guard let state = kl_new(64 << 20) else { fail("Lua startet nicht") }
        nonisolated(unsafe) let L = state
        kl_set_sender { json, len in
            guard let json else { return }
            ExtHost.writeLine(json, len)
        }
        guard let prelude = Bundle.main.path(forResource: "prelude", ofType: "lua") else { fail("prelude.lua fehlt") }
        if let err = call({ kl_run_file(L, prelude, $0, $1) }) { fail(err) }

        let lua = DispatchQueue(label: "lua")
        // Eigener Lese-Thread: bei EOF endet der Prozess hier, auch wenn Lua gerade in einer Endlosschleife hängt.
        Thread {
            while let line = readLine() {
                lua.async {
                    if let err = call({ kl_dispatch(L, line, $0, $1) }) { FileHandle.standardError.write(Data((err + "\n").utf8)) }
                }
            }
            exit(0)
        }.start()
        dispatchMain()
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
