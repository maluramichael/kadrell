import Foundation
import os

/// Zustand und Steuerung der lokalen Provider für Einstellungen und Funktionen: welche Modelle wirklich installiert
/// sind, laufende Downloads, Suche auf ollama.com und die Anfrage einer Funktion an ihr Modell.
@Observable
@MainActor
final class LocalModels {
    static let shared = LocalModels()
    private static let log = Logger(subsystem: "de.malura.kadrell", category: "local-models")

    enum Status: Equatable {
        case off, checking
        case ready(String)
        case failed(String)
    }

    /// Ein laufender oder gescheiterter Download, sichtbar in den Einstellungen.
    struct Pull: Identifiable, Equatable {
        let ref: LocalModelRef
        var progress = PullProgress(status: "")
        var error: String?
        var id: String { ref.string }
    }

    private(set) var status: [LocalProviderKind: Status] = [:]
    private(set) var models: [LocalProviderKind: [LocalModel]] = [:]
    private(set) var pulls: [Pull] = []
    private(set) var searchHits: [OllamaSearchHit] = []
    private(set) var searching = false
    private(set) var searchError: String?
    /// Letzter Fehler beim Löschen, bis zur nächsten Aktion.
    private(set) var actionError: String?

    /// Login-Shell-Umgebung (PATH, `LLAMA_CACHE`, `HF_TOKEN`), setzt die App nach dem Start.
    @ObservationIgnored var environment = ProcessInfo.processInfo.environment
    @ObservationIgnored var session: URLSession = .shared
    @ObservationIgnored let server = LlamaServerProcess()
    @ObservationIgnored private var pullTasks: [String: Task<Void, Never>] = [:]

    /// Benutzbare Modelle aller eingeschalteten Provider, zur Auswahl je Funktion.
    var available: [LocalModel] {
        LocalProviderKind.allCases.filter(LocalModelSettings.enabled).flatMap { (models[$0] ?? []).filter { !$0.downloading } }
    }

    func status(_ p: LocalProviderKind) -> Status { status[p] ?? .off }
    func models(_ p: LocalProviderKind) -> [LocalModel] { models[p] ?? [] }

    // MARK: Provider

    func refreshAll() async {
        async let a: Void = refresh(.ollama)
        async let b: Void = refresh(.llamacpp)
        _ = await (a, b)
    }

    func refresh(_ p: LocalProviderKind) async {
        guard LocalModelSettings.enabled(p) else {
            status[p] = .off
            models[p] = []
            if p == .llamacpp { server.stop() }
            return
        }
        if models[p]?.isEmpty ?? true { status[p] = .checking }
        do {
            switch p {
            case .ollama:
                let c = try ollama()
                let version = try await c.version()
                models[p] = try await c.models()
                status[p] = .ready("Ollama \(version)")
            case .llamacpp:
                let c = try await llama()
                models[p] = try await c.models()
                status[p] = .ready(server.isRunning && LocalHTTP.baseURL(LocalModelSettings.llamaURL) == nil
                                   ? String(localized: "von Kadrell gestartet · \(c.base.absoluteString)", bundle: Bundle.app) : c.base.absoluteString)
            }
        } catch {
            status[p] = .failed(LocalModelError.describe(error))
            models[p] = []
        }
    }

    func ollama() throws -> OllamaClient {
        guard let url = LocalHTTP.baseURL(LocalModelSettings.ollamaURL) else {
            throw LocalModelError.invalid(String(localized: "Adresse ungültig", bundle: Bundle.app))
        }
        return OllamaClient(base: url, session: session)
    }

    /// Eingestellte Adresse eines laufenden Servers, sonst der eigene Kindprozess (bei Bedarf gestartet).
    func llama() async throws -> LlamaClient {
        if let url = LocalHTTP.baseURL(LocalModelSettings.llamaURL) { return LlamaClient(base: url, session: session) }
        let url = try await server.ensure(binary: llamaBinary, modelsDir: LocalModelSettings.llamaModelsDir, environment: environment)
        return LlamaClient(base: url, session: session)
    }

    var llamaBinary: String {
        let s = LocalModelSettings.llamaBinary.trimmingCharacters(in: .whitespacesAndNewlines)
        return s.isEmpty ? LlamaServerProcess.autodetect(environment: environment) : FolderIndex.normalize(s)
    }

    /// Kadrell endet: eigener `llama-server` mit, Downloads abbrechen (Ollama setzt sie beim nächsten Mal fort).
    func shutdown() {
        for t in pullTasks.values { t.cancel() }
        server.stop()
    }

    // MARK: Modelle laden und löschen

    /// Modell aus der Ollama-Bibliothek laden. llama.cpp lädt nicht über Kadrell (siehe `LlamaClient`).
    func pull(_ name: String) {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let ref = LocalModelRef(provider: .ollama, name: name)
        guard !name.isEmpty, pullTasks[ref.string] == nil else { return }
        pulls.removeAll { $0.ref == ref }
        pulls.append(Pull(ref: ref, progress: PullProgress(status: String(localized: "startet …", bundle: Bundle.app))))
        pullTasks[ref.string] = Task { [weak self] in
            await self?.runPull(ref)
        }
    }

    private func runPull(_ ref: LocalModelRef) async {
        let update: @Sendable (PullProgress) -> Void = { [weak self] p in
            Task { @MainActor in self?.setProgress(ref, p) }
        }
        do {
            try await ollama().pull(ref.name, progress: update)
            pulls.removeAll { $0.ref == ref }
        } catch is CancellationError {
            pulls.removeAll { $0.ref == ref }
        } catch let e as URLError where e.code == .cancelled {
            pulls.removeAll { $0.ref == ref }
        } catch {
            Self.log.error("Download \(ref.string, privacy: .public): \(String(describing: error), privacy: .public)")
            if let i = pulls.firstIndex(where: { $0.ref == ref }) { pulls[i].error = LocalModelError.describe(error) }
        }
        pullTasks[ref.string] = nil
        await refresh(ref.provider)
    }

    private func setProgress(_ ref: LocalModelRef, _ p: PullProgress) {
        guard pullTasks[ref.string] != nil, let i = pulls.firstIndex(where: { $0.ref == ref }) else { return }
        pulls[i].progress = p
    }

    /// Laufenden Download abbrechen bzw. eine Fehlermeldung wegnehmen.
    func cancelPull(_ ref: LocalModelRef) {
        pullTasks[ref.string]?.cancel()
        pullTasks[ref.string] = nil
        pulls.removeAll { $0.ref == ref }
    }

    func delete(_ m: LocalModel) async {
        actionError = nil
        do {
            switch m.provider {
            case .ollama: try await ollama().delete(m.name)
            case .llamacpp: try await llama().delete(m.name)
            }
        } catch {
            actionError = "\(m.name): \(LocalModelError.describe(error))"
        }
        // Gelöschtes Modell einer Funktion: Auswahl bleibt stehen, die Funktion fällt ohne Modell zurück.
        await refresh(m.provider)
    }

    // MARK: Suche

    func search(_ query: String) async {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty else { searchHits = []; searchError = nil; return }
        searching = true
        defer { searching = false }
        do {
            searchHits = try await OllamaSearch.search(q, session: session)
            searchError = searchHits.isEmpty ? String(localized: "nichts gefunden", bundle: Bundle.app) : nil
        } catch {
            searchHits = []
            searchError = LocalModelError.describe(error)
        }
    }

    // MARK: Funktionen

    /// Modell einer Funktion, wenn sie an ist und ihr Provider eingeschaltet. Ob das Modell noch installiert ist,
    /// zeigt erst die Anfrage: fehlt es, scheitert sie und die Funktion macht ohne Modell weiter.
    func activeModel(for f: LocalFeature) -> LocalModelRef? {
        guard LocalModelSettings.featureEnabled(f), let ref = LocalModelSettings.featureModel(f), LocalModelSettings.enabled(ref.provider) else { return nil }
        return ref
    }

    /// Modell der Funktion schon einmal laden, ohne zu warten.
    func warmUp(for f: LocalFeature) {
        guard let ref = activeModel(for: f) else { return }
        Task {
            switch ref.provider {
            case .ollama: await (try? ollama())?.preload(model: ref.name)
            case .llamacpp: await (try? await llama())?.load(ref.name)
            }
        }
    }

    /// Eine Antwort des Modells der Funktion, höchstens `f.timeout` Sekunden lang (Laden inklusive).
    func complete(for f: LocalFeature, system: String, user: String, schema: Data?) async throws -> String {
        guard let ref = activeModel(for: f) else { throw LocalModelError.invalid(String(localized: "kein Modell gewählt", bundle: Bundle.app)) }
        let timeout = f.timeout
        let ask: @Sendable () async throws -> String
        switch ref.provider {
        case .ollama:
            let c = try ollama()
            ask = { try await c.chat(model: ref.name, system: system, user: user, schema: schema, timeout: timeout) }
        case .llamacpp:
            let c = try await llama()
            ask = { try await c.chat(model: ref.name, system: system, user: user, schema: schema, timeout: timeout) }
        }
        return try await Self.withTimeout(timeout, ask)
    }

    /// Wer zuerst fertig ist: die Anfrage oder die Uhr. URLRequest kennt nur eine Leerlauf-Zeit, keine Gesamtzeit.
    nonisolated static func withTimeout<T: Sendable>(_ seconds: TimeInterval, _ work: @escaping @Sendable () async throws -> T) async throws -> T {
        try await withThrowingTaskGroup(of: T.self) { group in
            group.addTask { try await work() }
            group.addTask {
                try await Task.sleep(for: .seconds(seconds))
                throw LocalModelError.timeout
            }
            defer { group.cancelAll() }
            guard let first = try await group.next() else { throw LocalModelError.timeout }
            return first
        }
    }
}
