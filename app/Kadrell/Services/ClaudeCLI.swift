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

    /// `KEY=VALUE`-Liste für SwiftTerms `startProcess(environment:)`.
    var environmentList: [String] { environment.map { "\($0.key)=\($0.value)" } }
    /// Datenordner von Claude Code (`~/.claude`, per `CLAUDE_CONFIG_DIR` verlegbar).
    var configDir: String { environment["CLAUDE_CONFIG_DIR"] ?? NSHomeDirectory() + "/.claude" }

    static func resolve() async -> ClaudeCLI {
        var env = ProcessInfo.processInfo.environment
        if let out = try? await runRaw("/bin/zsh", ["-lc", "env"], environment: nil, cwd: nil).output {
            for line in out.split(separator: "\n") {
                guard let eq = line.firstIndex(of: "=") else { continue }
                let key = String(line[..<eq])
                guard key.range(of: "^[A-Za-z_][A-Za-z0-9_]*$", options: .regularExpression) != nil else { continue }
                env[key] = String(line[line.index(after: eq)...])
            }
        }
        var binary = NSHomeDirectory() + "/.local/bin/claude"
        if !FileManager.default.isExecutableFile(atPath: binary) {
            let found = (try? await runRaw("/bin/zsh", ["-lc", "command -v claude"], environment: env, cwd: nil).output)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if found.hasPrefix("/") { binary = found }
        }
        log.info("claude binary: \(binary, privacy: .public)")
        return ClaudeCLI(binary: binary, environment: env)
    }

    /// Führt einen Prozess aus und liefert stdout+stderr. Der Prozess wird komplett auf einem
    /// Hintergrund-Thread aufgebaut, damit nichts Nicht-Sendable die Isolation kreuzt.
    static func runRaw(_ executable: String, _ args: [String], environment: [String: String]?, cwd: String?) async throws -> (status: Int32, output: String) {
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
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
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
}
