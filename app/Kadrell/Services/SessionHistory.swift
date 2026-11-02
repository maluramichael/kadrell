import Foundation

/// Eine frühere Claude-Session aus `<configDir>/projects/<slug>/<sessionId>.jsonl`: Titel und eigene Prompts,
/// keine Antworten. Grundlage der Verlaufssuche (`#` in ⌘P), die auch beendete Sessions im alten Ordner fortsetzt.
struct HistorySession: Codable, Sendable, Equatable {
    var sessionId: String
    var path: String
    /// Ordner der ersten Eingabe: dort liegt das Transcript, und von dort aus findet `claude --resume` es.
    var cwd: String = ""
    var customTitle: String?
    var aiTitle: String?
    var prompts: [String] = []
    /// Zeitpunkt jeder Eingabe in Sekunden seit 1970, auch wiederholter, für die Statistik. Hier zählt jede Eingabe,
    /// `prompts` fasst gleiche direkt hintereinander zusammen.
    var promptTimes: [Double] = []
    /// Änderungszeit des Transcripts in Sekunden seit 1970.
    var lastActive: Double = 0
    /// Dateigröße beim letzten Lesen: gleich groß heißt unverändert, die Datei wird nicht geöffnet.
    var size: UInt64 = 0
    /// Bis hierhin gelesen, immer hinter einem Zeilenende. Ein gewachsenes Transcript liest der nächste Lauf ab hier.
    var parsed: UInt64 = 0

    var title: String { ownTitle ?? aiTitle ?? prompts.first.flatMap { Transcript.flatPrompt($0, limit: 80) } ?? sessionId }
    var project: String { (cwd as NSString).lastPathComponent }
    /// Viele Sessions tragen als eigenen Titel nur den Ordnernamen; der sagt nichts, dann gilt Claudes Titel.
    private var ownTitle: String? { customTitle.flatMap { $0.caseInsensitiveCompare(project) == .orderedSame ? nil : $0 } }
    /// Ohne eigene Eingabe und ohne Titel gibt es nichts zu finden (z. B. abgebrochener Start).
    var isSearchable: Bool { !prompts.isEmpty || customTitle != nil || aiTitle != nil }
}

/// Liest Transcripts zeilenweise, ohne jede Zeile als JSON zu parsen: nur Zeilen mit einer Eingabe oder einem Titel.
/// Tool-Ergebnisse (die großen Zeilen) fallen schon an der Bytesuche raus.
enum SessionHistoryParser {
    /// Längere Prompts (eingefügte Logs, Dateien) werden gekürzt, die Suche braucht den Anfang.
    static let promptLimit = 2000
    private static let userMarker = Array(#""type":"user""#.utf8)
    private static let toolResultMarker = Array(#""type":"tool_result""#.utf8)
    private static let customTitleMarker = Array(#""type":"custom-title""#.utf8)
    private static let aiTitleMarker = Array(#""type":"ai-title""#.utf8)

    /// Alle vollständigen Zeilen aus `data` in `entry` übernehmen. Ergebnis: verbrauchte Bytes bis hinter das letzte `\n`,
    /// eine angefangene letzte Zeile (Claude schreibt gerade) bleibt für den nächsten Lauf liegen.
    static func read(_ data: Data, into entry: inout HistorySession) -> Int {
        data.withUnsafeBytes { raw in
            guard let base = raw.baseAddress else { return 0 }
            var start = 0
            while start < raw.count, let nl = memchr(base + start, 0x0A, raw.count - start) {
                let end = base.distance(to: UnsafeRawPointer(nl))
                line(UnsafeRawBufferPointer(start: base + start, count: end - start), into: &entry)
                start = end + 1
            }
            return start
        }
    }

    private static func line(_ bytes: UnsafeRawBufferPointer, into entry: inout HistorySession) {
        if contains(bytes, userMarker) {
            if !contains(bytes, toolResultMarker) { user(bytes, into: &entry) }
        } else if contains(bytes, customTitleMarker) {
            entry.customTitle = object(bytes)?["customTitle"] as? String ?? entry.customTitle
        } else if contains(bytes, aiTitleMarker) {
            entry.aiTitle = object(bytes)?["aiTitle"] as? String ?? entry.aiTitle
        }
    }

    private static func user(_ bytes: UnsafeRawBufferPointer, into entry: inout HistorySession) {
        guard let obj = object(bytes) else { return }
        if entry.cwd.isEmpty, let cwd = obj["cwd"] as? String { entry.cwd = cwd }
        guard !isMachineWritten(obj), let text = Transcript.userText(object: obj), !text.hasPrefix(peerPrefix),
              let flat = Transcript.flatPrompt(text, limit: promptLimit) else { return }
        if let time = time(obj["timestamp"]) { entry.promptTimes.append(time) }
        if entry.prompts.last != flat { entry.prompts.append(flat) }
    }

    private static func time(_ value: Any?) -> Double? {
        guard let text = value as? String else { return nil }
        let style = Date.ISO8601FormatStyle(includingFractionalSeconds: true)
        return (try? style.parse(text))?.timeIntervalSince1970
    }

    /// Ältere Transcripts haben noch kein `origin`: Nachrichten anderer Sessions erkennt man dort nur am Anfang.
    private static let peerPrefix = "Another Claude session sent a message:"

    /// Steht als `user` im Transcript, hat aber keiner getippt: Task-Meldungen, Nachrichten anderer Sessions
    /// (`promptSource` „system“, `origin.kind` ≠ „human“) und die Zusammenfassung nach dem Verdichten des Kontexts.
    private static func isMachineWritten(_ obj: [String: Any]) -> Bool {
        if obj["isCompactSummary"] as? Bool == true || obj["promptSource"] as? String == "system" { return true }
        guard let kind = (obj["origin"] as? [String: Any])?["kind"] as? String else { return false }
        return kind != "human"
    }

    private static func object(_ bytes: UnsafeRawBufferPointer) -> [String: Any]? {
        try? JSONSerialization.jsonObject(with: Data(bytes)) as? [String: Any]
    }

    private static func contains(_ bytes: UnsafeRawBufferPointer, _ needle: [UInt8]) -> Bool {
        guard let base = bytes.baseAddress else { return false }
        return memmem(base, bytes.count, needle, needle.count) != nil
    }

    // MARK: Dateien

    struct File: Sendable { let path: String; let size: UInt64; let mtime: Double }

    /// Transcripts direkt unter `<root>/projects/<slug>/`, nur `<uuid>.jsonl`. Subagenten liegen eine Ebene tiefer,
    /// beiseitegelegte Fassungen (`.orphaned-…`) heißen anders, beides zählt nicht.
    static func files(root: String) -> [File] {
        let fm = FileManager.default, projects = root + "/projects"
        guard let dirs = try? fm.contentsOfDirectory(atPath: projects) else { return [] }
        return dirs.flatMap { dir -> [File] in
            let folder = projects + "/" + dir
            let names = (try? fm.contentsOfDirectory(atPath: folder)) ?? []
            return names.compactMap { name in
                guard name.hasSuffix(".jsonl"), UUID(uuidString: String(name.dropLast(6))) != nil else { return nil }
                let path = folder + "/" + name
                guard let a = try? fm.attributesOfItem(atPath: path) else { return nil }
                return File(path: path, size: (a[.size] as? UInt64) ?? 0, mtime: (a[.modificationDate] as? Date)?.timeIntervalSince1970 ?? 0)
            }
        }
    }

    /// Neu oder gewachsen: ab `parsed` weiterlesen. Geschrumpft (neu geschrieben): von vorn.
    /// Gemappt statt geladen, damit parallele Läufe über große Transcripts den Speicher nicht füllen.
    static func update(_ file: File, old: HistorySession?) -> HistorySession {
        let id = String((file.path as NSString).lastPathComponent.dropLast(6))
        var entry = old.flatMap { $0.parsed <= file.size ? $0 : nil } ?? HistorySession(sessionId: id, path: file.path)
        entry.size = file.size
        entry.lastActive = file.mtime
        guard entry.parsed < file.size,
              let data = try? Data(contentsOf: URL(fileURLWithPath: file.path), options: .alwaysMapped),
              Int(entry.parsed) <= data.count else { return entry }
        entry.parsed += UInt64(read(data[Int(entry.parsed)...], into: &entry))
        return entry
    }

    /// Ganzer Bestand: unveränderte Einträge bleiben, geänderte und neue werden parallel gelesen, gelöschte fallen weg.
    static func refresh(_ old: [String: HistorySession], root: String) async -> (entries: [String: HistorySession], changed: Bool) {
        let files = files(root: root)
        var out: [String: HistorySession] = [:]
        var todo: [File] = []
        for f in files {
            if var o = old[f.path], o.size == f.size { o.lastActive = f.mtime; out[f.path] = o } else { todo.append(f) }
        }
        let changed = !todo.isEmpty || out.count != old.count
        await withTaskGroup(of: HistorySession.self) { group in
            var next = todo.makeIterator()
            for _ in 0..<max(2, ProcessInfo.processInfo.activeProcessorCount) {
                guard let f = next.next() else { break }
                group.addTask { update(f, old: old[f.path]) }
            }
            for await e in group {
                out[e.path] = e
                if let f = next.next() { group.addTask { update(f, old: old[f.path]) } }
            }
        }
        return (out, changed)
    }
}

/// Treffer der Verlaufssuche; `snippet` ist der passende Prompt ab kurz vor der Fundstelle.
struct HistoryHit: Sendable {
    let session: HistorySession
    let snippet: String?
}

/// Durchsuchbarer Stand, neueste Session zuerst. Jedes Wort der Suche muss im Titel, im Ordner oder in einem Prompt
/// stehen (Groß-/Kleinschreibung und Akzente egal), Treffer im Titel rücken nach vorn. Ohne solchen Treffer gilt die
/// unscharfe Suche wie sonst in ⌘P, aber nur über Titel und Projekt, sonst passt jeder lange Prompt.
struct HistoryIndex: Sendable {
    let sessions: [HistorySession]
    /// Gefaltet und als Bytes, damit die Suche per `memmem` statt über `String` läuft.
    private let haystacks: [[UInt8]]
    private let titles: [[UInt8]]
    /// Titel und Projekt gefaltet als Zeichen, für die unscharfe Suche.
    private let labels: [[Character]]

    init(_ entries: [HistorySession]) {
        sessions = entries.filter(\.isSearchable).sorted { $0.lastActive > $1.lastActive }
        titles = sessions.map { Array(Self.fold($0.title).utf8) }
        labels = sessions.map { Array(Self.fold($0.title + " " + $0.project)) }
        haystacks = sessions.map { s in Array(Self.fold(([s.title, s.cwd] + s.prompts).joined(separator: "\n")).utf8) }
    }

    static func fold(_ s: String) -> String { s.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: nil) }

    func search(_ query: String, limit: Int = 200) -> [HistoryHit] {
        let words = query.split(whereSeparator: \.isWhitespace).map(String.init)
        guard !words.isEmpty else { return sessions.prefix(limit).map { HistoryHit(session: $0, snippet: nil) } }
        let needles = words.map { Array(Self.fold($0).utf8) }
        var ranked: [(index: Int, inTitle: Int)] = []
        for i in sessions.indices where needles.allSatisfy({ Self.contains(haystacks[i], $0) }) {
            ranked.append((i, needles.filter { Self.contains(titles[i], $0) }.count))
        }
        if ranked.isEmpty { return fuzzy(query, limit: limit) }
        ranked.sort { $0.inTitle != $1.inTitle ? $0.inTitle > $1.inTitle : $0.index < $1.index }
        let probe = words.max { $0.count < $1.count } ?? words[0]
        return ranked.prefix(limit).map { HistoryHit(session: sessions[$0.index], snippet: Self.snippet(sessions[$0.index], probe)) }
    }

    private func fuzzy(_ query: String, limit: Int) -> [HistoryHit] {
        let q = Array(Self.fold(query).filter { !$0.isWhitespace })
        guard q.count >= 3 else { return [] }
        return sessions.indices.lazy.filter { Self.tight(q, labels[$0]) }
            .prefix(limit).map { HistoryHit(session: sessions[$0], snippet: nil) }
    }

    /// Unscharf wie ⌘P (Buchstaben in Reihenfolge), aber eng beieinander: über höchstens die dreifache Länge der
    /// Suche verteilt. Sonst passt in langen Titeln fast jede Buchstabenfolge.
    static func tight(_ q: [Character], _ s: [Character]) -> Bool {
        for start in s.indices where s[start] == q[0] {
            var i = 1, j = start + 1
            let end = min(s.count, start + 3 * q.count)
            while i < q.count, j < end { if s[j] == q[i] { i += 1 }; j += 1 }
            if i == q.count { return true }
        }
        return false
    }

    /// Erster Prompt mit dem Wort, ab ein paar Zeichen davor; passt es nur im Titel, kein Ausschnitt.
    static func snippet(_ s: HistorySession, _ word: String) -> String? {
        for p in s.prompts {
            guard let r = p.range(of: word, options: [.caseInsensitive, .diacriticInsensitive]) else { continue }
            let start = p.index(r.lowerBound, offsetBy: -30, limitedBy: p.startIndex) ?? p.startIndex
            return (start > p.startIndex ? "…" : "") + p[start...].prefix(160)
        }
        return nil
    }

    private static func contains(_ hay: [UInt8], _ needle: [UInt8]) -> Bool {
        hay.withUnsafeBytes { h in memmem(h.baseAddress, h.count, needle, needle.count) != nil }
    }
}

/// Index aller Transcripts für die Verlaufssuche. Liegt zwischengespeichert in den Caches, beim Start und bei jedem
/// Öffnen der Suche werden nur neue und gewachsene Transcripts nachgelesen, die Suche selbst geht nur gegen den Speicher.
@MainActor
final class SessionHistory {
    static let shared = SessionHistory()

    /// nil bis der erste Lauf fertig ist (Cache geladen und nachgelesen bzw. beim allerersten Mal alles gelesen).
    private(set) var index: HistoryIndex?
    private var entries: [String: HistorySession] = [:]
    private var running = false
    private var waiters: [@MainActor (Bool) -> Void] = []

    /// Nachlesen im Hintergrund; `then(changed)` läuft danach, auch wenn schon ein Lauf unterwegs war.
    func refresh(configDir: String, then: (@MainActor (Bool) -> Void)? = nil) {
        if let then { waiters.append(then) }
        guard !running else { return }
        running = true
        let old = entries, url = Self.cacheURL(configDir: configDir), firstRun = index == nil
        Task.detached(priority: .utility) {
            let base = firstRun ? Self.loadCache(url) : old
            let (fresh, changed) = await SessionHistoryParser.refresh(base, root: configDir)
            if changed { Self.saveCache(fresh, url) }
            let next = changed || firstRun ? HistoryIndex(Array(fresh.values)) : nil
            await MainActor.run { self.finish(fresh, next) }
        }
    }

    /// Nachlesen und danach alle Sessions mit Eingaben oder Titel (für die Statistik).
    func sessions(configDir: String) async -> [HistorySession] {
        await withCheckedContinuation { done in
            refresh(configDir: configDir) { [weak self] _ in done.resume(returning: self?.index?.sessions ?? []) }
        }
    }

    private func finish(_ fresh: [String: HistorySession], _ next: HistoryIndex?) {
        entries = fresh
        if let next { index = next }
        running = false
        let done = waiters
        waiters = []
        done.forEach { $0(next != nil) }
    }

    // MARK: Cache

    /// Abgeleitet aus `~/.claude`, deshalb in den Caches und für alle Profile gemeinsam, je Claude-Datenordner eine Datei.
    nonisolated static func cacheURL(configDir: String) -> URL {
        let caches = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        let name = configDir.replacingOccurrences(of: "/", with: "_")
        return caches.appendingPathComponent("de.malura.kadrell/session-history-v2\(name).plist")
    }

    nonisolated private static func loadCache(_ url: URL) -> [String: HistorySession] {
        guard let d = try? Data(contentsOf: url), let list = try? PropertyListDecoder().decode([HistorySession].self, from: d) else { return [:] }
        return Dictionary(list.map { ($0.path, $0) }, uniquingKeysWith: { a, _ in a })
    }

    nonisolated private static func saveCache(_ entries: [String: HistorySession], _ url: URL) {
        let encoder = PropertyListEncoder()
        encoder.outputFormat = .binary
        guard let d = try? encoder.encode(Array(entries.values)) else { return }
        try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try? d.write(to: url, options: .atomic)
    }
}
