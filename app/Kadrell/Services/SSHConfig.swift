import Foundation

/// Hosts aus `~/.ssh/config` für Remote-Sessions. ssh selbst kann keine Hosts auflisten (`ssh -G` löst nur
/// einen auf), deshalb ein kleiner Parser, der `Include` verfolgt: Schlüsselwörter ohne Groß-/Kleinschreibung,
/// Pfade relativ zu `~/.ssh`, Globs wie `config.d/*`. Muster mit `*`, `?`, `!` sind keine Hosts.
enum SSHConfig {
    static var defaultConfig: URL { URL(fileURLWithPath: NSHomeDirectory() + "/.ssh/config") }
    private static let recentKey = "remote.recent"

    static func hosts(config: URL = defaultConfig) -> [String] {
        var seen = Set<String>(), out: [String] = [], visited = Set<String>()
        read(config, base: config.deletingLastPathComponent(), visited: &visited) { host in
            if seen.insert(host).inserted { out.append(host) }
        }
        return out
    }

    private static func read(_ file: URL, base: URL, visited: inout Set<String>, emit: (String) -> Void) {
        guard visited.insert(file.path).inserted, let text = try? String(contentsOf: file, encoding: .utf8) else { return }
        for raw in text.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: .whitespaces)
            guard !line.isEmpty, !line.hasPrefix("#") else { continue }
            let parts = line.split(whereSeparator: { $0 == " " || $0 == "\t" || $0 == "=" }).map(String.init)
            guard parts.count > 1 else { continue }
            switch parts[0].lowercased() {
            case "host":
                for h in parts.dropFirst() where h.rangeOfCharacter(from: CharacterSet(charactersIn: "*?!")) == nil { emit(h) }
            case "include":
                for pattern in parts.dropFirst() {
                    let expanded = (pattern as NSString).expandingTildeInPath
                    let abs = expanded.hasPrefix("/") ? expanded : base.appendingPathComponent(expanded).path
                    for path in glob(abs) { read(URL(fileURLWithPath: path), base: base, visited: &visited, emit: emit) }
                }
            default: continue
            }
        }
    }

    private static func glob(_ pattern: String) -> [String] {
        var g = glob_t()
        defer { globfree(&g) }
        guard Darwin.glob(pattern, 0, nil, &g) == 0 else { return [] }
        return (0..<Int(g.gl_pathc)).compactMap { g.gl_pathv[$0].map { String(cString: $0) } }.sorted()
    }

    /// Zuletzt verbundene Hosts, neueste vorn. Ersetzt eine Favoritenliste: was man nutzt, steht oben.
    static var recent: [String] { Profile.defaults.stringArray(forKey: recentKey) ?? [] }
    static func recordUse(_ host: String) {
        var r = recent.filter { $0 != host }
        r.insert(host, at: 0)
        Profile.defaults.set(Array(r.prefix(20)), forKey: recentKey)
    }

    /// Ein Wort für die Remote-Shell: in `'…'`, jedes `'` als `'\''`. tmux-Namen kommen aus Palette und Remote-Ausgabe.
    static func shellQuote(_ s: String) -> String { "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'" }

    /// tmux-Sessions auf dem Host. nil, wenn ssh scheitert; leer, wenn dort kein tmux-Server läuft.
    static func tmuxSessions(host: String, environment: [String: String]) async -> [String]? {
        await Task.detached(priority: .userInitiated) { () -> [String]? in
            let p = Process()
            p.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
            p.arguments = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=5", "--", host, "tmux list-sessions -F '#S' 2>/dev/null || true"]
            p.environment = environment
            let out = Pipe()
            p.standardOutput = out
            p.standardError = FileHandle.nullDevice
            do { try p.run() } catch { return nil }
            let data = out.fileHandleForReading.readDataToEndOfFile()
            p.waitUntilExit()
            guard p.terminationStatus == 0 else { return nil }
            return String(decoding: data, as: UTF8.self).split(separator: "\n").map(String.init).filter { !$0.isEmpty }
        }.value
    }
}
