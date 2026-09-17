import Foundation
import os

struct Group: Codable, Equatable, Identifiable, Sendable {
    var id: String
    var name: String
    var color: String
    var cwd: String
    var sessionIds: [String]
    /// Kein Optional mehr: der dreiwertige Bool war ein Bug (Toggle setzte `nil` statt `false`). Fehlt der
    /// Schlüssel in einer alten groups.json, gilt `false`.
    var favorite: Bool = false
    /// Remote-Gruppe: ssh-Host, alle Sessions darin hängen an dessen tmux. Fehlt in alten Dateien, dann lokal.
    var host: String? = nil

    var isFavorite: Bool { favorite }

    enum CodingKeys: String, CodingKey { case id, name, color, cwd, sessionIds, favorite, host }

    init(id: String, name: String, color: String, cwd: String, sessionIds: [String], favorite: Bool = false, host: String? = nil) {
        self.id = id
        self.name = name
        self.color = color
        self.cwd = cwd
        self.sessionIds = sessionIds
        self.favorite = favorite
        self.host = host
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try c.decode(String.self, forKey: .name)
        color = try c.decode(String.self, forKey: .color)
        cwd = try c.decode(String.self, forKey: .cwd)
        sessionIds = try c.decodeIfPresent([String].self, forKey: .sessionIds) ?? []
        favorite = try c.decodeIfPresent(Bool.self, forKey: .favorite) ?? false
        host = try c.decodeIfPresent(String.self, forKey: .host)
    }
}

/// Gruppen sind App-Daten, persistiert als JSON unter Application Support.
@MainActor
final class GroupStore {
    private(set) var groups: [Group] = []
    /// Letzter Lade- oder Speicherfehler (kaputte Datei, Schreibfehler). Kein Poll räumt es automatisch ab.
    private(set) var lastError: String?
    let url: URL
    private static let schemaVersion = 1
    private static let log = Logger(subsystem: "de.malura.kadrell", category: "groups")
    /// Umschlag nennt eine höhere Version als diese Kadrell-Version kennt: nur lesen, nie überschreiben.
    private var readOnly = false

    static var defaultURL: URL { Profile.directory.appendingPathComponent("groups.json") }

    init(url: URL = GroupStore.defaultURL) {
        self.url = url
        let result = JSONFile.loadArray(Group.self, from: url, currentVersion: Self.schemaVersion) { [weak self] msg in self?.lastError = msg }
        groups = result.items
        readOnly = result.newerThanKnown
        dedupeSessionsAcrossGroups()
    }

    /// Dieselbe sessionId darf in höchstens einer Gruppe stehen: eine kaputte Datei oder ein Bug beim Schreiben
    /// könnte sie doppelt eingetragen haben, `group(forSession:)` würde dann still die erste nehmen. Die erste
    /// Gruppe behält die Id, aus den anderen fliegt sie raus.
    private func dedupeSessionsAcrossGroups() {
        var seen = Set<String>()
        var changed = false
        for i in groups.indices {
            let before = groups[i].sessionIds.count
            groups[i].sessionIds.removeAll { !seen.insert($0).inserted }
            if groups[i].sessionIds.count != before { changed = true }
        }
        if changed {
            Self.log.warning("groups.json: doppelte sessionIds über mehrere Gruppen bereinigt")
            save()
        }
    }

    /// Loggt und meldet Schreibfehler (voller Volume, gesperrter Ordner), statt sie mit `try?` zu verschlucken.
    func save() {
        guard !readOnly else {
            Self.log.error("groups.json hat eine neuere Schema-Version, wird nicht überschrieben")
            return
        }
        JSONFile.saveArray(groups, to: url, version: Self.schemaVersion) { [weak self] msg in self?.lastError = msg }
    }

    func group(id: String) -> Group? { groups.first { $0.id == id } }
    func group(forSession sessionId: String) -> Group? { groups.first { $0.sessionIds.contains(sessionId) } }
    func group(forCwd cwd: String) -> Group? { groups.first { $0.cwd == cwd && $0.host == nil } }
    func group(forHost host: String) -> Group? { groups.first { $0.host == host } }

    func nextColor() -> String {
        let used = groups.map(\.color)
        return Theme.palette.first { !used.contains($0) } ?? Theme.palette[groups.count % Theme.palette.count]
    }

    /// Name, den eine neu angelegte Gruppe für `cwd` ungefragt bekommt (auch zum Erkennen unveränderter Gruppen).
    static func defaultName(cwd: String, host: String? = nil) -> String {
        if let host { return host }
        let base = URL(fileURLWithPath: cwd).lastPathComponent
        return base.isEmpty ? cwd : base
    }

    func makeGroup(cwd: String, name: String? = nil) -> Group {
        Group(id: UUID().uuidString.lowercased(), name: name ?? GroupStore.defaultName(cwd: cwd),
              color: nextColor(), cwd: cwd, sessionIds: [])
    }

    /// Gruppe für einen ssh-Host, heißt wie der Host; `cwd` ist nur Platzhalter für lokale Aktionen.
    func makeGroup(host: String) -> Group {
        Group(id: UUID().uuidString.lowercased(), name: host, color: nextColor(), cwd: NSHomeDirectory(), sessionIds: [], host: host)
    }

    /// Gruppe hat weder umbenannten Namen noch eine von Hand gewählte Farbe, entspricht also noch
    /// dem, was `makeGroup` frisch vergeben hätte.
    private func isUnmodified(_ g: Group) -> Bool {
        g.name == GroupStore.defaultName(cwd: g.cwd, host: g.host) && Theme.palette.contains(g.color)
    }

    /// Ordnet Sessions ohne Gruppe der Gruppe mit gleichem `cwd` zu, legt sonst eine neue an, und räumt
    /// unveränderte leere Gruppen weg. Gibt zurück, ob sich etwas geändert hat.
    ///
    /// Prunt absichtlich keine `sessionIds` mehr gegen `sessions`: das übernehmen `removeSession`/`remove(id:)`
    /// beim echten Schließen. Ein Poll mit einer unvollständigen Liste (Ladefehler, Teilverlust von
    /// sessions.json) hätte sonst ganze Gruppen leergeräumt, siehe #747 und #775.
    @discardableResult
    func assign(_ sessions: [Session]) -> Bool {
        // Eine leere Liste ist eher ein Aussetzer (CLI-Update, Daemon kurz weg, umbenanntes Feld) als
        // "alle Sessions weg": nichts speichern, sonst reißt ein einziger Fehlpoll alle Gruppen weg.
        guard !sessions.isEmpty else { return false }
        let before = groups
        for s in sessions where group(forSession: s.id) == nil {
            if let idx = groups.firstIndex(where: { s.host != nil ? $0.host == s.host : $0.cwd == s.cwd && $0.host == nil }) {
                groups[idx].sessionIds.append(s.id)
            } else {
                var g = s.host.map { makeGroup(host: $0) } ?? makeGroup(cwd: s.cwd)
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
        if changed { save() }
        return changed
    }

    func add(_ group: Group) { groups.append(group); save() }
    func update(_ group: Group) {
        guard let i = groups.firstIndex(where: { $0.id == group.id }) else { return }
        groups[i] = group
        save()
    }
    func remove(id: String) { groups.removeAll { $0.id == id }; save() }
    func toggleFavorite(id: String) {
        guard let i = groups.firstIndex(where: { $0.id == id }) else { return }
        groups[i].favorite.toggle()
        save()
    }
    func removeSession(_ sessionId: String) {
        for i in groups.indices { groups[i].sessionIds.removeAll { $0 == sessionId } }
        save()
    }
    func attach(sessionId: String, to groupId: String) {
        for i in groups.indices { groups[i].sessionIds.removeAll { $0 == sessionId } }
        guard let i = groups.firstIndex(where: { $0.id == groupId }) else { return }
        groups[i].sessionIds.append(sessionId)
        save()
    }

    /// Sortieren per Ziehen: die Session nimmt den Platz von `target` in derselben Gruppe ein.
    func moveSession(_ id: String, to target: String) {
        guard let g = groups.firstIndex(where: { $0.sessionIds.contains(id) }),
              let from = groups[g].sessionIds.firstIndex(of: id), let to = groups[g].sessionIds.firstIndex(of: target), from != to else { return }
        groups[g].sessionIds.insert(groups[g].sessionIds.remove(at: from), at: to)
        save()
    }

    /// Die Gruppe nimmt den Platz von `target` ein.
    func moveGroup(_ id: String, to target: String) {
        guard let from = groups.firstIndex(where: { $0.id == id }), let to = groups.firstIndex(where: { $0.id == target }), from != to else { return }
        groups.insert(groups.remove(at: from), at: to)
        save()
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
