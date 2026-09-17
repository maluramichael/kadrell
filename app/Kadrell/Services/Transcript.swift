import Foundation

/// Letzte Textantwort von Claude aus dem Transcript `<configDir>/projects/<slug>/<sessionId>.jsonl` (`ClaudeCLI.configDir`).
enum Transcript {
    struct Entry: Sendable {
        let path: String
        let size: UInt64
        let text: String?
        /// Pfade und Bash-Kommandos aus `tool_use`-Aufrufen im gelesenen Tail, neueste zuerst. Für die
        /// Worktree-Erkennung (`Worktree.active`).
        let toolCandidates: [String]
    }

    /// Über alle Projektordner gesucht statt aus `cwd` abgeleitet: nach einem Worktree-Wechsel stimmt der Slug nicht mehr.
    static func path(sessionId: String, configDir: String) -> String? {
        let fm = FileManager.default, root = configDir + "/projects"
        guard !sessionId.isEmpty, let dirs = try? fm.contentsOfDirectory(atPath: root) else { return nil }
        return dirs.lazy.map { "\(root)/\($0)/\(sessionId).jsonl" }.first { fm.fileExists(atPath: $0) }
    }

    /// Kein Treffer bei der Pfadsuche: Zeitpunkt, ab dem wieder gesucht werden darf, statt bei jedem Poll erneut
    /// den ganzen Ordner zu scannen (z. B. eine brandneue Session, deren Transcript noch nicht geschrieben ist).
    nonisolated(unsafe) private static var notFoundUntil: [String: Date] = [:]

    /// Liest nur Transcripts neu, deren Größe sich geändert hat, gewachsene nur ab der alten Größe statt komplett.
    /// `wantText`: `lastText` kostet eine eigene JSON-Passage, nötig nur wenn `Settings.showLastMessage` an ist.
    static func refresh(_ sessionIds: [String], configDir: String, cache: [String: Entry], wantText: Bool = true) -> [String: Entry] {
        var out: [String: Entry] = [:]
        let now = Date()
        for id in sessionIds {
            let resolved: String?
            if let p = cache[id]?.path { resolved = p }
            else if let until = notFoundUntil[id], until > now { continue }
            else if let p = path(sessionId: id, configDir: configDir) { notFoundUntil[id] = nil; resolved = p }
            else { notFoundUntil[id] = now.addingTimeInterval(30); continue }
            guard let path = resolved else { continue }
            let size = (try? FileManager.default.attributesOfItem(atPath: path)[.size] as? UInt64) ?? 0
            if let old = cache[id] {
                if old.size == size { out[id] = old; continue }
                if size > old.size { out[id] = grow(path: path, size: size, from: old, wantText: wantText); continue }
            }
            // Erstes Lesen oder geschrumpft (z. B. `/clear`, neue Datei unter altem Pfad): voller Tail.
            out[id] = readFull(path: path, size: size, wantText: wantText)
        }
        return out
    }

    /// Nur die seit `old.size` neu geschriebenen Bytes lesen und parsen; wo nichts Neueres trifft, gilt der alte Wert.
    private static func grow(path: String, size: UInt64, from old: Entry, wantText: Bool) -> Entry {
        guard let h = FileHandle(forReadingAtPath: path) else { return readFull(path: path, size: size, wantText: wantText) }
        defer { try? h.close() }
        try? h.seek(toOffset: old.size)
        let data = try? h.readToEnd()
        let newCandidates = data.map { toolCandidates(jsonl: $0) } ?? []
        return Entry(path: path, size: size, text: (wantText ? data.flatMap { lastText(jsonl: $0) } : nil) ?? old.text,
                     toolCandidates: newCandidates.isEmpty ? old.toolCandidates : Array((newCandidates + old.toolCandidates).prefix(20)))
    }

    private static func readFull(path: String, size: UInt64, wantText: Bool) -> Entry {
        let data = tailData(path: path)
        return Entry(path: path, size: size, text: wantText ? data.flatMap { lastText(jsonl: $0) } : nil,
                     toolCandidates: data.map { toolCandidates(jsonl: $0) } ?? [])
    }

    /// Erste echte Eingabe des Nutzers, gekürzt. Slash-Commands und ihre Ausgabe (`<command-name>` …) zählen nicht.
    static func firstPrompt(path: String, head: Int = 1 << 18, limit: Int = 50) -> String? {
        guard let h = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? h.close() }
        guard let data = try? h.read(upToCount: head) else { return nil }
        for line in data.split(separator: UInt8(ascii: "\n")) {
            guard let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
                  obj["type"] as? String == "user", obj["isMeta"] as? Bool != true, obj["isSidechain"] as? Bool != true,
                  let content = (obj["message"] as? [String: Any])?["content"] else { continue }
            let text = (content as? String)
                ?? (content as? [[String: Any]])?.compactMap { $0["type"] as? String == "text" ? $0["text"] as? String : nil }.joined(separator: " ")
                ?? ""
            let flat = text.replacingOccurrences(of: "\\[Image #\\d+\\]", with: "", options: .regularExpression)
                .split(whereSeparator: \.isWhitespace).joined(separator: " ")
            if flat.isEmpty || flat.hasPrefix("<") { continue }
            return flat.count > limit ? flat.prefix(limit).trimmingCharacters(in: .whitespaces) + "…" : flat
        }
        return nil
    }

    /// ponytail: nur das letzte MB, reicht solange keine Tool-Ausgabe allein größer ist; sonst ganze Datei lesen.
    private static func tailData(path: String, tail: UInt64 = 1 << 20) -> Data? {
        guard let h = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? h.close() }
        let size = (try? h.seekToEnd()) ?? 0
        try? h.seek(toOffset: size > tail ? size - tail : 0)
        return try? h.readToEnd()
    }

    /// Von hinten die erste Assistant-Zeile mit Text. Eine angeschnittene erste Zeile ist kein JSON und fällt raus.
    static func lastText(jsonl data: Data) -> String? {
        for line in data.split(separator: UInt8(ascii: "\n")).reversed() {
            guard let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
                  obj["type"] as? String == "assistant", obj["isSidechain"] as? Bool != true,
                  let content = (obj["message"] as? [String: Any])?["content"] as? [[String: Any]] else { continue }
            let text = content.compactMap { $0["type"] as? String == "text" ? $0["text"] as? String : nil }.last ?? ""
            let flat = text.replacingOccurrences(of: "[`*#]", with: "", options: .regularExpression)
                .split(whereSeparator: \.isWhitespace).joined(separator: " ")
            if !flat.isEmpty { return flat }
        }
        return nil
    }

    /// Pfade (`file_path`/`path`/`notebook_path`) und Bash-Kommandos aus `tool_use`-Aufrufen, neueste zuerst
    /// (jüngste Zeile zuerst, innerhalb einer Zeile der letzte Aufruf zuerst). Sidechains (Subagenten) zählen nicht.
    static func toolCandidates(jsonl data: Data, limit: Int = 20) -> [String] {
        var out: [String] = []
        for line in data.split(separator: UInt8(ascii: "\n")).reversed() {
            guard let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
                  obj["type"] as? String == "assistant", obj["isSidechain"] as? Bool != true,
                  let content = (obj["message"] as? [String: Any])?["content"] as? [[String: Any]] else { continue }
            for item in content.reversed() {
                guard item["type"] as? String == "tool_use", let input = item["input"] as? [String: Any] else { continue }
                for key in ["file_path", "path", "notebook_path"] {
                    if let p = input[key] as? String, p.hasPrefix("/") { out.append(p) }
                }
                if let command = input["command"] as? String { out.append(command) }
                if out.count >= limit { return out }
            }
        }
        return out
    }
}
