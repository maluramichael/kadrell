import Foundation

/// Letzte Textantwort von Claude aus dem Transcript `~/.claude/projects/<slug>/<sessionId>.jsonl`.
enum Transcript {
    struct Entry: Sendable {
        let path: String
        let size: UInt64
        let text: String?
    }

    static let root = NSHomeDirectory() + "/.claude/projects"

    /// Über alle Projektordner gesucht statt aus `cwd` abgeleitet: nach einem Worktree-Wechsel stimmt der Slug nicht mehr.
    static func path(sessionId: String) -> String? {
        let fm = FileManager.default
        guard !sessionId.isEmpty, let dirs = try? fm.contentsOfDirectory(atPath: root) else { return nil }
        return dirs.lazy.map { "\(root)/\($0)/\(sessionId).jsonl" }.first { fm.fileExists(atPath: $0) }
    }

    /// Liest nur Transcripts neu, deren Größe sich geändert hat. Schlüssel ist die `sessionId`.
    static func refresh(_ sessionIds: [String], cache: [String: Entry]) -> [String: Entry] {
        var out: [String: Entry] = [:]
        for id in sessionIds {
            guard let path = cache[id]?.path ?? path(sessionId: id) else { continue }
            let size = (try? FileManager.default.attributesOfItem(atPath: path)[.size] as? UInt64) ?? 0
            if let old = cache[id], old.size == size { out[id] = old; continue }
            out[id] = Entry(path: path, size: size, text: lastText(path: path))
        }
        return out
    }

    /// ponytail: nur das letzte MB, reicht solange keine Tool-Ausgabe allein größer ist; sonst ganze Datei lesen.
    static func lastText(path: String, tail: UInt64 = 1 << 20) -> String? {
        guard let h = FileHandle(forReadingAtPath: path) else { return nil }
        defer { try? h.close() }
        let size = (try? h.seekToEnd()) ?? 0
        try? h.seek(toOffset: size > tail ? size - tail : 0)
        guard let data = try? h.readToEnd() else { return nil }
        return lastText(jsonl: data)
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
}
