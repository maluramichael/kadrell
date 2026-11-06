import AppKit

/// Lokaler Sessiontitel: die Statusmeldungen der Hooks (`controlStatus`) stoßen `SessionTitler` an.
extension AppDelegate {
    func makeSessionTitler() -> SessionTitler {
        let local = LocalModels.shared
        let titler = SessionTitler(
            isActive: { local.activeModel(for: .sessionTitle) != nil },
            ensureRunning: { await local.ensureOllamaRunning(for: .sessionTitle) },
            ask: { system, user in try await local.complete(for: .sessionTitle, system: system, user: user, schema: SessionTitlePrompt.schema) })
        titler.onTitle = { [weak self] id, title in self?.registry?.setLocalTitle(id, title) }
        return titler
    }
}
