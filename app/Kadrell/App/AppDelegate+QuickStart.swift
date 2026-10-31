import AppKit

/// Schnellstart (⌘⇧I): Idee rein, Session raus. Kandidaten aus Gruppen, Sessions, benutzten Ordnern und Git-Repos;
/// gestartet wird wie `kadrell new -c <ordner> <prompt>` über `startSession(…, prompt:)`.
extension AppDelegate {
    func openQuickStart() {
        guard registry != nil else { NSSound.beep(); return }
        let local = LocalModels.shared
        let ref = local.activeModel(for: .quickStart)
        let ask: ((String, String) async throws -> String)? = ref.map { _ in
            { system, user in try await local.complete(for: .quickStart, system: system, user: user, schema: QuickStartPrompt.schema) }
        }
        let model = QuickStartModel(candidates: quickStartCandidates(), modelName: ref?.name, ask: ask)
        local.warmUp(for: .quickStart)
        // Repos neben bekannten Projekten nachladen wie bei ⌘N; bis der Scan steht, gilt der gespeicherte Stand.
        let known = store.groups.map(\.cwd) + FolderIndex.shared.uses.keys
        FolderIndex.shared.refresh(roots: [Settings.startFolder] + known.map { ($0 as NSString).deletingLastPathComponent }) { [weak self, weak model] in
            guard let self else { return }
            model?.updateCandidates(quickStartCandidates())
        }
        model.onClose = { [weak self] in self?.sheets.dismiss() }
        model.onStart = { [weak self] c, prompt in
            guard let self else { return }
            sheets.dismiss()
            startSession(group: c.groupId.flatMap { self.store.group(id: $0) }, cwd: c.path, prompt: prompt.isEmpty ? nil : prompt)
        }
        sheets.present(QuickStartView(model: model), onCancel: { model.cancel() }, onPrimary: { model.submit() })
    }

    private func quickStartCandidates() -> [QuickStartCandidate] {
        let index = FolderIndex.shared
        return QuickStartIndex.build(groups: store.groups, sessions: registry?.sessions ?? [], closed: registry?.closed ?? [],
                                     uses: index.uses, repos: index.repos, frecency: { index.frecency($0) })
    }
}
