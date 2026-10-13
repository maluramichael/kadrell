import Foundation

/// Start-Argumente und Abfragen für Pi (`pi`, pi.dev). Geprüft gegen pi 1.2.0 (`pi --help`, `docs/` im Paket).
enum PiCLI {
    /// Integration, die Kadrell jedem eigenen Pi-Prozess per `-e` mitgibt (wie `--settings` bei Claude): meldet
    /// Zustand, sessionId, Titel und Modell über `kadrell status`. Nichts wird in Pis Konfiguration eingetragen.
    static var extensionPath: String? { Bundle.main.path(forResource: "kadrell-pi", ofType: "js") }

    /// `--session-id` nimmt genau diese Id und legt die Session an, wenn es sie noch nicht gibt: ein Argument
    /// für neu und fortsetzen, Kadrell vergibt die Id selbst wie bei Claude.
    static func sessionArgs(sessionId: String) -> [String] { ["--session-id", sessionId] }

    /// Erste Nachricht hinter `--`, sonst liest Pi einen Prompt wie `--help` als Option.
    static func promptArgs(_ prompt: String) -> [String] { ["--", prompt] }

    /// Start-Flags aus den Einstellungen, leerer String = Pis eigene Einstellung, Flag entfällt.
    static func launchArgs(model: String, thinking: String, extensionPath: String? = PiCLI.extensionPath) -> [String] {
        (model.isEmpty ? [] : ["--model", model])
            + (thinking.isEmpty ? [] : ["--thinking", thinking])
            + (extensionPath.map { ["-e", $0] } ?? [])
    }

    /// Datenordner von Pi (`~/.pi/agent`, per `PI_CODING_AGENT_DIR` verlegbar).
    static func agentDir(environment: [String: String]) -> String {
        environment["PI_CODING_AGENT_DIR"] ?? NSHomeDirectory() + "/.pi/agent"
    }

    /// Gibt es zu dieser Id schon eine gespeicherte Konversation? Dateien heißen `<zeitstempel>_<id>.jsonl`,
    /// je Projektordner ein Unterordner unter `sessions/`.
    static func hasSession(sessionId: String, environment: [String: String]) -> Bool {
        let root = environment["PI_CODING_AGENT_SESSION_DIR"] ?? agentDir(environment: environment) + "/sessions"
        let fm = FileManager.default
        let suffix = "_" + sessionId + ".jsonl"
        for dir in (try? fm.contentsOfDirectory(atPath: root)) ?? [] {
            if ((try? fm.contentsOfDirectory(atPath: root + "/" + dir)) ?? []).contains(where: { $0.hasSuffix(suffix) }) { return true }
        }
        return false
    }

    /// Modelle mit Zugangsdaten als `provider/id`, aus `pi --list-models --offline`.
    static func listModels(binary: String, environment: [String: String]) async -> [String] {
        guard let out = try? await ProcessRunner.run(binary, ["--list-models", "--offline"], environment: environment, timeout: 15).output else { return [] }
        return parseModels(out)
    }

    /// Tabelle mit Kopfzeile `provider  model  context …`. Modell-Ids dürfen Leerzeichen enthalten, deshalb
    /// nach den Spalten der Kopfzeile schneiden statt nach Leerraum.
    static func parseModels(_ output: String) -> [String] {
        let lines = output.split(separator: "\n", omittingEmptySubsequences: true).map(String.init)
        guard let header = lines.firstIndex(where: { $0.hasPrefix("provider") }),
              let modelCol = lines[header].range(of: "model")?.lowerBound,
              let contextCol = lines[header].range(of: "context")?.lowerBound else { return [] }
        let m = lines[header].distance(from: lines[header].startIndex, to: modelCol)
        let c = lines[header].distance(from: lines[header].startIndex, to: contextCol)
        return lines[(header + 1)...].compactMap { line in
            let chars = Array(line)
            guard chars.count > c else { return nil }
            let provider = String(chars[..<m]).trimmingCharacters(in: .whitespaces)
            let id = String(chars[m..<c]).trimmingCharacters(in: .whitespaces)
            return provider.isEmpty || id.isEmpty ? nil : provider + "/" + id
        }
    }

    /// Pis eigener Default aus `settings.json` (`defaultProvider`/`defaultModel`), für „Standard (…)“ in den Einstellungen.
    static func defaultModel(environment: [String: String]) -> String? {
        let url = URL(fileURLWithPath: agentDir(environment: environment) + "/settings.json")
        guard let data = try? Data(contentsOf: url),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let model = json["defaultModel"] as? String, !model.isEmpty else { return nil }
        return (json["defaultProvider"] as? String).map { $0 + "/" + model } ?? model
    }
}
