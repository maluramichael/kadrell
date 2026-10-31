import Foundation

/// Lokale Modell-Provider. Kadrell nutzt einen laufenden Ollama-Dienst (bringt ihn nicht mit) und startet
/// `llama-server` bei Bedarf selbst als Kindprozess, der mit Kadrell endet (siehe `LlamaServerProcess`).
enum LocalProviderKind: String, CaseIterable, Codable, Sendable {
    case ollama, llamacpp

    var title: String { self == .ollama ? "Ollama" : "llama.cpp" }
}

/// Verweis auf ein Modell eines Providers, gespeichert als `provider|name` (Ollama-Namen enthalten `:` und `/`).
struct LocalModelRef: Hashable, Sendable {
    let provider: LocalProviderKind
    let name: String

    init(provider: LocalProviderKind, name: String) {
        self.provider = provider
        self.name = name
    }

    init?(string: String) {
        let parts = string.split(separator: "|", maxSplits: 1).map(String.init)
        guard parts.count == 2, let p = LocalProviderKind(rawValue: parts[0]), !parts[1].isEmpty else { return nil }
        self.init(provider: p, name: parts[1])
    }

    var string: String { provider.rawValue + "|" + name }
    var display: String { "\(provider.title) · \(name)" }
}

/// Ein installiertes Modell, so wie der Provider es meldet. Nur was wirklich da ist, steht in den Listen.
struct LocalModel: Hashable, Sendable, Identifiable {
    let provider: LocalProviderKind
    let name: String
    var size: Int64?
    /// Kurzinfo rechts in der Zeile: „4.0B · Q4_K_M“ (Ollama) bzw. Status „loaded“ (llama.cpp).
    var detail = ""
    /// llama.cpp löscht nur Modelle aus seinem Cache, keine aus einem Modellordner oder Preset.
    var canRemove = true
    /// llama.cpp listet laufende Downloads mit: noch nicht benutzbar.
    var downloading = false

    var id: String { ref.string }
    var ref: LocalModelRef { LocalModelRef(provider: provider, name: name) }

    var sizeText: String { size.map { ByteCountFormatter.string(fromByteCount: $0, countStyle: .file) } ?? "" }
}

/// Fortschritt eines Downloads. `fraction` fehlt, solange der Provider noch keine Gesamtgröße kennt.
struct PullProgress: Equatable, Sendable {
    var status: String
    var completed: Int64 = 0
    var total: Int64 = 0

    var fraction: Double? { total > 0 ? min(1, Double(completed) / Double(total)) : nil }
}

/// Treffer der Modellsuche auf ollama.com.
struct OllamaSearchHit: Equatable, Sendable, Identifiable {
    let name: String
    var summary = ""
    /// Größen-Tags wie „0.6b“, „4b“: daraus wird `name:tag` zum Laden.
    var tags: [String] = []
    var id: String { name }
}

enum LocalModelError: LocalizedError, Equatable {
    case unreachable(String)
    case http(Int, String)
    case invalid(String)
    case timeout

    var errorDescription: String? {
        switch self {
        case .unreachable(let s): String(localized: "nicht erreichbar: \(s)", bundle: Bundle.app)
        case .http(let code, let s): s.isEmpty ? "HTTP \(code)" : "HTTP \(code): \(s)"
        case .invalid(let s): s
        case .timeout: String(localized: "keine Antwort in der Zeit", bundle: Bundle.app)
        }
    }

    /// Kurzer, lesbarer Text für die Einstellungen statt der langen NSURLError-Beschreibung.
    static func describe(_ error: Error) -> String {
        if let e = error as? LocalModelError { return e.errorDescription ?? "" }
        if let u = error as? URLError {
            switch u.code {
            case .timedOut: return LocalModelError.timeout.errorDescription ?? ""
            case .cannotConnectToHost, .cannotFindHost, .networkConnectionLost, .notConnectedToInternet:
                return LocalModelError.unreachable(u.failingURL?.host() ?? "").errorDescription ?? ""
            default: break
            }
        }
        return error.localizedDescription
    }
}

/// Kleiner JSON-Helfer über URLSession für beide Provider. Bodies kommen fertig als `Data`, Antworten als `Data`:
/// so kreuzt nichts Nicht-Sendables eine Isolationsgrenze.
enum LocalHTTP {
    static func request(_ url: URL, method: String = "GET", body: Data? = nil, timeout: TimeInterval) -> URLRequest {
        var r = URLRequest(url: url, timeoutInterval: timeout)
        r.httpMethod = method
        if let body {
            r.httpBody = body
            r.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        return r
    }

    /// Antwort-Body bei 2xx, sonst `LocalModelError.http` mit der Fehlermeldung aus dem Body (`{"error": …}`).
    static func data(_ session: URLSession, _ request: URLRequest) async throws -> Data {
        let (data, response) = try await session.data(for: request)
        try check(response, data)
        return data
    }

    static func check(_ response: URLResponse, _ data: Data) throws {
        guard let http = response as? HTTPURLResponse else { return }
        guard !(200..<300).contains(http.statusCode) else { return }
        throw LocalModelError.http(http.statusCode, errorMessage(data))
    }

    /// `{"error": "…"}` (Ollama) oder `{"error": {"message": "…"}}` (llama.cpp), sonst der Text selbst.
    static func errorMessage(_ data: Data) -> String {
        let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        if let s = obj?["error"] as? String { return s }
        if let e = obj?["error"] as? [String: Any], let s = e["message"] as? String { return s }
        return String(decoding: data.prefix(200), as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func json(_ object: [String: Any]) -> Data {
        (try? JSONSerialization.data(withJSONObject: object)) ?? Data()
    }

    /// Basis-URL aus der Einstellung: ohne Schema gilt http, Slash am Ende weg.
    static func baseURL(_ text: String) -> URL? {
        var s = text.trimmingCharacters(in: .whitespacesAndNewlines)
        while s.hasSuffix("/") { s.removeLast() }
        guard !s.isEmpty else { return nil }
        if !s.contains("://") { s = "http://" + s }
        guard let url = URL(string: s), url.host() != nil else { return nil }
        return url
    }

    /// Erstes JSON-Objekt in einer Modellantwort: kleine Modelle setzen gern Codezäune oder Text drumherum.
    static func firstJSONObject(in text: String) -> [String: Any]? {
        guard let start = text.firstIndex(of: "{"), let end = text.lastIndex(of: "}"), start < end else { return nil }
        return (try? JSONSerialization.jsonObject(with: Data(text[start...end].utf8))) as? [String: Any]
    }
}
