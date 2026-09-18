import Foundation

/// Ein Ticket-Provider holt Tickets und baut die Prompt für eine neue Session. Eine Implementierung je System
/// (Kanboard, später Jira), umgeschaltet über die Einstellungen. Kein Fremdcode-Plugin, nur eingebaute Provider.
protocol TicketProvider: Sendable {
    func fetch() async throws -> [Ticket]
}
