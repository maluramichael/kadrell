import Foundation

/// Kanboard über JSON-RPC (`<url>/jsonrpc.php`, Basic-Auth `jsonrpc:<token>`). Holt die offenen Tickets aller
/// Projekte und flacht sie zu einer Liste. Endpoint und Token kommen aus den Einstellungen bzw. dem Schlüsselbund.
struct KanboardProvider: TicketProvider {
    let endpoint: URL
    let token: String

    static let keychainService = "de.malura.kadrell.kanboard"
    static let keychainAccount = "token"

    func fetch() async throws -> [Ticket] {
        let projects = try await call("getAllProjects", params: [:], as: [KBProject].self)
        var tickets: [Ticket] = []
        for p in projects {
            let tasks = try await call("getAllTasks", params: ["project_id": p.id, "status_id": 1], as: [KBTask].self)
            for t in tasks {
                tickets.append(Ticket(id: String(t.id), title: t.title, description: t.description ?? "", url: t.url ?? "", project: p.name))
            }
        }
        return tickets
    }

    private func call<T: Decodable>(_ method: String, params: [String: Any], as: T.Type) async throws -> T {
        var req = URLRequest(url: endpoint)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("Basic " + Data("jsonrpc:\(token)".utf8).base64EncodedString(), forHTTPHeaderField: "Authorization")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["jsonrpc": "2.0", "id": 1, "method": method, "params": params])
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard let http = resp as? HTTPURLResponse, http.statusCode == 200 else {
            throw TicketError(String(localized: "Kanboard nicht erreichbar", bundle: Bundle.app))
        }
        let decoded = try JSONDecoder().decode(RPCResponse<T>.self, from: data)
        if let e = decoded.error { throw TicketError(e.message) }
        guard let r = decoded.result else { throw TicketError(String(localized: "Leere Antwort von Kanboard", bundle: Bundle.app)) }
        return r
    }

    private struct RPCResponse<T: Decodable>: Decodable { let result: T?; let error: RPCError? }
    private struct RPCError: Decodable { let message: String }
    private struct KBProject: Decodable { let id: Int; let name: String }
    private struct KBTask: Decodable { let id: Int; let title: String; let description: String?; let url: String? }
}
