import Foundation

extension LocalModels {
    private static let ollamaApp = "/Applications/Ollama.app"

    /// Läuft Ollama (1 s Limit)? Sonst Ollama.app im Hintergrund starten und bis zu 10 s warten. Nur für ein Modell
    /// der Funktion bei Ollama auf dem eigenen Rechner. Fehlt die App oder kommt der Dienst nicht hoch, passiert nichts.
    func ensureOllamaRunning(for f: LocalFeature) async {
        guard activeModel(for: f)?.provider == .ollama, let client = try? ollama(), Self.isLocal(client.base) else { return }
        if await Self.reachable(client) { return }
        guard Self.launchOllamaApp() else { return }
        for _ in 0..<10 {
            try? await Task.sleep(for: .seconds(1))
            if await Self.reachable(client) { return }
        }
    }

    private static func reachable(_ client: OllamaClient) async -> Bool {
        (try? await withTimeout(1) { try await client.version() }) != nil
    }

    private static func isLocal(_ url: URL) -> Bool { ["localhost", "127.0.0.1", "::1"].contains(url.host ?? "") }

    private static func launchOllamaApp() -> Bool {
        guard FileManager.default.fileExists(atPath: ollamaApp) else { return false }
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        p.arguments = ["-g", "-a", ollamaApp]
        return (try? p.run()) != nil
    }
}
