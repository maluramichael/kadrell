import Foundation

/// Externe Prozesse: mit Ausgabe und Timeout (`run`) oder ohne Warten (`spawn`).
enum ProcessRunner {
    /// Führt einen Prozess aus und liefert stdout (mit `mergeStderr` auch stderr). Der Prozess wird komplett auf einem
    /// Hintergrund-Thread aufgebaut, damit nichts Nicht-Sendable die Isolation kreuzt. Gelesen wird bis zum Prozessende,
    /// nicht bis EOF: ein Hintergrundjob aus dem Shell-Profil, der die Pipe erbt, hält sie sonst ewig offen.
    /// Nach `timeout` Sekunden bekommt der Prozess SIGKILL, die bis dahin gelesene Ausgabe kommt trotzdem zurück.
    static func run(_ executable: String, _ args: [String], environment: [String: String]? = nil, cwd: String? = nil,
                    timeout: TimeInterval = 60, mergeStderr: Bool = true) async throws -> (status: Int32, output: String) {
        let r = try await execute(executable, args, environment, cwd, timeout, stderr: mergeStderr ? .merge : .discard)
        return (r.status, r.output)
    }

    /// Wie `run`, liefert stderr aber getrennt von stdout zurück statt es zu verwerfen oder zu vermischen.
    static func runCapturingStderr(_ executable: String, _ args: [String], environment: [String: String]? = nil, cwd: String? = nil,
                                   timeout: TimeInterval = 60) async throws -> (status: Int32, output: String, stderr: String) {
        try await execute(executable, args, environment, cwd, timeout, stderr: .capture)
    }

    private enum StderrMode { case merge, discard, capture }

    private static func execute(_ executable: String, _ args: [String], _ environment: [String: String]?, _ cwd: String?,
                                _ timeout: TimeInterval, stderr mode: StderrMode) async throws -> (status: Int32, output: String, stderr: String) {
        try await withCheckedThrowingContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async {
                let p = process(executable, args, environment, cwd), pipe = Pipe(), errPipe = Pipe()
                p.standardOutput = pipe
                switch mode {
                case .merge: p.standardError = pipe
                case .discard: p.standardError = FileHandle.nullDevice
                case .capture: p.standardError = errPipe
                }
                do { try p.run() } catch { cont.resume(throwing: error); return }
                var fds = [pipe.fileHandleForReading.fileDescriptor]
                if mode == .capture { fds.append(errPipe.fileHandleForReading.fileDescriptor) }
                for fd in fds { _ = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK) }
                let deadline = Date().addingTimeInterval(timeout)
                var data = [Data](repeating: Data(), count: fds.count), open = [Bool](repeating: true, count: fds.count)
                while open.contains(true) {
                    if Date() >= deadline {
                        ClaudeCLI.log.warning("\(executable, privacy: .public) \(args.joined(separator: " "), privacy: .public): nach \(Int(timeout)) s abgebrochen")
                        if p.isRunning { kill(p.processIdentifier, SIGKILL) }
                        break
                    }
                    let exited = !p.isRunning
                    let got = drain(fds, &data, &open, timeoutMs: exited ? 0 : 100)
                    if exited && !got { break }
                }
                p.waitUntilExit()
                cont.resume(returning: (p.terminationStatus, String(decoding: data[0], as: UTF8.self),
                                        String(decoding: data.count > 1 ? data[1] : Data(), as: UTF8.self)))
            }
        }
    }

    /// Wartet bis zu `timeoutMs` auf Ausgabe und liest, was da ist. EOF schließt den jeweiligen Kanal (`open`).
    /// Liefert, ob Daten gelesen wurden.
    private static func drain(_ fds: [Int32], _ data: inout [Data], _ open: inout [Bool], timeoutMs: Int32) -> Bool {
        let active = fds.indices.filter { open[$0] }
        var pfds = active.map { pollfd(fd: fds[$0], events: Int16(POLLIN), revents: 0) }
        guard poll(&pfds, nfds_t(pfds.count), timeoutMs) > 0 else { return false }
        var got = false, buf = [UInt8](repeating: 0, count: 1 << 16)
        for (i, pfd) in zip(active, pfds) where pfd.revents != 0 {
            let n = read(fds[i], &buf, buf.count)
            if n > 0 { data[i].append(buf, count: n); got = true } else if n == 0 { open[i] = false }
        }
        return got
    }

    /// SIGKILL an die direkten Kinder von `pid`. Process legt jedes Kind in eine eigene Prozessgruppe, ein Signal an
    /// die Gruppe der Eltern erreicht sie nicht, und stirbt der Elternprozess, laufen sie unter launchd weiter.
    static func killChildren(of pid: pid_t) {
        // ponytail: höchstens 256 Kinder, mehr startet ein Extension-Helper nicht gleichzeitig.
        var buf = [pid_t](repeating: 0, count: 256)
        let n = proc_listchildpids(pid, &buf, Int32(buf.count * MemoryLayout<pid_t>.size))
        for child in buf.prefix(max(0, min(Int(n), buf.count))) where child > 0 { kill(child, SIGKILL) }
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
