import AppKit

/// Lua-Extensions: Start, Steuerbefehle aus `kadrell.run` und Session-Events.
extension AppDelegate {
    /// Steuerbefehle laufen wie von außen (`caller: nil`), also ohne die Gruppen-Grenze für Sessions.
    func startExtensions(environment: [String: String]) {
        let manager = ExtensionManager(environment: environment, control: { [weak self] argv in
            await self?.runControl(ControlRequest(argv: argv, cwd: NSHomeDirectory(), caller: nil)) ?? .fail("Kadrell beendet sich")
        }, sessions: { [weak self] in
            guard let self else { return [] }
            return registry.sessions.map(extensionSession)
        })
        extensions = manager
        manager.start()
    }

    /// Session als Event-Daten; `state` wie in `kadrell ls --json`.
    func extensionSession(_ s: Session) -> JSONValue {
        .object(["key": .string(s.id), "title": .string(s.title), "cwd": .string(s.cwd), "branch": s.branch.map(JSONValue.string) ?? .null,
                 "sessionId": .string(s.sessionId), "state": .string(s.status.rawValue),
                 "group": store.group(forSession: s.id).map { .string($0.name) } ?? .null])
    }

    func emitSessionFocus(_ s: Session) {
        extensions?.emit("session.focus", .object(["session": extensionSession(s)]))
    }

    /// Vergleicht mit dem letzten Stand. Auch ohne Manager mitführen, sonst kämen beim Start alle Sessions als neu.
    func emitSessionEvents(_ sessions: [Session]) {
        let old = extensionSessions
        extensionSessions = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        guard let extensions else { return }
        for (event, s) in Self.sessionEvents(from: old, to: sessions) {
            var data = ["session": extensionSession(s)]
            if event == "session.status" { data["state"] = .string(s.status.rawValue) }
            extensions.emit(event, .object(data))
        }
    }

    /// `session.status` nur bei geändertem Status, nicht bei jedem neuen Titel oder Branch.
    static func sessionEvents(from old: [String: Session], to new: [Session]) -> [(event: String, session: Session)] {
        let keys = Set(new.map(\.id))
        let changed: [(event: String, session: Session)] = new.compactMap { s in
            guard let before = old[s.id] else { return ("session.new", s) }
            return before.status == s.status ? nil : ("session.status", s)
        }
        return changed + old.filter { !keys.contains($0.key) }.sorted { $0.key < $1.key }.map { ("session.remove", $0.value) }
    }
}
