import Foundation

/// Ein Ticket aus einem Ticket-Provider (Kanboard, später Jira). Nur was das Panel und die Prompt brauchen.
struct Ticket: Equatable, Identifiable, Sendable {
    let id: String
    let title: String
    let description: String
    let url: String
    /// Projektname, als zweite Zeile im Panel und zur Zuordnung des Arbeitsordners.
    let project: String

    /// Erste Nachricht der „implement“-Session: Titel, Beschreibung und Link, damit Claude im Projektordner loslegt.
    func implementPrompt() -> String {
        var s = "Implementiere das folgende Ticket.\n\nTitel: \(title)"
        if !description.isEmpty { s += "\n\nBeschreibung:\n\(description)" }
        if !url.isEmpty { s += "\n\nTicket: \(url)" }
        return s
    }
}

struct TicketError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}
