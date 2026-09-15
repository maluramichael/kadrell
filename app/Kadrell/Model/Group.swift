import Foundation

struct GroupFrame: Codable, Equatable, Sendable {
    var x: CGFloat, y: CGFloat, w: CGFloat, h: CGFloat
    init(_ r: CGRect) { x = r.minX; y = r.minY; w = r.width; h = r.height }
    var rect: CGRect { CGRect(x: x, y: y, width: w, height: h) }
}

struct Group: Codable, Equatable, Identifiable, Sendable {
    var id: String
    var name: String
    var color: String
    var cwd: String
    var sessionIds: [String]
    /// Position und Größe in Weltpunkten (i3-artig frei verschiebbar). nil = noch nie platziert.
    var frame: GroupFrame?
}

/// Gruppen sind App-Daten, persistiert als JSON unter Application Support.
@MainActor
final class GroupStore {
    private(set) var groups: [Group] = []
    let url: URL

    static var defaultURL: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        return base.appendingPathComponent("de.malura.kadrell/groups.json")
    }

    init(url: URL = GroupStore.defaultURL) {
        self.url = url
        if let data = try? Data(contentsOf: url), let g = try? JSONDecoder().decode([Group].self, from: data) {
            groups = g
        }
    }

    func save() throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let enc = JSONEncoder()
        enc.outputFormatting = [.prettyPrinted, .sortedKeys]
        try enc.encode(groups).write(to: url, options: .atomic)
    }

    func group(id: String) -> Group? { groups.first { $0.id == id } }
    func group(forSession sessionId: String) -> Group? { groups.first { $0.sessionIds.contains(sessionId) } }
    func group(forCwd cwd: String) -> Group? { groups.first { $0.cwd == cwd } }

    func nextColor() -> String {
        let used = groups.map(\.color)
        return Theme.palette.first { !used.contains($0) } ?? Theme.palette[groups.count % Theme.palette.count]
    }

    func makeGroup(cwd: String, name: String? = nil) -> Group {
        let base = URL(fileURLWithPath: cwd).lastPathComponent
        return Group(id: UUID().uuidString.lowercased(), name: name ?? (base.isEmpty ? cwd : base),
                     color: nextColor(), cwd: cwd, sessionIds: [], frame: nil)
    }

    /// Ordnet Sessions ohne Gruppe der Gruppe mit gleichem `cwd` zu, legt sonst eine neue an,
    /// entfernt Ids, die es nicht mehr gibt, und leere Gruppen. Gibt zurück, ob sich etwas geändert hat.
    @discardableResult
    func assign(_ sessions: [Session]) -> Bool {
        let before = groups
        // Migration: früher stand die sessionId (UUID) in den Gruppen, jetzt die kurze Id.
        let byUUID = Dictionary(sessions.map { ($0.sessionId, $0.id) }, uniquingKeysWith: { a, _ in a })
        for i in groups.indices { groups[i].sessionIds = groups[i].sessionIds.map { byUUID[$0] ?? $0 } }
        let known = Set(sessions.map(\.id))
        for i in groups.indices { groups[i].sessionIds.removeAll { !known.contains($0) } }
        for s in sessions where group(forSession: s.id) == nil {
            if let idx = groups.firstIndex(where: { $0.cwd == s.cwd }) {
                groups[idx].sessionIds.append(s.id)
            } else {
                var g = makeGroup(cwd: s.cwd)
                g.sessionIds = [s.id]
                groups.append(g)
            }
        }
        // Leere Gruppen fliegen raus: ohne Sessions hat eine Gruppe keinen Zweck, und die Datei
        // sammelt sonst Ordner von längst beendeten Sessions.
        groups.removeAll { $0.sessionIds.isEmpty }
        placeUnplaced()
        let changed = groups != before
        if changed { try? save() }
        return changed
    }

    /// Gruppen ohne Position bekommen einen freien Platz neben den anderen.
    func placeUnplaced() {
        for i in groups.indices where groups[i].frame == nil {
            let existing = groups.compactMap { $0.frame?.rect }
            groups[i].frame = GroupFrame(Layout.placeNewGroup(existing: existing))
        }
    }

    func setFrame(_ rect: CGRect, for id: String) {
        guard let i = groups.firstIndex(where: { $0.id == id }) else { return }
        groups[i].frame = GroupFrame(rect)
        try? save()
    }

    func add(_ group: Group) { groups.append(group); placeUnplaced(); try? save() }
    func update(_ group: Group) {
        guard let i = groups.firstIndex(where: { $0.id == group.id }) else { return }
        groups[i] = group
        try? save()
    }
    func remove(id: String) { groups.removeAll { $0.id == id }; try? save() }
    func removeSession(_ sessionId: String) {
        for i in groups.indices { groups[i].sessionIds.removeAll { $0 == sessionId } }
        try? save()
    }
    func attach(sessionId: String, to groupId: String) {
        for i in groups.indices { groups[i].sessionIds.removeAll { $0 == sessionId } }
        guard let i = groups.firstIndex(where: { $0.id == groupId }) else { return }
        groups[i].sessionIds.append(sessionId)
        try? save()
    }
}
