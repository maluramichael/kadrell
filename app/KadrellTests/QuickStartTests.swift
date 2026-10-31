import XCTest
@testable import Kadrell

@MainActor
final class QuickStartTests: XCTestCase {
    private func candidate(_ path: String, _ name: String? = nil, source: QuickStartCandidate.Source = .folder,
                           score: Double = 0, groupId: String? = nil) -> QuickStartCandidate {
        let base = (path as NSString).lastPathComponent
        return QuickStartCandidate(path: path, name: name ?? base, groupId: groupId, source: source,
                                   aliases: name.map { [$0, base] } ?? [base], score: score)
    }

    private lazy var projects = [
        candidate("/p/kadrell", "Kadrell", source: .group, score: 5, groupId: "g1"),
        candidate("/p/robo-thomas", source: .repo, score: 1),
        candidate("/p/bloom", "Bloom", source: .group, score: 3, groupId: "g2"),
        candidate("/p/app", source: .repo, score: 9),
    ]

    func testIndexMergesGroupsSessionsUsesAndRepos() {
        let g = Group(id: "g1", name: "Kadrell", color: "#fff", cwd: "/p/kadrell", sessionIds: ["s1"])
        let remote = Group(id: "g9", name: "pc", color: "#fff", cwd: "~", sessionIds: [], host: "pc")
        let s = Session(id: "s1", cwd: "/p/kadrell", startedAt: 1_700_000_000_000, sessionId: "s1", name: "Statusleiste")
        let gone = Session(id: "s2", cwd: "/p/weg", startedAt: 1, sessionId: "s2", name: "")
        let index = QuickStartIndex.build(groups: [g, remote], sessions: [s, gone], uses: ["/p/bloom": FolderIndex.Use(count: 3, last: 10)],
                                          repos: ["/p/kadrell", "/p/robo-thomas"], frecency: { $0 == "/p/bloom" ? 2 : 0 },
                                          exists: { $0 != "/p/weg" })
        XCTAssertEqual(index.map(\.path), ["/p/kadrell", "/p/bloom", "/p/robo-thomas"], "je Pfad einmal, Remote und verschwundene Ordner nicht")
        let k = index[0]
        XCTAssertEqual(k.name, "Kadrell")
        XCTAssertEqual(k.groupId, "g1")
        XCTAssertEqual(k.source, .group)
        XCTAssertEqual(k.sessions, 1)
        XCTAssertEqual(k.titles, ["Statusleiste"])
        XCTAssertEqual(k.lastUsed, 1_700_000_000)
        XCTAssertEqual(index[1].lastUsed, 10)
        XCTAssertEqual(index[1].score, 2)
        XCTAssertEqual(k.score, 0.6, accuracy: 0.001, "Gruppe und Sessions geben Bonus")
    }

    func testNameInTextIsConfident() {
        let m = QuickStartMatcher.match("In RoboThomas habe ich die Idee, dass der Roboter piept", in: projects)
        XCTAssertEqual(m.top?.path, "/p/robo-thomas", "Bindestrich und Großschreibung zählen nicht")
        XCTAssertEqual(m.topStrength, 2)
        XCTAssertTrue(m.confident)
        let spaced = QuickStartMatcher.match("bei robo thomas: Akku anzeigen", in: projects)
        XCTAssertEqual(spaced.top?.path, "/p/robo-thomas", "zusammengezogene Wortfolge")
        XCTAssertTrue(spaced.confident)
    }

    func testStopwordFolderNeverMatchesAlone() {
        let m = QuickStartMatcher.match("Die App soll einen Dark Mode bekommen", in: projects)
        XCTAssertEqual(m.topStrength, 0, "„app“ ist kein Projektname im Satz")
        XCTAssertFalse(m.confident)
        XCTAssertEqual(m.top?.path, "/p/app", "ohne Treffer zählt die Nutzung")
    }

    func testPrefixIsWeakAndAmbiguityIsNotConfident() {
        let weak = QuickStartMatcher.match("im kadrel noch einen Knopf", in: projects)
        XCTAssertEqual(weak.top?.path, "/p/kadrell")
        XCTAssertEqual(weak.topStrength, 1)
        XCTAssertFalse(weak.confident)
        let two = QuickStartMatcher.match("Kadrell und Bloom angleichen", in: projects)
        XCTAssertEqual(two.topStrength, 2)
        XCTAssertFalse(two.confident, "zwei Namen im Text: das Modell oder der Nutzer entscheidet")
    }

    func testCleanedPromptOnlyWhenOnlyTheReferenceIsGone() {
        let c = projects[1]
        let original = "In RoboThomas habe ich die Idee, dass der Roboter beim Start piept."
        XCTAssertTrue(QuickStartMatcher.acceptCleaned("der Roboter beim Start piept.", original: original, candidate: c))
        XCTAssertTrue(QuickStartMatcher.acceptCleaned("Der Roboter beim Start piept", original: original, candidate: c))
        XCTAssertFalse(QuickStartMatcher.acceptCleaned("Der Roboter soll beim Start laut piepen", original: original, candidate: c), "umformuliert")
        XCTAssertFalse(QuickStartMatcher.acceptCleaned("habe ich die Idee, dass der Roboter beim Start", original: original, candidate: c), "Projektname bleibt drin, Inhalt fehlt")
        XCTAssertFalse(QuickStartMatcher.acceptCleaned(original, original: original, candidate: c))
        XCTAssertFalse(QuickStartMatcher.acceptCleaned("piept", original: original, candidate: c), "zu kurz")
    }

    func testPromptListsCandidatesAndParsesAnswer() {
        var c = projects[0]
        c.sessions = 4
        c.lastUsed = 1_761_800_000
        c.titles = ["Statusleiste"]
        let user = QuickStartPrompt.user(idea: "Idee", candidates: [c, projects[2]], nameMatch: 0)
        XCTAssertTrue(user.contains("0: Kadrell | /p/kadrell | 4 sessions | last used 2025-10-"))
        XCTAssertTrue(user.contains("1: Bloom"))
        XCTAssertTrue(user.hasSuffix("Idea: Idee"))
        XCTAssertEqual(QuickStartPrompt.parse("```json\n{\"index\": 1, \"sure\": true, \"prompt\": \"X\"}\n```"),
                       QuickStartPrompt.Answer(index: 1, sure: true, prompt: "X"))
        XCTAssertNil(QuickStartPrompt.parse("weiß nicht"))
    }

    // MARK: Ablauf im Dialog

    private func existingProjects() throws -> [QuickStartCandidate] {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-quickstart-\(UUID().uuidString)")
        for n in ["kadrell", "robo-thomas", "bloom"] { try FileManager.default.createDirectory(at: root.appendingPathComponent(n), withIntermediateDirectories: true) }
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        return [candidate(root.path + "/kadrell", "Kadrell", source: .group, score: 5, groupId: "g1"),
                candidate(root.path + "/robo-thomas", score: 1), candidate(root.path + "/bloom", "Bloom", score: 3)]
    }

    func testWithoutModelNameMatchStartsWithOriginalText() throws {
        let model = QuickStartModel(candidates: try existingProjects(), modelName: nil, ask: nil)
        var started: (String, String)?
        model.onStart = { c, p in started = (c.name, p) }
        model.idea = "In RoboThomas habe ich die Idee, dass er piept"
        XCTAssertEqual(model.live.top?.name, "robo-thomas", "Hinweis schon beim Tippen")
        model.submit()
        XCTAssertEqual(model.phase, .pick, "erst bestätigen, dann starten")
        XCTAssertEqual(model.list.first?.name, "robo-thomas")
        model.submit()
        XCTAssertEqual(started?.0, "robo-thomas")
        XCTAssertEqual(started?.1, "In RoboThomas habe ich die Idee, dass er piept")
    }

    func testTypingAndArrowsPickAnotherProject() throws {
        let model = QuickStartModel(candidates: try existingProjects(), modelName: nil, ask: nil)
        var started: String?
        model.onStart = { c, _ in started = c.name }
        model.idea = "Ein neuer Knopf"
        model.submit()
        XCTAssertEqual(model.list.first?.name, "Kadrell", "ohne Namen: meistbenutztes zuerst")
        model.move(1)
        XCTAssertEqual(model.list[model.selected].name, "Bloom")
        model.filter = "rob"
        XCTAssertEqual(model.list.map(\.name), ["robo-thomas"])
        model.submit()
        XCTAssertEqual(started, "robo-thomas")
    }

    func testEscapeGoesBackThenCloses() throws {
        let model = QuickStartModel(candidates: try existingProjects(), modelName: nil, ask: nil)
        var closed = false
        model.onClose = { closed = true }
        model.idea = "Kadrell bekommt X"
        model.submit()
        model.cancel()
        XCTAssertEqual(model.phase, .idea)
        XCTAssertFalse(closed)
        XCTAssertEqual(model.idea, "Kadrell bekommt X", "Idee bleibt beim Zurückgehen")
        model.cancel()
        XCTAssertTrue(closed)
    }

    func testModelDecidesWhenNameIsUnclearAndCleansPrompt() async throws {
        let projects = try existingProjects()
        let asked = expectation(description: "Modell gefragt")
        let model = QuickStartModel(candidates: projects, modelName: "qwen3:4b") { system, user in
            XCTAssertTrue(system.contains("JSON"))
            XCTAssertTrue(user.contains("Idea: Beim Roboter möchte ich, dass er beim Start piept"))
            asked.fulfill()
            let index = user.components(separatedBy: "\n").first { $0.contains("robo-thomas") }.flatMap { Int($0.prefix { $0.isNumber }) } ?? -1
            return "{\"index\": \(index), \"sure\": true, \"prompt\": \"dass er beim Start piept\"}"
        }
        model.idea = "Beim Roboter möchte ich, dass er beim Start piept"
        model.submit()
        XCTAssertEqual(model.phase, .thinking)
        await fulfillment(of: [asked], timeout: 2)
        try await waitUntil { model.phase == .pick }
        XCTAssertEqual(model.list.first?.name, "robo-thomas", "Modell wählt aus der Liste")
        XCTAssertNil(model.cleaned, "„Roboter“ ist kein Name des Projekts: der Prompt bleibt, wie er ist")
        XCTAssertTrue(model.origin.contains("qwen3:4b"))
    }

    func testModelStripsReferenceWhenSafe() async throws {
        let model = QuickStartModel(candidates: try existingProjects(), modelName: "m") { _, _ in
            "{\"index\": 0, \"sure\": true, \"prompt\": \"der Roboter piept beim Start\"}"
        }
        var started: String?
        model.onStart = { _, p in started = p }
        model.idea = "In RoboThomas: der Roboter piept beim Start"
        model.submit()
        try await waitUntil { model.phase == .pick }
        XCTAssertEqual(model.list.first?.name, "robo-thomas", "eindeutiger Name schlägt das Modell")
        XCTAssertEqual(model.prompt, "der Roboter piept beim Start")
        model.toggleCleaned()
        XCTAssertEqual(model.prompt, "In RoboThomas: der Roboter piept beim Start", "Tab: zurück zum Originaltext")
        model.toggleCleaned()
        model.submit()
        XCTAssertEqual(started, "der Roboter piept beim Start")
    }

    func testModelFailureFallsBackToNameSearch() async throws {
        let model = QuickStartModel(candidates: try existingProjects(), modelName: "m") { _, _ in throw LocalModelError.timeout }
        model.idea = "irgendwas Neues"
        model.submit()
        try await waitUntil { model.phase == .pick }
        XCTAssertEqual(model.list.first?.name, "Kadrell")
        XCTAssertTrue(model.origin.contains(LocalModelError.timeout.errorDescription!))
    }

    func testEnterWhileThinkingSkipsTheModel() async throws {
        let model = QuickStartModel(candidates: try existingProjects(), modelName: "m") { _, _ in
            try await Task.sleep(for: .seconds(10))
            return "{}"
        }
        model.idea = "bloom kriegt einen Export"
        model.submit()
        XCTAssertEqual(model.phase, .thinking)
        model.submit()
        XCTAssertEqual(model.phase, .pick)
        XCTAssertEqual(model.list.first?.name, "Bloom")
    }

    private func waitUntil(_ condition: @escaping () -> Bool, timeout: TimeInterval = 2) async throws {
        let end = Date().addingTimeInterval(timeout)
        while !condition() {
            guard Date() < end else { return XCTFail("Bedingung nicht erreicht") }
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}
