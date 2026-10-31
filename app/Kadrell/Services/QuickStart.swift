import Foundation

/// Ein Projekt, in dem der Schnellstart eine Session starten kann: Gruppe, Ordner früherer Sessions, benutzter
/// Ordner oder Git-Repo unter dem Projektordner, je Pfad einmal zusammengefasst.
struct QuickStartCandidate: Identifiable, Equatable, Sendable {
    enum Source: Int, Comparable, Sendable {
        case group, session, folder, repo
        static func < (a: Self, b: Self) -> Bool { a.rawValue < b.rawValue }
    }

    let path: String
    var name: String
    var groupId: String?
    var source: Source
    /// Weitere Namen, unter denen das Projekt im Text vorkommen kann (Gruppenname, Ordnername).
    var aliases: [String]
    var sessions = 0
    /// Letzte Nutzung (Session gestartet, Ordner benutzt), Sekunden seit 1970, 0 = unbekannt.
    var lastUsed: TimeInterval = 0
    /// Oft und kürzlich benutzt, aus `FolderIndex.frecency` plus Bonus für Gruppen und Sessions.
    var score = 0.0
    /// Titel der letzten Sessions dort, nur als Kontext fürs Modell.
    var titles: [String] = []
    var id: String { path }
}

/// Kandidaten aus der Historie, ohne Plattenzugriff außer dem Existenz-Check der Ordner.
enum QuickStartIndex {
    static func build(groups: [Group], sessions: [Session], closed: [ClosedSession] = [], uses: [String: FolderIndex.Use],
                      repos: [String], frecency: (String) -> Double, exists: (String) -> Bool = FolderIndex.isDirectory) -> [QuickStartCandidate] {
        var byPath: [String: QuickStartCandidate] = [:]
        var order: [String] = []
        func upsert(_ path: String, _ source: QuickStartCandidate.Source, _ change: (inout QuickStartCandidate) -> Void) {
            let p = FolderIndex.normalize(path)
            guard !p.isEmpty, p != "/" else { return }
            var c = byPath[p] ?? QuickStartCandidate(path: p, name: (p as NSString).lastPathComponent, source: source,
                                                      aliases: [(p as NSString).lastPathComponent])
            if byPath[p] == nil { order.append(p) }
            c.source = min(c.source, source)
            change(&c)
            byPath[p] = c
        }
        for g in groups where g.host == nil {
            upsert(g.cwd, .group) { c in
                c.name = g.name
                c.groupId = g.id
                if !c.aliases.contains(g.name) { c.aliases.insert(g.name, at: 0) }
            }
        }
        for s in sessions + closed.map(\.session) where !s.isRemote && !s.cwd.isEmpty {
            upsert(s.cwd, .session) { c in
                c.sessions += 1
                c.lastUsed = max(c.lastUsed, s.startedAt / 1000)
                if !s.isShell, c.titles.count < 3, !c.titles.contains(s.title) { c.titles.append(s.title) }
            }
        }
        for (p, u) in uses { upsert(p, .folder) { $0.lastUsed = max($0.lastUsed, u.last) } }
        for p in repos { upsert(p, .repo) { _ in } }
        return order.compactMap { byPath[$0] }
            .filter { $0.source == .group || exists($0.path) }
            .map { c in
                var c = c
                c.score = frecency(c.path) + (c.groupId != nil ? 0.5 : 0) + Double(c.sessions) * 0.1
                return c
            }
    }
}

/// Ergebnis der Namenssuche: alle Kandidaten in Reihenfolge, die besten zuerst.
struct QuickStartMatch: Equatable {
    var ranked: [QuickStartCandidate]
    /// Wie gut der erste Kandidat zum Text passt: 2 Name steht im Text, 1 Wortanfang passt, 0 kein Bezug.
    var topStrength = 0
    /// Genau ein Kandidat steht mit vollem Namen im Text: dann braucht es kein Modell zum Entscheiden.
    var confident = false
    var top: QuickStartCandidate? { ranked.first }
}

/// Einfache Zuordnung über Namen im Text, ohne Modell. „In RoboThomas habe ich …“ findet die Gruppe RoboThomas
/// und den Ordner `robo-thomas` (Bindestriche, Groß-/Kleinschreibung und Akzente zählen nicht).
enum QuickStartMatcher {
    /// Ordnernamen, die als Wort in fast jedem Satz vorkommen und deshalb nie allein ein Projekt erkennen lassen.
    static let stopwords: Set<String> = ["app", "apps", "api", "docs", "doc", "test", "tests", "web", "src", "code", "lib", "tmp",
                                         "der", "die", "das", "und", "the", "and", "for", "neu", "new", "idee", "idea", "main", "home",
                                         "projekt", "project", "projects", "projekte", "dev", "data", "site", "server", "client"]

    static func match(_ text: String, in candidates: [QuickStartCandidate]) -> QuickStartMatch {
        let words = self.words(text)
        let scored = candidates.map { c in (c, strength(words: words, c)) }
        let ranked = scored.sorted { a, b in
            if a.1 != b.1 { return a.1 > b.1 }
            if a.1 > 0, a.0.source != b.0.source { return a.0.source < b.0.source }
            if a.0.score != b.0.score { return a.0.score > b.0.score }
            if a.0.lastUsed != b.0.lastUsed { return a.0.lastUsed > b.0.lastUsed }
            return a.0.path.lowercased() < b.0.path.lowercased()
        }
        let top = ranked.first?.1 ?? 0
        let strong = ranked.filter { $0.1 == 2 }
        return QuickStartMatch(ranked: ranked.map(\.0), topStrength: top, confident: strong.count == 1)
    }

    /// 2: ein Name steht als Wort (oder als zusammengezogene Wortfolge, „robo thomas“) im Text.
    /// 1: ein Wort ab 4 Zeichen ist Anfang des Namens oder der Name Anfang des Worts („kadrells“).
    static func strength(words: [String], _ c: QuickStartCandidate) -> Int {
        let names = Set(c.aliases.map(normalize)).filter { $0.count >= 3 && !stopwords.contains($0) }
        guard !names.isEmpty, !words.isEmpty else { return 0 }
        let joined = joinedRuns(words, maxLength: 3)
        if names.contains(where: { joined.contains($0) }) { return 2 }
        let weak = words.contains { w in
            w.count >= 4 && names.contains { n in (n.hasPrefix(w) && w.count * 2 >= n.count) || (n.count >= 4 && w.hasPrefix(n)) }
        }
        return weak ? 1 : 0
    }

    /// Alle Folgen aus bis zu `maxLength` benachbarten Wörtern, zusammengezogen: „robo“, „thomas“, „robothomas“.
    static func joinedRuns(_ words: [String], maxLength: Int) -> Set<String> {
        var out = Set<String>()
        for i in words.indices {
            var s = ""
            for j in i..<min(words.count, i + maxLength) {
                s += words[j]
                out.insert(s)
            }
        }
        return out
    }

    /// Kleinbuchstaben, ohne Akzente, nur Buchstaben und Ziffern: „Robo-Thomas“ → „robothomas“.
    static func normalize(_ s: String) -> String {
        s.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil).lowercased().filter { $0.isLetter || $0.isNumber }
    }

    static func words(_ s: String) -> [String] {
        s.split { !($0.isLetter || $0.isNumber) }.map { normalize(String($0)) }.filter { !$0.isEmpty }
    }

    /// Bereinigten Prompt des Modells nur übernehmen, wenn er wirklich nur den Projektverweis los ist: Er ist das
    /// Original mit genau einem herausgeschnittenen Stück (vorn, hinten oder in der Mitte), Wort für Wort gleich,
    /// mindestens zwei Wörter lang, und im herausgeschnittenen Stück steht ein Name des Projekts. Sonst Originaltext.
    static func acceptCleaned(_ cleaned: String, original: String, candidate: QuickStartCandidate) -> Bool {
        let cw = words(cleaned), ow = words(original)
        guard cw.count >= 2, cw.count < ow.count else { return false }
        let prefix = zip(cw, ow).prefix { $0 == $1 }.count
        let suffix = zip(cw.reversed(), ow.reversed()).prefix { $0 == $1 }.count
        guard prefix + suffix >= cw.count else { return false }
        let head = min(prefix, cw.count), tail = cw.count - head
        let removed = joinedRuns(Array(ow[head..<(ow.count - tail)]), maxLength: 3)
        return candidate.aliases.map(normalize).contains { $0.count >= 3 && removed.contains($0) }
    }
}

/// Frage ans Modell: kurze Kandidatenliste, Antwort als JSON mit Index, Sicherheit und bereinigtem Prompt.
enum QuickStartPrompt {
    /// Mehr Kandidaten machen ein kleines Modell langsam und unsicher: die per Namen passenden plus die zuletzt benutzten.
    static let limit = 25

    static let system = """
    You map a user's idea to one of their software projects. Reply with JSON only:
    {"index": <number of the project from the list, or -1 if none fits>, "sure": <true only if the text clearly names or describes that project>, "prompt": "<the idea without the reference to the project>"}
    For "prompt": remove only the words that point to the project (e.g. "In RoboThomas I have the idea that the robot beeps" -> "the robot beeps", "Für Kadrell: Dark Mode einbauen" -> "Dark Mode einbauen"). Keep the language and everything else word for word. If unsure, return the idea unchanged.
    """

    static let schema: Data = LocalHTTP.json([
        "type": "object",
        "properties": ["index": ["type": "integer"], "sure": ["type": "boolean"], "prompt": ["type": "string"]],
        "required": ["index", "sure", "prompt"],
    ])

    static func user(idea: String, candidates: [QuickStartCandidate], nameMatch: Int?) -> String {
        let df = DateFormatter()
        df.dateFormat = "yyyy-MM-dd"
        let lines = candidates.enumerated().map { i, c in
            var parts = ["\(i): \(c.name)", Theme.shortPath(c.path)]
            if c.sessions > 0 { parts.append("\(c.sessions) sessions") }
            if c.lastUsed > 0 { parts.append("last used " + df.string(from: Date(timeIntervalSince1970: c.lastUsed))) }
            if !c.titles.isEmpty { parts.append("recent: " + c.titles.prefix(2).joined(separator: "; ")) }
            return parts.joined(separator: " | ")
        }
        var out = "Projects:\n" + lines.joined(separator: "\n")
        if let nameMatch { out += "\n\nName search suggests: \(nameMatch)" }
        return out + "\n\nIdea: " + idea
    }

    struct Answer: Equatable {
        var index: Int
        var sure: Bool
        var prompt: String
    }

    static func parse(_ text: String) -> Answer? {
        guard let obj = LocalHTTP.firstJSONObject(in: text), let index = (obj["index"] as? NSNumber)?.intValue else { return nil }
        return Answer(index: index, sure: (obj["sure"] as? Bool) ?? false, prompt: (obj["prompt"] as? String) ?? "")
    }
}
