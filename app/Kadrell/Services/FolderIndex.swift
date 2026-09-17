import AppKit

/// Ordner für ⌘N: benutzte Ordner mit Häufigkeit und letzter Nutzung, dazu Git-Repos unter dem Startordner
/// und neben bekannten Projekten. Der Scan läuft im Hintergrund und liegt zwischengespeichert, die Suche
/// geht nur gegen den Index, nie gegen die Platte.
@MainActor
final class FolderIndex {
    static let shared = FolderIndex()

    struct Use: Codable { var count: Int; var last: TimeInterval }

    private(set) var repos: [String] = []
    private(set) var uses: [String: Use] = [:]
    private var scanning = false
    private let reposURL: URL
    private let usesURL: URL
    /// Ungenutzte Einträge (Ordner gelöscht, Repo umbenannt) verfallen nach dieser Zeit, siehe `pruneUses`.
    private static let usesMaxAge: TimeInterval = 90 * 86400
    private static let usesLimit = 500

    init(directory: URL = SessionRegistry.defaultURL.deletingLastPathComponent()) {
        reposURL = directory.appendingPathComponent("folders.json")
        usesURL = directory.appendingPathComponent("folders-uses.json")
        if let d = try? Data(contentsOf: reposURL), let r = try? JSONDecoder().decode([String].self, from: d) { repos = r }
        uses = JSONFile.loadDict(Use.self, from: usesURL)
    }

    func recordUse(_ path: String) {
        let p = Self.normalize(path)
        var u = uses[p] ?? Use(count: 0, last: 0)
        u.count += 1
        u.last = Date().timeIntervalSince1970
        uses[p] = u
        pruneUses()
        JSONFile.saveDict(uses, to: usesURL)
    }

    /// Verworfene und umbenannte Ordner sammeln sich sonst für immer: älter als 90 Tage oder der Pfad existiert
    /// nicht mehr fliegt raus, danach bleiben höchstens `usesLimit` Einträge, die mit der besten Frecency zuerst.
    private func pruneUses() {
        let now = Date().timeIntervalSince1970
        uses = uses.filter { now - $0.value.last < Self.usesMaxAge && FileManager.default.fileExists(atPath: $0.key) }
        guard uses.count > Self.usesLimit else { return }
        let keep = uses.sorted { frecency($0.key, now: now) > frecency($1.key, now: now) }.prefix(Self.usesLimit)
        uses = Dictionary(uniqueKeysWithValues: keep.map { ($0.key, $0.value) })
    }

    /// Neu einlesen, ohne zu warten: bis der Scan fertig ist, gilt der gespeicherte Stand. `then` läuft danach.
    func refresh(roots: [String], then: (@MainActor () -> Void)? = nil) {
        guard !scanning else { return }
        scanning = true
        let roots = Array(Set(roots.map { Self.normalize($0) }))
        let url = reposURL
        Task.detached(priority: .utility) {
            let found = FolderIndex.scanRepos(roots: roots)
            try? FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            if let d = try? JSONEncoder().encode(found) { try? d.write(to: url, options: .atomic) }
            await MainActor.run {
                self.repos = found
                self.scanning = false
                then?()
            }
        }
    }

    /// „Oft und kürzlich“ wie bei zoxide: Nutzungen zählen, je älter die letzte, desto weniger.
    func frecency(_ path: String, now: TimeInterval = Date().timeIntervalSince1970) -> Double {
        guard let u = uses[path] else { return 0 }
        let age = now - u.last
        let weight: Double = age < 3600 ? 4 : age < 86_400 ? 2 : age < 604_800 ? 1 : 0.5
        return Double(u.count) * weight
    }

    // MARK: Scan

    /// Git-Repos bis Tiefe 4 unter den Wurzeln. In ein Repo wird nicht weiter hineingeschaut.
    /// ponytail: fester Tiefen- und Mengen-Deckel statt echter Ausschlussliste, reicht für Projektordner.
    nonisolated static func scanRepos(roots: [String], maxDepth: Int = 4, limit: Int = 5000) -> [String] {
        let skip: Set<String> = ["node_modules", "vendor", "build", "dist", "Library", "Applications", "Pictures", "Music", "Movies"]
        let fm = FileManager.default
        var out: [String] = []
        var queue = roots.map { ($0, 0) }
        var seen = Set<String>()
        while !queue.isEmpty, out.count < limit {
            let (dir, depth) = queue.removeFirst()
            guard seen.insert(dir).inserted else { continue }
            if fm.fileExists(atPath: dir + "/.git") { out.append(dir); continue }
            guard depth < maxDepth, let names = try? fm.contentsOfDirectory(atPath: dir) else { continue }
            for n in names where !n.hasPrefix(".") && !skip.contains(n) {
                let p = dir + "/" + n
                var isDir: ObjCBool = false
                if fm.fileExists(atPath: p, isDirectory: &isDir), isDir.boolValue { queue.append((p, depth + 1)) }
            }
        }
        return out.sorted()
    }

    // MARK: Suche

    /// Wortfetzen wie bei zoxide: jeder muss im Pfad vorkommen, der letzte im letzten Ordnernamen.
    /// Kleiner ist besser: 0 Präfix, 1 Teilstring, 2 Buchstabenfolge im Namen, 3 nur im Pfad. nil = kein Treffer.
    nonisolated static func rank(_ query: String, path: String) -> Int? {
        let words = query.lowercased().split(separator: " ").map(String.init)
        guard let last = words.last else { return 0 }
        let lower = path.lowercased()
        let name = (lower as NSString).lastPathComponent
        for w in words.dropLast() where !lower.contains(w) { return nil }
        if name.hasPrefix(last) { return 0 }
        if name.contains(last) { return 1 }
        if PaletteWindow.fuzzy(last, name) { return 2 }
        return words.count == 1 && PaletteWindow.fuzzy(last, lower) ? 3 : nil
    }

    /// `~/d/p/kad` → `~/development/projects/kadrell`: jedes Stück passt als Präfix (sonst Buchstabenfolge)
    /// auf einen Ordner. Ein exakter Name gewinnt, damit ausgeschriebene Pfade nicht aufblähen.
    nonisolated static func expandAbbreviated(_ typed: String, base: String = NSHomeDirectory(), limit: Int = 20) -> [String] {
        let path = normalize(typed, keepTrailingSlash: true)
        var parts = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        var starts: [String]
        if path.hasPrefix("/") { starts = ["/"]; parts.removeFirst() } else { starts = [base] }
        let trailing = parts.last == ""
        if trailing { parts.removeLast() }
        for (i, part) in parts.enumerated() {
            let isLast = i == parts.count - 1 && !trailing
            // Über alle Kandidaten gemeinsam: exakt vor Präfix, Buchstabenfolge nur, wenn nirgends ein Präfix passt.
            let q = part.lowercased()
            var exact: [String] = [], prefix: [String] = [], fuzzy: [String] = []
            for dir in starts {
                for n in subdirectories(of: dir).sorted(by: { $0.lowercased() < $1.lowercased() }) {
                    let l = n.lowercased()
                    if n == part { exact.append(join(dir, n)) } else if l.hasPrefix(q) { prefix.append(join(dir, n)) } else if isLast, PaletteWindow.fuzzy(q, l) { fuzzy.append(join(dir, n)) }
                }
            }
            starts = Array((!exact.isEmpty ? exact : !prefix.isEmpty ? prefix : fuzzy).prefix(limit))
            if starts.isEmpty { return [] }
        }
        guard trailing else { return starts }
        return Array(starts.flatMap { d in subdirectories(of: d).sorted { $0.lowercased() < $1.lowercased() }.map { join(d, $0) } }.prefix(limit))
    }

    nonisolated static func subdirectories(of dir: String) -> [String] {
        let fm = FileManager.default
        guard let names = try? fm.contentsOfDirectory(atPath: dir) else { return [] }
        return names.filter { n in
            var isDir: ObjCBool = false
            return !n.hasPrefix(".") && fm.fileExists(atPath: join(dir, n), isDirectory: &isDir) && isDir.boolValue
        }
    }

    nonisolated static func join(_ dir: String, _ name: String) -> String { dir.hasSuffix("/") ? dir + name : dir + "/" + name }

    /// `~` auflösen, Slashes am Ende weg (außer bei `/` selbst).
    nonisolated static func normalize(_ path: String, keepTrailingSlash: Bool = false) -> String {
        var p = path.trimmingCharacters(in: .whitespacesAndNewlines)
        if p == "~" || p.hasPrefix("~/") { p = NSHomeDirectory() + p.dropFirst() }
        if !keepTrailingSlash { while p.count > 1, p.hasSuffix("/") { p.removeLast() } }
        return p
    }

    nonisolated static func isDirectory(_ path: String) -> Bool {
        var isDir: ObjCBool = false
        return FileManager.default.fileExists(atPath: path, isDirectory: &isDir) && isDir.boolValue
    }

    // MARK: Kontext

    /// Pfad aus der Zwischenablage, wenn dort gerade ein existierender Ordner (oder eine Datei darin) liegt.
    static func clipboardFolder() -> String? {
        let pb = NSPasteboard.general
        if let url = (pb.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL])?.first {
            return folder(for: url.path)
        }
        guard let s = pb.string(forType: .string), !s.contains("\n"), s.count < 1024 else { return nil }
        return folder(for: normalize(s))
    }

    /// Ordner selbst, bei einer Datei ihr Ordner, sonst nil.
    nonisolated static func folder(for path: String) -> String? {
        guard path.hasPrefix("/"), FileManager.default.fileExists(atPath: path) else { return nil }
        return isDirectory(path) ? normalize(path) : (path as NSString).deletingLastPathComponent
    }

    /// Ordner des vordersten Finder-Fensters. Fragt beim ersten Mal nach der Erlaubnis, den Finder zu steuern.
    /// Über `osascript` statt NSAppleScript: das darf nur auf den Main-Thread, und die Rückfrage würde ihn blockieren.
    nonisolated static func finderFolder() async -> String? {
        await Task.detached(priority: .userInitiated) {
            let p = Process()
            p.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
            p.arguments = ["-e", "tell application \"Finder\" to if (count of Finder windows) > 0 then POSIX path of (target of front Finder window as alias)"]
            let out = Pipe()
            p.standardOutput = out
            p.standardError = FileHandle.nullDevice
            guard (try? p.run()) != nil else { return nil }
            let data = out.fileHandleForReading.readDataToEndOfFile()
            p.waitUntilExit()
            let r = String(decoding: data, as: UTF8.self)
            return p.terminationStatus == 0 ? folder(for: normalize(r)) : nil
        }.value
    }
}
