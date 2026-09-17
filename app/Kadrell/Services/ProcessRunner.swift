import Foundation

/// Externe Prozesse: mit Ausgabe und Timeout (`run`) oder ohne Warten (`spawn`).
enum ProcessRunner {
    /// Führt einen Prozess aus und liefert stdout (mit `mergeStderr` auch stderr). Der Prozess wird komplett auf einem
    /// Hintergrund-Thread aufgebaut, damit nichts Nicht-Sendable die Isolation kreuzt. Gelesen wird bis zum Prozessende,
    /// nicht bis EOF: ein Hintergrundjob aus dem Shell-Profil, der die Pipe erbt, hält sie sonst ewig offen.
    /// Nach `timeout` Sekunden bekommt der Prozess SIGKILL, die bis dahin gelesene Ausgabe kommt trotzdem zurück.
    static func run(_ executable: String, _ args: [String], environment: [String: String]? = nil, cwd: String? = nil,
                    timeout: TimeInterval = 60, mergeStderr: Bool = true) async throws -> (status: Int32, output: String) {
        try await withCheckedThrowingContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async {
                let p = process(executable, args, environment, cwd), pipe = Pipe()
                p.standardOutput = pipe
                p.standardError = mergeStderr ? pipe : FileHandle.nullDevice
                do { try p.run() } catch { cont.resume(throwing: error); return }
                let fd = pipe.fileHandleForReading.fileDescriptor
                _ = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK)
                let deadline = Date().addingTimeInterval(timeout)
                var data = Data(), buf = [UInt8](repeating: 0, count: 1 << 16)
                while true {
                    if Date() >= deadline {
                        ClaudeCLI.log.warning("\(executable, privacy: .public) \(args.joined(separator: " "), privacy: .public): nach \(Int(timeout)) s abgebrochen")
                        if p.isRunning { kill(p.processIdentifier, SIGKILL) }
                        break
                    }
                    let exited = !p.isRunning
                    var pfd = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
                    if poll(&pfd, 1, exited ? 0 : 100) > 0 {
                        let n = read(fd, &buf, buf.count)
                        if n > 0 { data.append(buf, count: n); continue }
                        if n == 0 { break }
                    }
                    if exited { break }
                }
                p.waitUntilExit()
                cont.resume(returning: (p.terminationStatus, String(decoding: data, as: UTF8.self)))
            }
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
