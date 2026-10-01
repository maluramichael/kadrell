import Foundation

/// Externe Prozesse: mit Ausgabe und Timeout (`run`) oder ohne Warten (`spawn`).
enum ProcessRunner {
    /// Führt einen Prozess aus und liefert stdout (mit `mergeStderr` auch stderr). Der Prozess wird komplett auf einem
    /// Hintergrund-Thread aufgebaut, damit nichts Nicht-Sendable die Isolation kreuzt. Gelesen wird bis zum Prozessende,
    /// nicht bis EOF: ein Hintergrundjob aus dem Shell-Profil, der die Pipe erbt, hält sie sonst ewig offen.
    /// Nach `timeout` Sekunden bekommt der Prozess SIGKILL, die bis dahin gelesene Ausgabe kommt trotzdem zurück.
    static func run(_ executable: String, _ args: [String], environment: [String: String]? = nil, cwd: String? = nil,
                    timeout: TimeInterval = 60, mergeStderr: Bool = true) async throws -> (status: Int32, output: String) {
        let r = try await exec(executable, args, environment, cwd, timeout, mergeStderr ? .merged : .discarded)
        return (r.status, r.stdout)
    }

    /// Wie `run`, aber stdout und stderr getrennt: Warnungen auf stderr verfälschen so keine geparste Ausgabe.
    static func runSeparated(_ executable: String, _ args: [String], environment: [String: String]? = nil, cwd: String? = nil,
                             timeout: TimeInterval = 60) async throws -> (status: Int32, stdout: String, stderr: String) {
        try await exec(executable, args, environment, cwd, timeout, .separate)
    }

    private enum ErrMode { case merged, discarded, separate }

    private static func exec(_ executable: String, _ args: [String], _ environment: [String: String]?, _ cwd: String?,
                             _ timeout: TimeInterval, _ errMode: ErrMode) async throws -> (status: Int32, stdout: String, stderr: String) {
        try await withCheckedThrowingContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async {
                let p = process(executable, args, environment, cwd), outPipe = Pipe(), errPipe = Pipe()
                p.standardOutput = outPipe
                switch errMode {
                case .merged: p.standardError = outPipe
                case .discarded: p.standardError = FileHandle.nullDevice
                case .separate: p.standardError = errPipe
                }
                do { try p.run() } catch { cont.resume(throwing: error); return }
                var streams = [Stream(outPipe)]
                if errMode == .separate { streams.append(Stream(errPipe)) }
                readAll(&streams, p, deadline: Date().addingTimeInterval(timeout), label: "\(executable) \(args.joined(separator: " "))", timeout: timeout)
                p.waitUntilExit()
                cont.resume(returning: (p.terminationStatus, String(decoding: streams[0].data, as: UTF8.self),
                                        String(decoding: streams.count > 1 ? streams[1].data : Data(), as: UTF8.self)))
            }
        }
    }

    /// Liest alle Streams bis Prozessende, EOF oder Deadline (dann SIGKILL).
    private static func readAll(_ streams: inout [Stream], _ p: Process, deadline: Date, label: String, timeout: TimeInterval) {
        while true {
            if Date() >= deadline {
                ClaudeCLI.log.warning("\(label, privacy: .public): nach \(Int(timeout)) s abgebrochen")
                if p.isRunning { kill(p.processIdentifier, SIGKILL) }
                return
            }
            let exited = !p.isRunning
            var pfds = streams.map { pollfd(fd: $0.fd, events: Int16(POLLIN), revents: 0) }
            var progressed = false
            if poll(&pfds, nfds_t(pfds.count), exited ? 0 : 100) > 0 {
                for i in streams.indices where pfds[i].revents != 0 { progressed = streams[i].readChunk() || progressed }
            }
            if streams.allSatisfy(\.eof) || (exited && !progressed) { return }
        }
    }

    /// Nicht blockierend gelesenes Pipe-Ende.
    private struct Stream {
        let fd: Int32
        var data = Data(), eof = false
        init(_ pipe: Pipe) {
            fd = pipe.fileHandleForReading.fileDescriptor
            _ = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK)
        }
        /// true, wenn Daten kamen.
        mutating func readChunk() -> Bool {
            var buf = [UInt8](repeating: 0, count: 1 << 16)
            let n = read(fd, &buf, buf.count)
            if n > 0 { data.append(buf, count: n); return true }
            if n == 0 { eof = true }
            return false
        }
    }

    /// Startet ohne Warten, stdin leer. `discardOutput`: stdout und stderr ins Leere statt geerbt.
    @discardableResult
    static func spawn(_ executable: String, _ args: [String], environment: [String: String]? = nil, cwd: String? = nil, discardOutput: Bool = false) throws -> Process {
        let p = process(executable, args, environment, cwd)
        if discardOutput { p.standardOutput = FileHandle.nullDevice; p.standardError = FileHandle.nullDevice }
        try p.run()
        return p
    }

    private static func process(_ executable: String, _ args: [String], _ environment: [String: String]?, _ cwd: String?) -> Process {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: executable)
        p.arguments = args
        if let environment { p.environment = environment }
        if let cwd { p.currentDirectoryURL = URL(fileURLWithPath: cwd) }
        p.standardInput = FileHandle.nullDevice
        return p
    }
}
