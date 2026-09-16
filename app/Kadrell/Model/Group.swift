import Foundation

struct Group: Codable, Equatable, Identifiable, Sendable {
    var id: String
    var name: String
    var color: String
    var cwd: String
    var sessionIds: [String]
    /// Optional, damit eine groups.json ohne den Schlüssel weiter lädt.
    var favorite: Bool?

    var isFavorite: Bool { favorite == true }
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

    /// Name, den eine neu angelegte Gruppe für `cwd` ungefragt bekommt (auch zum Erkennen unveränderter Gruppen).
    static func defaultName(cwd: String) -> String {
        let base = URL(fileURLWithPath: cwd).lastPathComponent
        return base.isEmpty ? cwd : base
    }

    func makeGroup(cwd: String, name: String? = nil) -> Group {
        Group(id: UUID().uuidString.lowercased(), name: name ?? GroupStore.defaultName(cwd: cwd),
              color: nextColor(), cwd: cwd, sessionIds: [])
    }

    /// Gruppe hat weder umbenannten Namen noch eine von Hand gewählte Farbe, entspricht also noch
    /// dem, was `makeGroup` frisch vergeben hätte.
    private func isUnmodified(_ g: Group) -> Bool {
        g.name == GroupStore.defaultName(cwd: g.cwd) && Theme.palette.contains(g.color)
    }

    /// Ordnet Sessions ohne Gruppe der Gruppe mit gleichem `cwd` zu, legt sonst eine neue an,
    /// entfernt Ids, die es nicht mehr gibt, und unveränderte leere Gruppen. Gibt zurück, ob sich etwas geändert hat.
    @discardableResult
    func assign(_ sessions: [Session]) -> Bool {
        // Eine leere Liste ist eher ein Aussetzer (CLI-Update, Daemon kurz weg, umbenanntes Feld) als
        // "alle Sessions weg": nicht prunen, nichts speichern, sonst reißt ein einziger Fehlpoll alle Gruppen weg.
        guard !sessions.isEmpty else { return false }
        let before = groups
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
        // Leere Gruppen fliegen nur raus, wenn sie noch unverändert sind: ohne Sessions und ohne
        // eigenen Namen/eigene Farbe hat eine Gruppe keinen Zweck, und die Datei sammelt sonst Ordner
        // von längst beendeten Sessions. Favoriten und von Hand gepflegte Gruppen bleiben leer stehen,
        // damit die nächste Session desselben cwd wieder dort landet.
        groups.removeAll { $0.sessionIds.isEmpty && !$0.isFavorite && isUnmodified($0) }
        let changed = groups != before
        if changed { try? save() }
        return changed
    }

    func add(_ group: Group) { groups.append(group); try? save() }
    func update(_ group: Group) {
        guard let i = groups.firstIndex(where: { $0.id == group.id }) else { return }
        groups[i] = group
        try? save()
    }
    func remove(id: String) { groups.removeAll { $0.id == id }; try? save() }
    func toggleFavorite(id: String) {
        guard let i = groups.firstIndex(where: { $0.id == id }) else { return }
        groups[i].favorite = groups[i].isFavorite ? nil : true
        try? save()
    }
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

    /// Sortieren per Ziehen: die Session nimmt den Platz von `target` in derselben Gruppe ein.
    func moveSession(_ id: String, to target: String) {
        guard let g = groups.firstIndex(where: { $0.sessionIds.contains(id) }),
              let from = groups[g].sessionIds.firstIndex(of: id), let to = groups[g].sessionIds.firstIndex(of: target), from != to else { return }
        groups[g].sessionIds.insert(groups[g].sessionIds.remove(at: from), at: to)
        try? save()
    }

    /// Die Gruppe nimmt den Platz von `target` ein.
    func moveGroup(_ id: String, to target: String) {
        guard let from = groups.firstIndex(where: { $0.id == id }), let to = groups.firstIndex(where: { $0.id == target }), from != to else { return }
        groups.insert(groups.remove(at: from), at: to)
        try? save()
    }
}

/// Sortierung im Baum, umschaltbar in der Leiste. `off` zeigt die von Hand gezogene Reihenfolge.
enum SidebarSort: String, CaseIterable {
    case off, alpha, status

    var next: SidebarSort { Self.allCases[(Self.allCases.firstIndex(of: self)! + 1) % Self.allCases.count] }

    /// Wartet auf Antwort vor Fehler vor arbeitet vor fertig. Gleichstand behält die Handreihenfolge.
    private static func rank(_ s: SessionStatus) -> Int {
        switch s { case .waiting: 0; case .error: 1; case .running: 2; case .idle: 3 }
    }

    /// Sortiert Gruppen und die Sessions darin, ohne `groups.json` anzufassen.
    func apply(_ groups: [Group], sessions: [String: Session]) -> [Group] {
        guard self != .off else { return groups }
        func stable<T>(_ items: [T], _ less: (T, T) -> Bool) -> [T] {
            items.enumerated().sorted { less($0.1, $1.1) || (!less($1.1, $0.1) && $0.0 < $1.0) }.map(\.1)
        }
        let sorted = groups.map { g -> Group in
            var g = g
            g.sessionIds = stable(g.sessionIds) { a, b in
                guard let x = sessions[a], let y = sessions[b] else { return false }
                return self == .alpha ? x.title.localizedStandardCompare(y.title) == .orderedAscending
                                      : Self.rank(x.status) < Self.rank(y.status)
            }
            return g
        }
        let groupRank = { (g: Group) in g.sessionIds.compactMap { sessions[$0] }.map { Self.rank($0.status) }.min() ?? 4 }
        return stable(sorted) { a, b in
            self == .alpha ? a.name.localizedStandardCompare(b.name) == .orderedAscending : groupRank(a) < groupRank(b)
        }
    }
}
