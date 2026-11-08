import Foundation

/// Prompt und Aufräumen für den lokalen Sessiontitel (`LocalFeature.sessionTitle`).
enum SessionTitlePrompt {
    static let maxLength = 60
    static let readmeLimit = 400

    /// Beispiele ohne Projektnamen: die Gruppe in der Seitenleiste zeigt das Projekt schon, und ein kleines Modell
    /// macht nach, womit die Beispiele anfangen. Sie sind erfunden, weil es sie sonst kopiert.
    static let system = """
    You name a coding session. Reply with JSON only: {"title": "<title>"}
    The title has 2 to 5 words and says WHAT the session is about (topic only), like a commit subject.
    The project is already shown next to the title: never start with or repeat the project or folder name.
    Never copy the prompt. Never write "check", "look", "work on" or the folder path. Use the language of the prompt.
    Examples: "Login timeout fix", "Newsletter-Versand", "CSV import".
    """

    static let schema: Data = LocalHTTP.json([
        "type": "object",
        "properties": ["title": ["type": "string"]],
        "required": ["title"],
    ])

    static func user(cwd: String, prompt: String, readme: String?) -> String {
        var out = "Project folder: \(URL(fileURLWithPath: cwd).lastPathComponent)\nPath: \(Theme.shortPath(cwd))\nPrompt: \(String(prompt.prefix(500)))"
        if let readme, !readme.isEmpty { out += "\n\nREADME start:\n" + readme }
        return out
    }

    static func parse(_ text: String) -> String? {
        guard let obj = LocalHTTP.firstJSONObject(in: text), let title = obj["title"] as? String else { return nil }
        return clean(title)
    }

    /// Eine Zeile, ohne Anführungszeichen und Schlusspunkt, höchstens `maxLength` Zeichen. Leer = Fehler (nil).
    static func clean(_ raw: String) -> String? {
        let line = raw.split(whereSeparator: \.isNewline).first.map(String.init) ?? ""
        let strip = CharacterSet.whitespaces.union(CharacterSet(charactersIn: "\"'`„“”‚‘’«»"))
        var t = line.trimmingCharacters(in: strip)
        while t.hasSuffix(".") { t.removeLast() }
        t = t.trimmingCharacters(in: strip)
        return t.isEmpty ? nil : String(t.prefix(maxLength))
    }

    /// Anfang der `README.md` im Projektordner, nil ohne Datei.
    static func readmeHead(cwd: String, limit: Int = readmeLimit) -> String? {
        guard let handle = FileHandle(forReadingAtPath: cwd + "/README.md") else { return nil }
        defer { try? handle.close() }
        guard let data = try? handle.read(upToCount: limit * 4), !data.isEmpty else { return nil }
        return String(String(decoding: data, as: UTF8.self).prefix(limit))
    }
}

/// Gibt neuen Claude-Sessions einen kurzen Titel von einem lokalen Modell. Alles ist still: scheitert irgendetwas,
/// bleibt es beim Titel von Claude Code bzw. beim ersten Prompt. Der Sessionstart wartet nie auf das Modell.
@MainActor
final class SessionTitler {
    typealias Ask = (String, String) async throws -> String

    private let isActive: () -> Bool
    private let ensureRunning: () async -> Void
    private let ask: Ask
    private let readme: @Sendable (String) -> String?
    private var warmed = Set<String>()
    private var attempted = Set<String>()
    /// Schlüssel der Session und fertiger Titel.
    var onTitle: (String, String) -> Void = { _, _ in }

    init(isActive: @escaping () -> Bool, ensureRunning: @escaping () async -> Void, ask: @escaping Ask,
         readme: @escaping @Sendable (String) -> String? = { SessionTitlePrompt.readmeHead(cwd: $0) }) {
        self.isActive = isActive
        self.ensureRunning = ensureRunning
        self.ask = ask
        self.readme = readme
    }

    /// Aus jeder Statusmeldung einer Session: vor dem ersten Prompt Modell bereitmachen, danach einmal nach dem Titel fragen.
    func note(_ s: Session) {
        guard eligible(s) else { return }
        if s.firstPrompt != nil {
            Task { await run(s) }
        } else if warmed.insert(s.id).inserted {
            Task {
                await ensureRunning()
                LocalModels.shared.warmUp(for: .sessionTitle)
            }
        }
    }

    private func eligible(_ s: Session) -> Bool {
        s.hasTranscript && s.customName == nil && s.localTitle == nil && isActive()
    }

    /// Fragt höchstens einmal je Session (bis zum nächsten Start von Kadrell), Fehler jeder Art bleiben unsichtbar.
    func run(_ s: Session) async {
        guard eligible(s), let prompt = s.firstPrompt, attempted.insert(s.id).inserted else { return }
        let cwd = s.cwd, readme = readme
        let head = await Task.detached { readme(cwd) }.value
        guard let text = try? await ask(SessionTitlePrompt.system, SessionTitlePrompt.user(cwd: cwd, prompt: prompt, readme: head)),
              let title = SessionTitlePrompt.parse(text) else { return }
        onTitle(s.id, title)
    }
}
