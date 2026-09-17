import Foundation
import os

/// `/download/latest.json`, wie `tools/release.sh` es neben jedem DMG ablegt.
struct UpdateManifest: Decodable, Equatable, Sendable {
    let version: String
    let url: String
    let sha256: String
    let notes: [String]
}

/// Prüft beim Start und danach alle 24 h ohne Tracking-Parameter, ob eine neuere Version bereitsteht.
@MainActor
final class UpdateChecker {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "update")
    private(set) var available: UpdateManifest?
    var onChange: ((UpdateManifest?) -> Void)?
    private var task: Task<Void, Never>?

    func start(interval: TimeInterval = 86400) {
        task?.cancel()
        task = Task { [weak self] in
            while !Task.isCancelled {
                await self?.checkNow()
                try? await Task.sleep(for: .seconds(interval))
            }
        }
    }

    func checkNow() async {
        guard Settings.checkForUpdates, let url = URL(string: "https://kadrell.malura.de/download/latest.json") else { return }
        do {
            let (data, _) = try await URLSession.shared.data(from: url)
            let m = try JSONDecoder().decode(UpdateManifest.self, from: data)
            let result = UpdateChecker.isNewer(m.version, than: Settings.version) ? m : nil
            guard result != available else { return }
            available = result
            onChange?(result)
        } catch {
            UpdateChecker.log.warning("update check: \(String(describing: error), privacy: .public)")
        }
    }

    /// `a` neuer als `b`, beide "x.y.z". Unbekanntes Format gilt als nicht neuer. `nonisolated`: reine Funktion, auch aus Tests ohne MainActor aufrufbar.
    nonisolated static func isNewer(_ a: String, than b: String) -> Bool {
        guard let va = ClaudeCLI.parseVersion(a), let vb = ClaudeCLI.parseVersion(b) else { return false }
        return vb < va
    }
}
