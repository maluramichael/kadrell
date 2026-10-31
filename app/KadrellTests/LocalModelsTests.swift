import XCTest
@testable import Kadrell

/// Gefälschter HTTP-Server für die Provider-Clients: jede Anfrage geht an `handler`, die Anfragen samt Body landen in `seen`.
final class MockHTTP: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var handler: (URLRequest, Data) -> (Int, Data) = { _, _ in (404, Data()) }
    nonisolated(unsafe) static var seen: [(request: URLRequest, body: Data)] = []

    static func session() -> URLSession {
        let c = URLSessionConfiguration.ephemeral
        c.protocolClasses = [MockHTTP.self]
        return URLSession(configuration: c)
    }

    static func reset(_ h: @escaping (URLRequest, Data) -> (Int, Data)) {
        handler = h
        seen = []
    }

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let body = Self.body(of: request)
        Self.seen.append((request, body))
        let (code, data) = Self.handler(request, body)
        client?.urlProtocol(self, didReceive: HTTPURLResponse(url: request.url!, statusCode: code, httpVersion: nil, headerFields: nil)!,
                            cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}

    /// URLSession legt den Body als Stream ab, nicht in `httpBody`.
    private static func body(of r: URLRequest) -> Data {
        if let d = r.httpBody { return d }
        guard let s = r.httpBodyStream else { return Data() }
        s.open()
        defer { s.close() }
        var out = Data(), buf = [UInt8](repeating: 0, count: 4096)
        while s.hasBytesAvailable {
            let n = s.read(&buf, maxLength: buf.count)
            if n <= 0 { break }
            out.append(buf, count: n)
        }
        return out
    }

    static func json(_ d: Data) -> [String: Any] { (try? JSONSerialization.jsonObject(with: d) as? [String: Any]) ?? [:] }
}

final class LocalModelsTests: XCTestCase {
    private let base = URL(string: "http://localhost:11434")!

    func testOllamaListsInstalledModels() async throws {
        MockHTTP.reset { r, _ in
            XCTAssertEqual(r.url?.path, "/api/tags")
            return (200, Data("""
            {"models":[{"name":"qwen3:4b","size":2500000000,"details":{"parameter_size":"4.0B","quantization_level":"Q4_K_M"}},
                       {"name":"gemma3:270m","size":291000000,"details":{}}]}
            """.utf8))
        }
        let models = try await OllamaClient(base: base, session: MockHTTP.session()).models()
        XCTAssertEqual(models.map(\.name), ["gemma3:270m", "qwen3:4b"], "nur was installiert ist, sortiert")
        XCTAssertEqual(models[1].detail, "4.0B · Q4_K_M")
        XCTAssertEqual(models[1].provider, .ollama)
    }

    func testOllamaPullStreamsProgressUntilSuccess() async throws {
        MockHTTP.reset { r, body in
            XCTAssertEqual(r.url?.path, "/api/pull")
            XCTAssertEqual(MockHTTP.json(body)["model"] as? String, "gemma3:270m")
            return (200, Data("""
            {"status":"pulling manifest"}
            {"status":"pulling abc","digest":"abc","total":1000,"completed":250}
            {"status":"pulling abc","digest":"abc","total":1000,"completed":1000}
            {"status":"success"}

            """.utf8))
        }
        let seen = Box<[PullProgress]>([])
        try await OllamaClient(base: base, session: MockHTTP.session()).pull("gemma3:270m") { seen.value.append($0) }
        XCTAssertEqual(seen.value.map(\.status), ["pulling manifest", "pulling abc", "pulling abc", "success"])
        XCTAssertEqual(seen.value[1].fraction, 0.25)
        XCTAssertNil(seen.value[0].fraction, "ohne Gesamtgröße kein Prozentwert")
    }

    func testOllamaPullErrorInStreamFails() async {
        MockHTTP.reset { _, _ in (200, Data("{\"status\":\"pulling manifest\"}\n{\"error\":\"pull model manifest: file does not exist\"}\n".utf8)) }
        do {
            try await OllamaClient(base: base, session: MockHTTP.session()).pull("gibtsnicht") { _ in }
            XCTFail("Fehler im Stream muss werfen")
        } catch {
            XCTAssertTrue(LocalModelError.describe(error).contains("does not exist"))
        }
    }

    func testOllamaDeleteSendsModelName() async throws {
        MockHTTP.reset { r, body in
            XCTAssertEqual(r.httpMethod, "DELETE")
            XCTAssertEqual(r.url?.path, "/api/delete")
            XCTAssertEqual(MockHTTP.json(body)["model"] as? String, "gemma3:270m")
            return (200, Data())
        }
        try await OllamaClient(base: base, session: MockHTTP.session()).delete("gemma3:270m")
        MockHTTP.reset { _, _ in (404, Data("{\"error\":\"model 'x' not found\"}".utf8)) }
        do { try await OllamaClient(base: base, session: MockHTTP.session()).delete("x"); XCTFail() } catch {
            XCTAssertEqual(error as? LocalModelError, .http(404, "model 'x' not found"))
        }
    }

    /// Schema als `format`, ohne Denken und ohne Stream; Modelle ohne Denkmodus lehnen `think` ab, dann einmal ohne.
    func testOllamaChatSendsSchemaAndRetriesWithoutThink() async throws {
        MockHTTP.reset { _, body in
            let j = MockHTTP.json(body)
            if j["think"] != nil { return (400, Data("{\"error\":\"\\\"gemma3:270m\\\" does not support thinking\"}".utf8)) }
            return (200, Data("{\"message\":{\"role\":\"assistant\",\"content\":\"{\\\"index\\\":1}\"}}".utf8))
        }
        let reply = try await OllamaClient(base: base, session: MockHTTP.session())
            .chat(model: "gemma3:270m", system: "s", user: "u", schema: QuickStartPrompt.schema, timeout: 5)
        XCTAssertEqual(reply, "{\"index\":1}")
        XCTAssertEqual(MockHTTP.seen.count, 2)
        let first = MockHTTP.json(MockHTTP.seen[0].body)
        XCTAssertEqual(first["think"] as? Bool, false)
        XCTAssertEqual(first["stream"] as? Bool, false)
        XCTAssertNotNil(first["format"] as? [String: Any])
        XCTAssertEqual((first["messages"] as? [[String: Any]])?.first?["role"] as? String, "system")
        XCTAssertNil(MockHTTP.json(MockHTTP.seen[1].body)["think"])
    }

    func testLlamaRouterModelsAndDelete() async throws {
        let llama = URL(string: "http://127.0.0.1:9999")!
        MockHTTP.reset { r, _ in
            if r.httpMethod == "DELETE" {
                XCTAssertEqual(r.url?.path, "/models")
                XCTAssertEqual(URLComponents(url: r.url!, resolvingAgainstBaseURL: false)?.queryItems?.first?.value, "ggml-org/gemma-3-1b-it-GGUF:Q4_K_M")
                return (200, Data("{\"success\":true}".utf8))
            }
            return (200, Data("""
            {"object":"list","data":[
              {"id":"ggml-org/gemma-3-1b-it-GGUF:Q4_K_M","status":{"value":"unloaded"},"can_remove":true},
              {"id":"local-model","status":{"value":"loaded","args":["llama-server"]},"can_remove":false},
              {"id":"neu/modell:Q4","status":{"value":"downloading","progress":{"https://a":{"done":50,"total":200},"https://b":{"done":50,"total":200}}}}]}
            """.utf8))
        }
        let client = LlamaClient(base: llama, session: MockHTTP.session())
        let models = try await client.models()
        XCTAssertEqual(models.map(\.name), ["ggml-org/gemma-3-1b-it-GGUF:Q4_K_M", "local-model", "neu/modell:Q4"])
        XCTAssertEqual(models.map(\.canRemove), [true, false, false])
        XCTAssertEqual(models[2].downloading, true)
        let entries = try await client.entries()
        XCTAssertEqual(LlamaClient.progress(entries[2])?.fraction, 0.25, "Dateien eines Downloads zusammengezählt")
        XCTAssertNil(LlamaClient.progress(entries[0]))
        try await client.delete("ggml-org/gemma-3-1b-it-GGUF:Q4_K_M")
    }

    func testLlamaChatUsesSchemaAndNoThinking() async throws {
        MockHTTP.reset { r, _ in
            XCTAssertEqual(r.url?.path, "/v1/chat/completions")
            return (200, Data("{\"choices\":[{\"message\":{\"content\":\"{\\\"index\\\":0}\"}}]}".utf8))
        }
        let reply = try await LlamaClient(base: URL(string: "http://127.0.0.1:9999")!, session: MockHTTP.session())
            .chat(model: "m", system: "s", user: "u", schema: QuickStartPrompt.schema, timeout: 5)
        XCTAssertEqual(reply, "{\"index\":0}")
        let body = MockHTTP.json(MockHTTP.seen[0].body)
        XCTAssertEqual(body["model"] as? String, "m")
        XCTAssertEqual((body["chat_template_kwargs"] as? [String: Any])?["enable_thinking"] as? Bool, false)
        XCTAssertEqual((body["response_format"] as? [String: Any])?["type"] as? String, "json_schema")
    }

    /// Ausschnitt aus dem echten Markup von ollama.com/search (Oktober 2026).
    func testSearchParsesNamesSummariesAndSizes() {
        let html = """
        <li class="x"><a href="/library/qwen3" class="group"><h2 title="qwen3"><span >qwen3</span></h2>
        <p class="mt-1 line-clamp-2">Qwen3 is the latest generation &amp; more.</p>
        <span class="inline-flex">tools</span><span class="inline-flex rounded-md bg-[#ddf4ff]">0.6b</span>
        <span class="inline-flex rounded-md bg-[#ddf4ff]">4b</span><span title="42,001,875 downloads"><span >42M</span></span></a></li>
        <li><a href="/library/gemma3"><p>Gemma</p><span class="inline-flex">270m</span></a></li>
        <a href="/library/qwen3">doppelt</a>
        """
        let hits = OllamaSearch.parse(html)
        XCTAssertEqual(hits.map(\.name), ["qwen3", "gemma3"])
        XCTAssertEqual(hits[0].summary, "Qwen3 is the latest generation & more.")
        XCTAssertEqual(hits[0].tags, ["0.6b", "4b"], "Download-Zahl „42M“ ist keine Größe")
        XCTAssertEqual(hits[1].tags, ["270m"])
    }

    func testRefsURLsAndJSONExtraction() {
        let ref = LocalModelRef(provider: .ollama, name: "hf.co/unsloth/Qwen3.5-9B-GGUF:Q8_0")
        XCTAssertEqual(LocalModelRef(string: ref.string), ref)
        XCTAssertNil(LocalModelRef(string: "ollama|"))
        XCTAssertNil(LocalModelRef(string: "foo|bar"))
        XCTAssertEqual(LocalHTTP.baseURL("localhost:11434/")?.absoluteString, "http://localhost:11434")
        XCTAssertNil(LocalHTTP.baseURL("  "))
        XCTAssertEqual(LocalHTTP.firstJSONObject(in: "```json\n{\"index\": 2}\n```")?["index"] as? Int, 2)
    }

    func testTimeoutWinsAgainstSlowWork() async {
        let start = Date()
        do {
            _ = try await LocalModels.withTimeout(0.2) { try await Task.sleep(for: .seconds(5)); return 1 }
            XCTFail("muss in die Zeitgrenze laufen")
        } catch {
            XCTAssertEqual(error as? LocalModelError, .timeout)
        }
        XCTAssertLessThan(Date().timeIntervalSince(start), 2)
    }

    func testFreePortAndAutodetect() {
        let p = LlamaServerProcess.freePort()
        XCTAssertNotNil(p)
        XCTAssertGreaterThan(p ?? 0, 1024)
        XCTAssertEqual(LlamaServerProcess.autodetect(environment: ["PATH": "/nirgends"]).isEmpty,
                       !FileManager.default.isExecutableFile(atPath: "/opt/homebrew/bin/llama-server")
                       && !FileManager.default.isExecutableFile(atPath: "/usr/local/bin/llama-server")
                       && !FileManager.default.isExecutableFile(atPath: NSHomeDirectory() + "/.local/bin/llama-server"))
    }
}

/// Sammelt Werte aus `@Sendable`-Rückrufen im Test.
final class Box<T>: @unchecked Sendable {
    var value: T
    init(_ v: T) { value = v }
}
