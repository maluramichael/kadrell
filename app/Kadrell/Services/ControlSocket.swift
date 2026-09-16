import Foundation
import os

/// Unix-Socket der laufenden App, wie der tmux-Server: eine Zeile JSON (`ControlRequest`) hin, eine zurück.
/// Nur der eigene Benutzer kommt durch (Dateirechte 0600 und `getpeereid`).
enum ControlSocket {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "control")
    static let maxRequest = 1 << 20

    static var defaultPath: String {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return base.appendingPathComponent("de.malura.kadrell/kadrell.sock").path
    }

    /// `sun_path` fasst 104 Bytes. Längere Pfade (sehr lange Benutzernamen) scheitern mit klarer Meldung.
    static func address(_ path: String) throws -> sockaddr_un {
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8)
        let cap = MemoryLayout.size(ofValue: addr.sun_path)
        guard bytes.count < cap else { throw ControlError("Socket-Pfad zu lang: \(path)") }
        withUnsafeMutableBytes(of: &addr.sun_path) { buf in
            buf.copyBytes(from: bytes)
            buf[bytes.count] = 0
        }
        return addr
    }

    static func withAddress<T>(_ addr: inout sockaddr_un, _ body: (UnsafePointer<sockaddr>, socklen_t) -> T) -> T {
        withUnsafePointer(to: &addr) { $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { body($0, socklen_t(MemoryLayout<sockaddr_un>.size)) } }
    }

    /// Liest bis EOF oder Zeilenende.
    static func readLine(_ fd: Int32) -> Data? {
        var data = Data()
        var buf = [UInt8](repeating: 0, count: 65536)
        while data.count <= maxRequest {
            let n = read(fd, &buf, buf.count)
            if n < 0, errno == EINTR { continue }
            if n <= 0 { break }
            data.append(contentsOf: buf[0..<n])
            if buf[0..<n].contains(10) { break }
        }
        return data.isEmpty || data.count > maxRequest ? nil : data
    }

    static func writeAll(_ fd: Int32, _ data: Data) {
        data.withUnsafeBytes { raw in
            var off = 0
            while off < raw.count {
                let n = write(fd, raw.baseAddress! + off, raw.count - off)
                if n < 0, errno == EINTR { continue }
                if n <= 0 { return }
                off += n
            }
        }
    }
}

/// Lauscht auf dem Socket und reicht jede Anfrage an `handler` auf dem Main-Thread weiter.
final class ControlServer: @unchecked Sendable {
    let path: String
    private let handler: @MainActor @Sendable (ControlRequest) -> ControlResponse
    private let queue = DispatchQueue(label: "de.malura.kadrell.control")
    private var listenFd: Int32 = -1
    private var source: DispatchSourceRead?

    init(path: String = ControlSocket.defaultPath, handler: @escaping @MainActor @Sendable (ControlRequest) -> ControlResponse) {
        self.path = path
        self.handler = handler
    }

    func start() throws {
        try FileManager.default.createDirectory(atPath: (path as NSString).deletingLastPathComponent, withIntermediateDirectories: true)
        var addr = try ControlSocket.address(path)
        // Reste eines abgestürzten Laufs. Eine zweite Instanz beendet sich vorher selbst (AppDelegate).
        unlink(path)
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw ControlError("socket: \(String(cString: strerror(errno)))") }
        let old = umask(0o077)
        let rc = ControlSocket.withAddress(&addr) { bind(fd, $0, $1) }
        umask(old)
        guard rc == 0, chmod(path, 0o600) == 0, listen(fd, 16) == 0 else {
            let msg = String(cString: strerror(errno))
            close(fd)
            throw ControlError("bind \(path): \(msg)")
        }
        listenFd = fd
        let src = DispatchSource.makeReadSource(fileDescriptor: fd, queue: queue)
        src.setEventHandler { [weak self] in self?.acceptOne() }
        src.setCancelHandler { close(fd) }
        src.resume()
        source = src
        ControlSocket.log.info("Steuer-Socket \(self.path, privacy: .public)")
    }

    func stop() {
        source?.cancel()
        source = nil
        unlink(path)
    }

    private func acceptOne() {
        let fd = accept(listenFd, nil, nil)
        guard fd >= 0 else { return }
        var uid = uid_t(), gid = gid_t()
        guard getpeereid(fd, &uid, &gid) == 0, uid == getuid() else { close(fd); return }
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        var tv = timeval(tv_sec: 5, tv_usec: 0)
        setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, socklen_t(MemoryLayout<timeval>.size))
        // Lesen blockiert höchstens 5 s und nur diese Queue; der Client schickt die Zeile sofort.
        guard let data = ControlSocket.readLine(fd), let req = try? JSONDecoder().decode(ControlRequest.self, from: data) else {
            ControlSocket.writeAll(fd, ControlServer.encode(.fail("ungültige Anfrage")))
            close(fd)
            return
        }
        let handler = handler, queue = queue
        Task { @MainActor in
            let out = ControlServer.encode(handler(req))
            queue.async { ControlSocket.writeAll(fd, out); close(fd) }
        }
    }

    static func encode(_ r: ControlResponse) -> Data {
        var d = (try? JSONEncoder().encode(r)) ?? Data()
        d.append(10)
        return d
    }
}

/// Die Kommandozeile: dasselbe Binary wie die App, mit Unterbefehl aufgerufen (`main.swift`).
enum ControlClient {
    /// Schickt die Anfrage, `nil`, wenn niemand lauscht.
    static func send(_ req: ControlRequest, path: String) throws -> ControlResponse? {
        var addr = try ControlSocket.address(path)
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw ControlError("socket: \(String(cString: strerror(errno)))") }
        defer { close(fd) }
        var on: Int32 = 1
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &on, socklen_t(MemoryLayout<Int32>.size))
        guard ControlSocket.withAddress(&addr, { connect(fd, $0, $1) }) == 0 else { return nil }
        var line = try JSONEncoder().encode(req)
        line.append(10)
        ControlSocket.writeAll(fd, line)
        shutdown(fd, SHUT_WR)
        guard let data = ControlSocket.readLine(fd) else { throw ControlError("keine Antwort von Kadrell") }
        return try JSONDecoder().decode(ControlResponse.self, from: data)
    }

    static func run(_ argv: [String]) -> Int32 {
        if (try? ControlCommand.parse(argv)) == .help {
            print(ControlCommand.usage)
            return 0
        }
        let env = ProcessInfo.processInfo.environment
        let path = env["KADRELL_SOCKET"] ?? ControlSocket.defaultPath
        let req = ControlRequest(argv: argv, cwd: FileManager.default.currentDirectoryPath, caller: env["KADRELL_SESSION_KEY"])
        do {
            var resp = try send(req, path: path)
            if resp == nil {
                launchApp()
                // Die App braucht fürs Einlesen der Login-Shell-Umgebung ein paar Sekunden, erst danach lauscht sie.
                let deadline = Date().addingTimeInterval(30)
                while resp == nil, Date() < deadline {
                    usleep(250_000)
                    resp = try send(req, path: path)
                }
            }
            guard let resp else { throw ControlError("Kadrell antwortet nicht (\(path))") }
            FileHandle.standardOutput.write(Data(resp.stdout.utf8))
            FileHandle.standardError.write(Data(resp.stderr.utf8))
            return resp.status
        } catch {
            let msg = (error as? ControlError)?.message ?? String(describing: error)
            FileHandle.standardError.write(Data("kadrell: \(msg)\n".utf8))
            return 1
        }
    }

    /// Startet genau das Bundle, zu dem dieses Binary gehört (auch über den Symlink), im Hintergrund.
    private static func launchApp() {
        FileHandle.standardError.write(Data("kadrell: starte Kadrell …\n".utf8))
        let bundle = Bundle.main.bundleURL
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        p.arguments = bundle.pathExtension == "app" ? ["-g", bundle.path] : ["-g", "-b", "de.malura.kadrell"]
        try? p.run()
        p.waitUntilExit()
    }
}
