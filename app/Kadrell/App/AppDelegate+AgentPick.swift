import AppKit

/// Agent-Auswahl vor dem Start: alle interaktiven Wege zu einer neuen Session (⌘N, ⌘⏎, Kontextmenüs, Baum,
/// Schnellstart) laufen hier durch. `kadrell new`, Terminal, Remote und Fortsetzen aus dem Verlauf rufen
/// `startSession` weiter direkt auf.
extension AppDelegate {
    /// Mit mehr als einem nutzbaren Agent erst die Auswahl, sonst startet Claude sofort wie bisher.
    func startNewSession(group: Group?, cwd: String, prompt: String? = nil) {
        // Bis `boot` Claude Code gefunden hat, gibt es keine Registry und damit auch keine Umgebung für `pi`.
        guard registry != nil else { NSSound.beep(); return }
        guard let pi = piSessionModel() else { startSession(group: group, cwd: cwd, prompt: prompt); return }
        let options = [AgentPickModel.Option(kind: .claude, detail: Settings.claudeSummary),
                       AgentPickModel.Option(kind: .pi, detail: piDetail(pi))]
        let model = AgentPickModel(folder: cwd, options: options, last: Settings.lastAgent)
        if model.list.first?.kind == .pi { LocalModels.shared.warmUp(for: .piSession) }
        model.onPick = { [weak self] kind in
            guard let self else { return }
            sheets.dismiss()
            Settings.lastAgent = kind
            startPicked(kind, group: group, cwd: cwd, prompt: prompt, pi: pi)
        }
        sheets.present(AgentPickView(model: model), onPrimary: { model.start() })
    }

    private func startPicked(_ kind: AgentKind, group: Group?, cwd: String, prompt: String?, pi: PiModel) {
        switch kind {
        case .claude: startSession(group: group, cwd: cwd, prompt: prompt)
        case .pi: startSession(group: group, cwd: cwd, prompt: prompt, sessionId: Session.newPiId(), model: pi.local)
        }
    }

    /// Zeile unter „Pi“: das lokale Modell oder Pis eigener Standard aus dessen Einstellungen.
    private func piDetail(_ pi: PiModel) -> String {
        if let ref = pi.local { return ref.name }
        return PiCLI.defaultModel() ?? String(localized: "Standard von Pi", bundle: Bundle.app)
    }
}
