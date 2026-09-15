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
        var env = environment
        env["TERM"] = "xterm-256color"
        env["COLORTERM"] = "truecolor"
        if env["LANG"] == nil { env["LANG"] = "en_US.UTF-8" }
        self.environment = env
    }

    /// `KEY=VALUE`-Liste für SwiftTerms `startProcess(environment:)`.
    var environmentList: [String] { environment.map { "\($0.key)=\($0.value)" } }

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

    func agents() async throws -> [Session] {
        let out = try await run(["agents", "--json", "--all"])
        guard let start = out.firstIndex(of: "[") else { return [] }
        return try Session.decodeList(Data(out[start...].utf8))
    }

    /// Startet eine Hintergrund-Session und liefert die kurze Id aus `backgrounded · <id> · <name>`.
    func start(cwd: String, name: String, prompt: String) async throws -> String? {
        var args = ["--bg", "--name", name]
        if !prompt.isEmpty { args.append(prompt) }
        let out = try await run(args, cwd: cwd)
        return ClaudeCLI.parseBackgroundedId(out)
    }

    func resume(sessionId: String, cwd: String) async throws -> String? {
        ClaudeCLI.parseBackgroundedId(try await run(["--bg", "--resume", sessionId], cwd: cwd))
    }

    func stop(id: String) async throws { try await run(["stop", id]) }
    func remove(id: String) async throws { try await run(["rm", id]) }
    func logs(id: String) async throws -> String { try await run(["logs", id]) }

    static func parseBackgroundedId(_ output: String) -> String? {
        guard let r = output.range(of: "backgrounded · ([0-9a-f]{8})", options: .regularExpression) else { return nil }
        return String(output[r].suffix(8))
    }

    static func shortName(prompt: String, cwd: String, counter: Int) -> String {
        let p = prompt.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "\n", with: " ")
        if !p.isEmpty { return String(p.prefix(48)) }
        return "\(URL(fileURLWithPath: cwd).lastPathComponent)-\(counter)"
    }
}
