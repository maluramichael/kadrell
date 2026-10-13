import Foundation

/// Agent-CLI einer Kachel (Harness). Claude ist der Default und hat die meisten Sonderfunktionen (Konten, Usage,
/// `--bg`-Übernahme, Transcript), alle anderen melden sich nur über ihre Integration per `kadrell status`.
/// Die Unterschiede sind Daten, deshalb switch-Properties statt einer Klasse je Typ (siehe `docs/agenten-plan.md`).
enum AgentKind: String, Codable, CaseIterable, Sendable {
    case claude, pi

    var displayName: String {
        switch self {
        case .claude: "Claude"
        case .pi: "Pi"
        }
    }

    /// Name des Binarys, aufgelöst wie bei Claude (`~/.local/bin/<name>`, sonst `command -v`).
    var binaryName: String { rawValue }

    /// Kürzel neben dem Titel im Baum, nur wenn mehrere Typen gleichzeitig dort stehen.
    var badge: String {
        switch self {
        case .claude: "C"
        case .pi: "π"
        }
    }

    /// Claude-Konten, Usage-Anzeige und Auto-Wechsel betreffen nur diesen Typ.
    var usesAccounts: Bool { self == .claude }
    /// Status und Titel auch per Poll (`<configDir>/sessions/<pid>.json`) und Transcript unter `~/.claude/projects`.
    /// Alle anderen Typen melden nur über ihre Integration.
    var usesClaudeData: Bool { self == .claude }

    /// Modell aus den Einstellungen, "" = Default der CLI.
    var configuredModel: String {
        switch self {
        case .claude: Settings.claudeModel
        case .pi: Settings.piModel
        }
    }

    /// Kompakte Zeile, mit welchen Start-Flags eine neue Session dieses Typs läuft (⌘N, Harness-Auswahl).
    var summary: String {
        switch self {
        case .claude: Settings.claudeSummary
        case .pi: Settings.piSummary
        }
    }

    /// Typabhängige Start-Flags aus den Einstellungen, ohne Session- und Prompt-Argumente.
    var launchArgs: [String] {
        switch self {
        case .claude:
            ClaudeCLI.launchArgs(allowBypass: Settings.claudeAllowBypass, mode: Settings.claudeMode, model: Settings.claudeModel, effort: Settings.claudeEffort)
                + ClaudeHook.launchArgs
        case .pi:
            PiCLI.launchArgs(model: Settings.piModel, thinking: Settings.piThinking)
        }
    }
}
