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
    /// Hier und nur hier zählen die geschickten Nachrichten: der Zuwachs ist genau das, was seit dem letzten Poll
    /// dazugekommen ist. Ein Transcript zum ersten Mal zu lesen darf den alten Verlauf nicht nachträglich einrechnen.
    /// ponytail: lag die letzte Größe mitten in einer Zeile, geht diese eine Nachricht verloren.
    private static func grow(path: String, size: UInt64, from old: Entry, wantText: Bool) -> Entry {
        guard let h = FileHandle(forReadingAtPath: path) else { return readFull(path: path, size: size, wantText: wantText) }
        defer { try? h.close() }
        try? h.seek(toOffset: old.size)
        let data = try? h.readToEnd()
        if let data { Stats.bump(.messages, by: userMessages(jsonl: data)) }
        let newCandidates = data.map { toolCandidates(jsonl: $0) } ?? []
        return Entry(path: path, size: size, text: (wantText ? data.flatMap { lastText(jsonl: $0) } : nil) ?? old.text,
                     toolCandidates: newCandidates.isEmpty ? old.toolCandidates : Array((newCandidates + old.toolCandidates).prefix(20)))
    }

    private static func readFull(path: String, size: UInt64, wantText: Bool) -> Entry {
        let data = tailData(path: path)
        return Entry(path: path, size: size, text: wantText ? data.flatMap { lastText(jsonl: $0) } : nil,
                     toolCandidates: data.map { toolCandidates(jsonl: $0) } ?? [])
    }

    /// Text einer echten Eingabe des Nutzers, sonst nil. Tool-Ergebnisse kommen ebenfalls als `user`-Zeile, zählen
    /// aber nicht, ebenso wenig Meta-Zeilen, Sidechains (Subagenten) und die Ausgabe von Slash-Commands
    /// (`<command-name>` …). Eine angeschnittene Zeile ist kein JSON und fällt damit von selbst raus.
    static func userText(line: Data) -> String? {
        guard let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any],
              obj["type"] as? String == "user", obj["isMeta"] as? Bool != true, obj["isSidechain"] as? Bool != true,
              let content = (obj["message"] as? [String: Any])?["content"] else { return nil }
        let text = (content as? String)
            ?? (content as? [[String: Any]])?.compactMap { $0["type"] as? String == "text" ? $0["text"] as? String : nil }.joined(separator: " ")
            ?? ""
        let flat = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return flat.isEmpty || flat.hasPrefix("<") ? nil : flat
    }

    /// Zählt die Eingaben des Nutzers im gelesenen Ausschnitt (für `Stats`).
    static func userMessages(jsonl data: Data) -> Int {
        var n = 0
        for line in data.split(separator: UInt8(ascii: "\n")) where userText(line: line) != nil { n += 1 }
        return n
    }

    /// Erste echte Eingabe des Nutzers, gekürzt. Eine Nachricht aus lauter Bildern hat keinen Titel und zählt hier nicht.
    static func firstPrompt(path: String, head: Int = 1 << 18, limit: Int = 50) -> String? {
        guard let h = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? h.close() }
        guard let data = try? h.read(upToCount: head) else { return nil }
        for line in data.split(separator: UInt8(ascii: "\n")) {
            guard let text = userText(line: line) else { continue }
            if let flat = flatPrompt(text, limit: limit) { return flat }
        }
        return nil
    }

    /// Eine Nutzereingabe als Ersatztitel: Bildmarker raus, eine Zeile, gekürzt; Slash-Command-Ausgaben (`<…`) sind keiner.
    static func flatPrompt(_ text: String, limit: Int = 50) -> String? {
        let flat = text.replacingOccurrences(of: "\\[Image #\\d+\\]", with: "", options: .regularExpression)
            .split(whereSeparator: \.isWhitespace).joined(separator: " ")
        if flat.isEmpty || flat.hasPrefix("<") { return nil }
        return flat.count > limit ? flat.prefix(limit).trimmingCharacters(in: .whitespaces) + "…" : flat
    }

    /// Eine Antwort als Baumzeile: Markdown-Zeichen raus, eine Zeile.
    static func flatAnswer(_ text: String) -> String? {
        let flat = text.replacingOccurrences(of: "[`*#]", with: "", options: .regularExpression)
            .split(whereSeparator: \.isWhitespace).joined(separator: " ")
        return flat.isEmpty ? nil : flat
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
            if let flat = flatAnswer(content.compactMap { $0["type"] as? String == "text" ? $0["text"] as? String : nil }.last ?? "") { return flat }
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
