import Foundation

/// HTTP-Client für `llama-server` im Router-Modus (ohne `-m` gestartet), Endpunkte laut `tools/server/README.md`
/// und `server-models.cpp` am Commit der installierten Version (0.6.0, d81235049): `GET /health`, `GET /models`
/// (Cache, `--models-dir`, Status je Modell), `POST /models/load`, `DELETE /models?model=` (nur Cache-Modelle,
/// `can_remove`), `POST /v1/chat/completions`. Herunterladen (`POST /models`) bietet Kadrell nicht an: der eigene
/// Server läuft mit `--offline` (siehe `LlamaServerProcess`), Modelle kommen per `llama-server -hf` in den Cache.
struct LlamaClient: Sendable {
    let base: URL
    var session: URLSession = .shared

    func healthy() async -> Bool {
        (try? await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "health"), timeout: 2))) != nil
    }

    struct Entry: Decodable {
        struct Progress: Decodable { let done: Int64?; let total: Int64? }
        struct Status: Decodable { let value: String?; let progress: [String: Progress]?; let failed: Bool? }
        let id: String
        let status: Status?
        let can_remove: Bool?
    }

    func entries() async throws -> [Entry] {
        let data = try await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "models"), timeout: 5))
        struct List: Decodable { let data: [Entry] }
        return try JSONDecoder().decode(List.self, from: data).data
    }

    func models() async throws -> [LocalModel] {
        try await entries().map(Self.model).sorted { $0.name.lowercased() < $1.name.lowercased() }
    }

    static func model(_ e: Entry) -> LocalModel {
        let state = e.status?.value ?? ""
        return LocalModel(provider: .llamacpp, name: e.id, detail: state, canRemove: e.can_remove ?? false, downloading: state == "downloading")
    }

    /// Modell vorab laden (`POST /models/load`), damit die erste Frage ohne Ladezeit antwortet. Geladen ist geladen:
    /// der Fehler „already loaded“ zählt nicht.
    func load(_ name: String) async {
        let body = LocalHTTP.json(["model": name])
        _ = try? await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "models/load"), method: "POST", body: body, timeout: 120))
    }

    func delete(_ name: String) async throws {
        var c = URLComponents(url: base.appending(path: "models"), resolvingAgainstBaseURL: false)!
        c.queryItems = [URLQueryItem(name: "model", value: name)]
        _ = try await LocalHTTP.data(session, LocalHTTP.request(c.url!, method: "DELETE", timeout: 30))
    }

    /// OpenAI-kompatibel; `response_format` mit Schema und `enable_thinking: false` wie in der README beschrieben.
    func chat(model: String, system: String, user: String, schema: Data?, timeout: TimeInterval) async throws -> String {
        var body: [String: Any] = [
            "model": model, "temperature": 0, "max_tokens": 400,
            "chat_template_kwargs": ["enable_thinking": false],
            "messages": [["role": "system", "content": system], ["role": "user", "content": user]],
        ]
        if let schema, let obj = try? JSONSerialization.jsonObject(with: schema) { body["response_format"] = ["type": "json_schema", "schema": obj] }
        let data = try await LocalHTTP.data(session, LocalHTTP.request(base.appending(path: "v1/chat/completions"), method: "POST",
                                                                       body: LocalHTTP.json(body), timeout: timeout))
        struct Message: Decodable { let content: String? }
        struct Choice: Decodable { let message: Message }
        struct Reply: Decodable { let choices: [Choice] }
        return try JSONDecoder().decode(Reply.self, from: data).choices.first?.message.content ?? ""
    }
}

/// `llama-server` als Kindprozess von Kadrell, nur wenn keine Adresse eines schon laufenden Servers eingestellt ist.
/// Router-Modus auf 127.0.0.1 und einem freien Port, ohne Web-Oberfläche, höchstens ein Modell gleichzeitig geladen,
/// `--offline`: ohne das prüft jede Modell-Instanz beim Laden erst Hugging Face und hing dabei im Test minutenlang
/// bei „initializing“; offline lädt dasselbe Cache-Modell in rund 2 s.
/// Endet mit Kadrell (`stop` aus `applicationWillTerminate`); die pid liegt im Profilordner, damit ein Start nach
/// einem Absturz den liegengebliebenen Server wegräumt statt ihn ewig mitlaufen zu lassen.
@MainActor
final class LlamaServerProcess {
    private var process: Process?
    private(set) var url: URL?
    private var key = ""
    private let pidFile: URL

    init(directory: URL = Profile.directory) {
        pidFile = directory.appendingPathComponent("llama-server.pid")
    }

    var isRunning: Bool { process?.isRunning == true }

    /// Läuft schon einer mit denselben Einstellungen, gilt der. Sonst neu starten und warten, bis `/health` antwortet.
    func ensure(binary: String, modelsDir: String, environment: [String: String]) async throws -> URL {
        let key = binary + "\n" + modelsDir
        if let url, isRunning, key == self.key { return url }
        stop()
        guard FileManager.default.isExecutableFile(atPath: binary) else {
            throw LocalModelError.invalid(String(localized: "\(binary): llama-server nicht gefunden", bundle: Bundle.app))
        }
        let port = Self.freePort() ?? 8091
        var args = ["--host", "127.0.0.1", "--port", String(port), "--no-webui", "--offline", "--models-max", "1"]
        if !modelsDir.isEmpty { args += ["--models-dir", FolderIndex.normalize(modelsDir)] }
        let p = try ProcessRunner.spawn(binary, args, environment: environment, discardOutput: true)
        process = p
        self.key = key
        try? String(p.processIdentifier).write(to: pidFile, atomically: true, encoding: .utf8)
        let url = URL(string: "http://127.0.0.1:\(port)")!
        let client = LlamaClient(base: url)
        for _ in 0..<60 {
            if await client.healthy() { self.url = url; return url }
            guard p.isRunning else { break }
            try await Task.sleep(for: .milliseconds(250))
        }
        stop()
        throw LocalModelError.invalid(String(localized: "llama-server startet nicht", bundle: Bundle.app))
    }

    func stop() {
        if let p = process, p.isRunning {
            ProcessRunner.killChildren(of: p.processIdentifier)   // Modell-Instanzen des Routers
            p.terminate()
        }
        process = nil
        url = nil
        try? FileManager.default.removeItem(at: pidFile)
    }

    /// Server eines früheren, abgestürzten Laufs desselben Profils beenden, aber nur, wenn die pid noch ein
    /// `llama-server` ist (pids werden wiederverwendet).
    func sweepStale() {
        guard let s = try? String(contentsOf: pidFile, encoding: .utf8), let pid = pid_t(s.trimmingCharacters(in: .whitespacesAndNewlines)), pid > 0 else { return }
        var buf = [CChar](repeating: 0, count: Int(MAXPATHLEN))
        if proc_pidpath(pid, &buf, UInt32(buf.count)) > 0, String(decoding: buf.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }, as: UTF8.self).hasSuffix("/llama-server") {
            ProcessRunner.killChildren(of: pid)
            kill(pid, SIGTERM)
        }
        try? FileManager.default.removeItem(at: pidFile)
    }

    /// Freier TCP-Port auf 127.0.0.1: Socket an Port 0 binden, zugeteilten Port lesen, wieder schließen.
    nonisolated static func freePort() -> Int? {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }
        defer { close(fd) }
        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        addr.sin_port = 0
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        let bound = withUnsafePointer(to: &addr) { $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { bind(fd, $0, len) } }
        guard bound == 0 else { return nil }
        let got = withUnsafeMutablePointer(to: &addr) { $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { getsockname(fd, $0, &len) } }
        guard got == 0 else { return nil }
        return Int(UInt16(bigEndian: addr.sin_port))
    }

    /// `llama-server` im PATH der Login-Shell, sonst an den üblichen Homebrew-Orten. "" = nicht gefunden.
    nonisolated static func autodetect(environment: [String: String]) -> String {
        let path = (environment["PATH"] ?? "").split(separator: ":").map(String.init)
        let dirs = path + ["/opt/homebrew/bin", "/usr/local/bin", NSHomeDirectory() + "/.local/bin"]
        return dirs.map { $0 + "/llama-server" }.first { FileManager.default.isExecutableFile(atPath: $0) } ?? ""
    }
}
