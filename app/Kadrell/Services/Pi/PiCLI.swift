import Foundation
import os

/// Pi (`pi`, earendil-works) als zweiter Agent neben Claude Code, immer mit einem lokalen Modell. Kadrell gibt Modell
/// und Adresse über die Umgebung mit; die eigene Pi-Extension (`kadrell.ts`, siehe `installExtension`) macht daraus den
/// Provider „kadrell“ und meldet den Status der Kachel per `kadrell status`, so wie die Hooks bei Claude Code.
/// Die Konversation hängt an `--session-id`: Pi legt sie beim ersten Start an und setzt sie danach fort.
enum PiCLI {
    private static let log = Logger(subsystem: "de.malura.kadrell", category: "pi")
    /// Name des Providers, den `kadrell.ts` registriert.
    static let provider = "kadrell"

    /// `pi` im PATH der Login-Shell, sonst unter `~/.local/bin` (Standard des Pi-Installers). nil = Pi fehlt.
    static func binary(environment: [String: String], home: String = NSHomeDirectory(), fm: FileManager = .default) -> String? {
        let path = (environment["PATH"] ?? "").split(separator: ":").map(String.init) + [home + "/.local/bin"]
        return path.lazy.map { $0 + "/pi" }.first { fm.isExecutableFile(atPath: $0) }
    }

    /// Aufruf und zusätzliche Umgebung für eine Pi-Kachel. `baseURL` ist die Adresse des Providers ohne `/v1`;
    /// fehlt sie (llama.cpp ließ sich nicht starten), startet Pi trotzdem und zeigt den Fehler selbst.
    static func launch(binary: String, session: Session, model: LocalModelRef, baseURL: URL?, prompt: String?) -> (args: [String], env: [String: String]) {
        var args = ["--provider", provider, "--model", model.name, "--session-id", session.piSessionId]
        if let prompt, !prompt.isEmpty { args.append(safePrompt(prompt)) }
        var env = ["KADRELL_PI": "1", "KADRELL_PI_MODEL": model.name]
        if let baseURL { env["KADRELL_PI_BASE_URL"] = baseURL.appending(path: "v1").absoluteString }
        return (args, env)
    }

    /// Pi liest ein Argument mit `-` als Option und eines mit `@` als Datei: ein Leerzeichen davor macht es zum Prompt.
    static func safePrompt(_ p: String) -> String { p.hasPrefix("-") || p.hasPrefix("@") ? " " + p : p }

    static let extensionSource: String? = Bundle.main.path(forResource: "kadrell", ofType: "ts")
        .flatMap { try? String(contentsOfFile: $0, encoding: .utf8) }

    /// Legt `~/.pi/agent/extensions/kadrell.ts` an oder bringt sie auf den Stand dieses Kadrell. Vor jedem Start einer
    /// Pi-Session, nicht beim App-Start: wer Pi nie aus Kadrell startet, bekommt nichts in `~/.pi`.
    @discardableResult
    static func installExtension(source: String? = extensionSource, home: String = NSHomeDirectory(), fm: FileManager = .default) -> Bool {
        guard let source else { log.error("kadrell.ts fehlt im Bundle"); return false }
        let dir = home + "/.pi/agent/extensions", file = dir + "/kadrell.ts"
        if fm.contents(atPath: file).map({ String(decoding: $0, as: UTF8.self) }) == source { return true }
        do {
            try fm.createDirectory(atPath: dir, withIntermediateDirectories: true)
            try Data(source.utf8).write(to: URL(fileURLWithPath: file), options: .atomic)
            log.info("Pi-Extension nach \(file, privacy: .private) geschrieben")
            return true
        } catch {
            log.error("Pi-Extension nicht geschrieben: \(error.localizedDescription, privacy: .public)")
            return false
        }
    }
}
