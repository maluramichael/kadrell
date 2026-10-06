import Foundation

/// Nachrichten einer Extension an Kadrell (eine JSON-Zeile, Unterscheidung über `t`).
enum ExtensionMessage: Equatable {
    case ready, pong(Int), panel(JSONValue?), status(JSONValue?), run(id: Int, argv: [String]), log(level: String, text: String)

    /// Kaputtes JSON, Binärmüll, unbekanntes `t` oder falsche Feldtypen ergeben `nil`; der Aufrufer verwirft die Zeile.
    static func decode(_ line: Data) -> ExtensionMessage? {
        guard let v = try? JSONDecoder().decode(JSONValue.self, from: line), let t = v["t"]?.string else { return nil }
        switch t {
        case "ready": return .ready
        case "pong": return v["id"]?.int.map { .pong($0) }
        case "panel": return .panel(v["tree"].flatMap(nonNull))
        case "status": return .status(v["item"].flatMap(nonNull))
        case "run": return decodeRun(v)
        case "log": return v["text"]?.string.map { .log(level: v["level"]?.string ?? "info", text: $0) }
        default: return nil
        }
    }

    private static func nonNull(_ v: JSONValue) -> JSONValue? { v == .null ? nil : v }

    private static func decodeRun(_ v: JSONValue) -> ExtensionMessage? {
        guard let id = v["id"]?.int, let items = v["argv"]?.array else { return nil }
        let argv = items.compactMap(\.string)
        return argv.count == items.count ? .run(id: id, argv: argv) : nil
    }
}

/// Nachrichten von Kadrell an eine Extension; `line()` ist genau eine Zeile für stdin.
enum HostMessage: Encodable {
    case hello(api: Int, name: String, dir: String, storageDir: String, config: [String: JSONValue], locale: String, sessions: [JSONValue])
    case event(name: String, data: JSONValue)
    case result(id: Int, status: Int32, stdout: String, stderr: String)
    case ping(Int)
    case shutdown

    private enum Key: String, CodingKey {
        case t, api, name, dir, storageDir, config, locale, sessions, data, id, status, stdout, stderr
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        switch self {
        case .hello(let api, let name, let dir, let storageDir, let config, let locale, let sessions):
            try c.encode("hello", forKey: .t)
            try c.encode(api, forKey: .api); try c.encode(name, forKey: .name); try c.encode(dir, forKey: .dir)
            try c.encode(storageDir, forKey: .storageDir); try c.encode(config, forKey: .config)
            try c.encode(locale, forKey: .locale); try c.encode(sessions, forKey: .sessions)
        case .event(let name, let data):
            try c.encode("event", forKey: .t); try c.encode(name, forKey: .name); try c.encode(data, forKey: .data)
        case .result(let id, let status, let stdout, let stderr):
            try c.encode("result", forKey: .t); try c.encode(id, forKey: .id); try c.encode(status, forKey: .status)
            try c.encode(stdout, forKey: .stdout); try c.encode(stderr, forKey: .stderr)
        case .ping(let id):
            try c.encode("ping", forKey: .t); try c.encode(id, forKey: .id)
        case .shutdown:
            try c.encode("shutdown", forKey: .t)
        }
    }

    func line() -> Data {
        let encoder = JSONEncoder()
        // Schrägstriche unmaskiert: kürzer, und json.lua liest beides.
        encoder.outputFormatting = [.withoutEscapingSlashes]
        var data = (try? encoder.encode(self)) ?? Data("{}".utf8)
        data.append(0x0A)
        return data
    }
}
