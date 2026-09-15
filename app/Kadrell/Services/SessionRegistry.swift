import Foundation

/// Pollt `claude agents --json --all` alle 2 s, meldet nur echte Änderungen.
@MainActor
final class SessionRegistry {
    let cli: ClaudeCLI
    private(set) var sessions: [Session] = []
    var onChange: (([Session]) -> Void)?
    var onError: ((Error) -> Void)?
    private var task: Task<Void, Never>?

    init(cli: ClaudeCLI) { self.cli = cli }

    func start(interval: TimeInterval = 2) {
        task?.cancel()
        task = Task { [weak self] in
            while !Task.isCancelled {
                await self?.pollNow()
                try? await Task.sleep(for: .seconds(interval))
            }
        }
    }

    func stop() { task?.cancel() }

    func pollNow() async {
        do {
            // Nur Hintergrund-Sessions: interaktive laufen in fremden Terminals und interessieren hier nicht.
            let fresh = try await cli.agents().filter(\.isBackground)
            if fresh != sessions {
                sessions = fresh
                onChange?(fresh)
            }
        } catch {
            onError?(error)
        }
    }

    /// Pollt sofort wieder, bis eine Session das Prädikat erfüllt (maximal `timeout`).
    func waitFor(timeout: TimeInterval = 10, _ predicate: @escaping (Session) -> Bool) async -> Session? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            await pollNow()
            if let s = sessions.first(where: predicate) { return s }
            try? await Task.sleep(for: .milliseconds(500))
        }
        return nil
    }
}
