import Foundation

/// HTTP-Client für einen laufenden Ollama-Dienst. Endpunkte laut API-Doku der installierten Version
/// (github.com/ollama/ollama, `docs/api.md` an v0.40.13): `/api/version`, `/api/tags`, `/api/pull` (Stream aus
/// JSON-Zeilen), `/api/delete`, `/api/chat` (`format` = JSON-Schema, `think`, `keep_alive`).
struct OllamaClient: Sendable {
    static let defaultURL = "http://localhost:11434"
    let base: URL
    var session: URLSession = .shared

    func version() async throws -> String {
        let data = try await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "api/version"), timeout: 3))
        struct V: Decodable { let version: String }
        return try JSONDecoder().decode(V.self, from: data).version
    }

    func models() async throws -> [LocalModel] {
        let data = try await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "api/tags"), timeout: 5))
        struct Details: Decodable { let parameter_size: String?; let quantization_level: String? }
        struct Item: Decodable { let name: String; let size: Int64?; let details: Details? }
        struct Tags: Decodable { let models: [Item] }
        return try JSONDecoder().decode(Tags.self, from: data).models.map { m in
            let detail = [m.details?.parameter_size, m.details?.quantization_level].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
            return LocalModel(provider: .ollama, name: m.name, size: m.size, detail: detail)
        }
        .sorted { $0.name.lowercased() < $1.name.lowercased() }
    }

    /// Lädt ein Modell aus der Ollama-Bibliothek. Der Dienst streamt eine JSON-Zeile je Fortschritt, die letzte hat
    /// `status: success`. Abbrechen (Task cancel) beendet nur das Mitlesen; Ollama setzt einen späteren Pull fort.
    func pull(_ name: String, progress: @escaping @Sendable (PullProgress) -> Void) async throws {
        let body = LocalHTTP.json(["model": name, "stream": true])
        let request = LocalHTTP.request(base.appending(path: "api/pull"), method: "POST", body: body, timeout: 600)
        let (bytes, response) = try await session.bytes(for: request)
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            var data = Data()
            for try await b in bytes { data.append(b) }
            throw LocalModelError.http(http.statusCode, LocalHTTP.errorMessage(data))
        }
        var done = false
        for try await line in bytes.lines {
            guard let p = try Self.parsePullLine(line) else { continue }
            progress(p)
            if p.status == "success" { done = true }
        }
        if !done { throw LocalModelError.invalid(String(localized: "Download ohne Abschluss beendet", bundle: Bundle.app)) }
    }

    /// Eine Zeile aus `/api/pull`. `{"error": …}` mitten im Stream (Modell gibt es nicht) wird zum Fehler.
    static func parsePullLine(_ line: String) throws -> PullProgress? {
        guard let obj = (try? JSONSerialization.jsonObject(with: Data(line.utf8))) as? [String: Any] else { return nil }
        if let e = obj["error"] as? String { throw LocalModelError.invalid(e) }
        guard let status = obj["status"] as? String else { return nil }
        let total = (obj["total"] as? NSNumber)?.int64Value ?? 0
        let completed = (obj["completed"] as? NSNumber)?.int64Value ?? 0
        return PullProgress(status: status, completed: completed, total: total)
    }

    func delete(_ name: String) async throws {
        let body = LocalHTTP.json(["model": name])
        _ = try await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "api/delete"), method: "DELETE", body: body, timeout: 30))
    }

    /// Eine Antwort ohne Stream. `think: false`: Denkmodelle (qwen3) antworten sonst erst nach langem Nachdenken.
    /// Modelle ohne Denkmodus lehnen `think` ab; dann einmal ohne.
    func chat(model: String, system: String, user: String, schema: Data?, timeout: TimeInterval) async throws -> String {
        do {
            return try await chatOnce(model: model, system: system, user: user, schema: schema, think: false, timeout: timeout)
        } catch LocalModelError.http(_, let message) where message.lowercased().contains("think") {
            return try await chatOnce(model: model, system: system, user: user, schema: schema, think: nil, timeout: timeout)
        }
    }

    private func chatOnce(model: String, system: String, user: String, schema: Data?, think: Bool?, timeout: TimeInterval) async throws -> String {
        var body: [String: Any] = [
            "model": model, "stream": false, "keep_alive": "10m",
            "options": ["temperature": 0],
            "messages": [["role": "system", "content": system], ["role": "user", "content": user]],
        ]
        if let schema, let format = try? JSONSerialization.jsonObject(with: schema) { body["format"] = format }
        if let think { body["think"] = think }
        let data = try await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "api/chat"), method: "POST",
                                                                       body: LocalHTTP.json(body), timeout: timeout))
        struct Message: Decodable { let content: String }
        struct Reply: Decodable { let message: Message }
        return try JSONDecoder().decode(Reply.self, from: data).message.content
    }

    /// Leere Nachrichtenliste lädt das Modell nur in den Speicher (Doku „Load a model“): beim Öffnen des
    /// Schnellstarts, damit die eigentliche Frage danach ohne Ladezeit antwortet.
    func preload(model: String) async {
        let body = LocalHTTP.json(["model": model, "messages": [], "keep_alive": "10m"])
        _ = try? await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "api/chat"), method: "POST", body: body, timeout: 60))
    }
}

/// Modellsuche über die Webseite ollama.com/search. Eine offizielle Such-Schnittstelle gibt es nicht (weder in der
/// API-Doku noch als `ollama search` in der CLI 0.40.13). Gelesen werden nur die Links `/library/<name>`, die
/// Beschreibung und die Größen-Tags; ändert die Seite ihr Markup, bleiben im schlimmsten Fall nur die Namen übrig.
/// Laden geht ohnehin immer über den eingetippten Namen (`/api/pull`), die Suche ist nur eine Hilfe.
enum OllamaSearch {
    static let page = URL(string: "https://ollama.com/search")!

    static func search(_ query: String, session: URLSession = .shared) async throws -> [OllamaSearchHit] {
        var c = URLComponents(url: page, resolvingAgainstBaseURL: false)!
        c.queryItems = [URLQueryItem(name: "q", value: query)]
        let data = try await LocalHTTP.data(session, LocalHTTP.request(c.url!, timeout: 10))
        return parse(String(decoding: data, as: UTF8.self))
    }

    /// Ein Treffer je `href="/library/<name>"`; Beschreibung (erstes `<p>`) und Tags wie `4b` aus demselben Block.
    static func parse(_ html: String) -> [OllamaSearchHit] {
        var hits: [OllamaSearchHit] = []
        let marker = "href=\"/library/"
        let blocks = html.components(separatedBy: marker).dropFirst()
        for block in blocks {
            guard let end = block.firstIndex(of: "\"") else { continue }
            let name = String(block[..<end])
            guard !name.isEmpty, name.range(of: "^[A-Za-z0-9._/-]+$", options: .regularExpression) != nil,
                  !hits.contains(where: { $0.name == name }) else { continue }
            let body = block.components(separatedBy: "</li>").first ?? block
            hits.append(OllamaSearchHit(name: name, summary: summary(body), tags: sizeTags(body)))
        }
        return hits
    }

    private static func summary(_ block: String) -> String {
        guard let p = block.range(of: "<p"), let open = block[p.upperBound...].firstIndex(of: ">"),
              let close = block.range(of: "</p>", range: open..<block.endIndex) else { return "" }
        return decodeEntities(String(block[block.index(after: open)..<close.lowerBound])).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Größen wie `0.6b`, `4b`, `270m`, `8x7b` als Text eines Tag-Spans mit `class`. Die Download-Zahl („42M“) steht
    /// in einem Span ohne Klasse und zählt nicht.
    private static func sizeTags(_ block: String) -> [String] {
        let pattern = "<span[^>]*class=\"[^\"]*\"[^>]*>\\s*([0-9]+(?:\\.[0-9]+)?(?:x[0-9]+)?[bm])\\s*</span>"
        guard let re = try? NSRegularExpression(pattern: pattern) else { return [] }
        let ns = block as NSString
        var out: [String] = []
        for m in re.matches(in: block, range: NSRange(location: 0, length: ns.length)) {
            let t = ns.substring(with: m.range(at: 1)).lowercased()
            if !out.contains(t) { out.append(t) }
        }
        return out
    }

    private static func decodeEntities(_ s: String) -> String {
        [("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&quot;", "\""), ("&#39;", "'"), ("&#x27;", "'")]
            .reduce(s) { $0.replacingOccurrences(of: $1.0, with: $1.1) }
    }
}
