/// Programm, das in einer neuen Session läuft. Ein neuer Agent ist ein Fall mehr, die Auswahl beim Start
/// (`AgentPickView`) zeigt ihn von selbst.
enum AgentKind: String, Codable, CaseIterable, Sendable {
    case claude, pi

    /// Eigenname, bewusst nicht übersetzt.
    var displayName: String {
        switch self {
        case .claude: "Claude"
        case .pi: "Pi"
        }
    }
}
