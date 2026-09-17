import Foundation
import os

struct CLIError: Error, CustomStringConvertible {
    let command: String
    let status: Int32
    let output: String
    var description: String { "\(command) → exit \(status): \(output.trimmingCharacters(in: .whitespacesAndNewlines))" }
}

/// Dünner Wrapper um das `claude`-Binary. Löst den Pfad selbst auf und erbt nie Shell-Aliase.
final class ClaudeCLI: Sendable {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "cli")
    let binary: String
    /// Login-Shell-Umgebung (`/bin/zsh -lc env`), einmal beim Start eingelesen.
    let environment: [String: String]

    init(binary: String, environment: [String: String]) {
        self.binary = binary
        // Aus einer Claude-Session heraus gestartet, erben die Prozesse sonst deren Marker: Claude Code
        // speichert dann kein Transcript („inherited CLAUDE_CODE_CHILD_SESSION marker“) und `--resume` findet nichts.
        // Nur die sitzungsbezogenen Variablen, Einstellungen wie CLAUDE_CODE_MAX_OUTPUT_TOKENS bleiben.
        let markers: Set = ["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_ENTRYPOINT",
                            "CLAUDE_CODE_SESSION_ATTENDED", "CLAUDE_CODE_MESSAGING_SOCKET", "CLAUDE_CODE_MESSAGING_TOKEN",
                            "CLAUDE_CODE_EXECPATH", "CLAUDE_PID", "CLAUDE_JOB_DIR"]
        var env = environment.filter { !markers.contains($0.key) }
        env["TERM"] = "xterm-256color"
        env["COLORTERM"] = "truecolor"
        if env["LANG"] == nil { env["LANG"] = "en_US.UTF-8" }
        self.environment = env
    }

    /// Datenordner von Claude Code (`~/.claude`, per `CLAUDE_CONFIG_DIR` verlegbar).
    var configDir: String { environment["CLAUDE_CONFIG_DIR"] ?? NSHomeDirectory() + "/.claude" }

    static func resolve() async -> ClaudeCLI {
        var env = ProcessInfo.processInfo.environment
        // NUL-getrennt, damit mehrzeilige Werte keine Variablen erfinden. Das führende NUL trennt Ausgaben des Profils ab.
        if let out = try? await runRaw("/bin/zsh", ["-lc", "printf '\\0'; exec env -0"], environment: nil, cwd: nil, timeout: 10).output {
            for line in out.split(separator: "\0", omittingEmptySubsequences: false).dropFirst() {
                guard let eq = line.firstIndex(of: "=") else { continue }
                let key = String(line[..<eq])
                guard key.range(of: "^[A-Za-z_][A-Za-z0-9_]*$", options: .regularExpression) != nil else { continue }
                env[key] = String(line[line.index(after: eq)...])
            }
        }
        var binary = NSHomeDirectory() + "/.local/bin/claude"
        if !FileManager.default.isExecutableFile(atPath: binary) {
            let found = (try? await runRaw("/bin/zsh", ["-lc", "command -v claude"], environment: env, cwd: nil, timeout: 10).output)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if found.hasPrefix("/") { binary = found }
        }
        log.info("claude binary: \(binary, privacy: .public)")
        return ClaudeCLI(binary: binary, environment: env)
    }

    /// Führt einen Prozess aus und liefert stdout+stderr. Der Prozess wird komplett auf einem
    /// Hintergrund-Thread aufgebaut, damit nichts Nicht-Sendable die Isolation kreuzt. Gelesen wird bis zum Prozessende,
    /// nicht bis EOF: ein Hintergrundjob aus dem Shell-Profil, der die Pipe erbt, hält sie sonst ewig offen.
    /// Nach `timeout` Sekunden bekommt der Prozess SIGKILL, die bis dahin gelesene Ausgabe kommt trotzdem zurück.
    static func runRaw(_ executable: String, _ args: [String], environment: [String: String]?, cwd: String?, timeout: TimeInterval = 60) async throws -> (status: Int32, output: String) {
        try await withCheckedThrowingContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async {
                let p = Process()
                p.executableURL = URL(fileURLWithPath: executable)
                p.arguments = args
                if let environment { p.environment = environment }
                if let cwd { p.currentDirectoryURL = URL(fileURLWithPath: cwd) }
                let pipe = Pipe()
                p.standardOutput = pipe
                p.standardError = pipe
                p.standardInput = FileHandle.nullDevice
                do { try p.run() } catch { cont.resume(throwing: error); return }
                let fd = pipe.fileHandleForReading.fileDescriptor
                _ = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK)
                let deadline = Date().addingTimeInterval(timeout)
                var data = Data(), buf = [UInt8](repeating: 0, count: 1 << 16)
                while true {
                    if Date() >= deadline {
                        log.warning("\(executable, privacy: .public) \(args.joined(separator: " "), privacy: .public): nach \(Int(timeout)) s abgebrochen")
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

    @discardableResult
    func run(_ args: [String], cwd: String? = nil) async throws -> String {
        if args.first != "agents" { ClaudeCLI.log.info("claude \(args.joined(separator: " "), privacy: .public)") }
        let r = try await ClaudeCLI.runRaw(binary, args, environment: environment, cwd: cwd)
        guard r.status == 0 else { throw CLIError(command: "claude " + args.joined(separator: " "), status: r.status, output: r.output) }
        return r.output
    }

    func agents() async throws -> [Agent] {
        let out = try await run(["agents", "--json", "--all"])
        guard let start = out.firstIndex(of: "[") else { return [] }
        return try Agent.decodeList(Data(out[start...].utf8))
    }

    /// Hält eine Hintergrund-Session aus `claude --bg` an; die Konversation bleibt für `--resume` erhalten.
    func stop(id: String) async throws { try await run(["stop", id]) }

    /// Argumente für den Claude-Prozess einer Kachel: vorhandene Konversation fortsetzen, sonst unter
    /// derselben sessionId neu beginnen (ohne erste Nachricht gibt es kein Transcript, `--resume` scheitert dann).
    static func sessionArgs(sessionId: String, hasTranscript: Bool) -> [String] {
        hasTranscript ? ["--resume", sessionId] : ["--session-id", sessionId]
    }

    /// Start-Flags aus den Einstellungen. Leerer String = Claude-Default, Flag entfällt.
    static func launchArgs(allowBypass: Bool, mode: String, model: String, effort: String) -> [String] {
        (allowBypass ? ["--allow-dangerously-skip-permissions"] : [])
            + (mode.isEmpty ? [] : ["--permission-mode", mode])
            + (model.isEmpty ? [] : ["--model", model])
            + (effort.isEmpty ? [] : ["--effort", effort])
    }

    /// Zuletzt gegen `docs/kadrell-verifikation.md` geprüfte Version: dort ist ab dieser CLI-Version das
    /// `--bg`-freie Verhalten verifiziert (Kindprozess/`--resume`, `state`/`status` in `agents --json`), auf
    /// das sich Kadrell verlässt. Ältere Versionen können daran unbemerkt scheitern.
    static let minVersion = (major: 2, minor: 1, patch: 273)

    /// Liest `claude --version` ("2.1.274 (Claude Code)") und vergleicht mit `minVersion`. `nil` = passt.
    func checkVersion() async -> String? {
        let out: String
        do { out = try await run(["--version"]) } catch {
            return String(localized: "\(binary): claude --version fehlgeschlagen: \(CLIError.firstLine(of: error))")
        }
        guard let v = ClaudeCLI.parseVersion(out) else {
            return String(localized: "\(binary): claude --version liefert kein erkennbares Versionsformat: \(out.trimmingCharacters(in: .whitespacesAndNewlines))")
        }
        guard v < ClaudeCLI.minVersion else { return nil }
        let found = "\(v.major).\(v.minor).\(v.patch)", tested = "\(ClaudeCLI.minVersion.major).\(ClaudeCLI.minVersion.minor).\(ClaudeCLI.minVersion.patch)"
        return String(localized: "claude \(found): älter als die von Kadrell getestete Version \(tested), bitte aktualisieren")
    }

    static func parseVersion(_ output: String) -> (major: Int, minor: Int, patch: Int)? {
        guard let match = output.range(of: #"\d+\.\d+\.\d+"#, options: .regularExpression) else { return nil }
        let parts = output[match].split(separator: ".").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        return (parts[0], parts[1], parts[2])
    }
}

private func < (lhs: (major: Int, minor: Int, patch: Int), rhs: (major: Int, minor: Int, patch: Int)) -> Bool {
    if lhs.major != rhs.major { return lhs.major < rhs.major }
    if lhs.minor != rhs.minor { return lhs.minor < rhs.minor }
    return lhs.patch < rhs.patch
}

extension CLIError {
    /// Erste Zeile der Prozessausgabe (bzw. Fehlerbeschreibung), für kurze Fehlermeldungen in der UI.
    static func firstLine(of error: Error) -> String {
        let text = ((error as? CLIError)?.output ?? error.localizedDescription).trimmingCharacters(in: .whitespacesAndNewlines)
        return text.split(separator: "\n").first.map(String.init) ?? text
    }
}
