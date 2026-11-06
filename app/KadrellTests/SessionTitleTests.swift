import XCTest
@testable import Kadrell

final class SessionTitleTests: XCTestCase {
    func testCleanTakesFirstLineAndStripsQuotesAndFullStop() {
        XCTAssertEqual(SessionTitlePrompt.clean("  \"Kadrell Sessiontitel.\"\nzweite Zeile"), "Kadrell Sessiontitel")
        XCTAssertEqual(SessionTitlePrompt.clean("„Dark Mode einbauen“"), "Dark Mode einbauen")
    }

    func testCleanRejectsEmptyAndCapsLength() {
        XCTAssertNil(SessionTitlePrompt.clean("  \"\"  "))
        XCTAssertNil(SessionTitlePrompt.clean(""))
        XCTAssertEqual(SessionTitlePrompt.clean(String(repeating: "a", count: 200))?.count, 60)
    }

    func testParseReadsTitleFromJSON() {
        XCTAssertEqual(SessionTitlePrompt.parse(#"{"title": "Hook-Fehler beheben."}"#), "Hook-Fehler beheben")
        XCTAssertNil(SessionTitlePrompt.parse("kein json"))
        XCTAssertNil(SessionTitlePrompt.parse(#"{"title": ""}"#))
    }

    func testReadmeHeadIsCappedAndMissingFileGivesNil() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("sessiontitle-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        XCTAssertNil(SessionTitlePrompt.readmeHead(cwd: dir.path))
        try String(repeating: "x", count: 1000).write(to: dir.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)
        XCTAssertEqual(SessionTitlePrompt.readmeHead(cwd: dir.path, limit: 400)?.count, 400)
    }

    func testDisplayOrderCustomLocalClaudeFirstPrompt() {
        var s = Session(id: "a", cwd: "/tmp/proj", startedAt: 0, sessionId: "s", name: "")
        s.firstPrompt = "check mal"
        XCTAssertEqual(s.title, "check mal")
        s.name = "Claude Titel"
        XCTAssertEqual(s.title, "Claude Titel")
        s.localTitle = "Lokaler Titel"
        XCTAssertEqual(s.title, "Lokaler Titel")
        s.customName = "Von Hand"
        XCTAssertEqual(s.title, "Von Hand")
    }

    func testLocalTitleSurvivesCoding() throws {
        var s = Session(id: "a", cwd: "/tmp/proj", startedAt: 0, sessionId: "s", name: "")
        s.localTitle = "Lokaler Titel"
        let back = try JSONDecoder().decode(Session.self, from: JSONEncoder().encode(s))
        XCTAssertEqual(back.localTitle, "Lokaler Titel")
    }

    @MainActor func testTitlerAsksOnceAfterFirstPromptAndReportsTitle() async throws {
        var asked = 0, titles: [(String, String)] = []
        let titler = SessionTitler(isActive: { true }, ensureRunning: {}, ask: { _, _ in asked += 1; return #"{"title":"Sessiontitel bauen"}"# },
                                   readme: { _ in "README" })
        titler.onTitle = { titles.append(($0, $1)) }
        var s = Session(id: "a", cwd: "/tmp/proj", startedAt: 0, sessionId: "s", name: "")
        s.firstPrompt = "baue den Titel"
        await titler.run(s)
        await titler.run(s)
        XCTAssertEqual(asked, 1)
        XCTAssertEqual(titles.map(\.1), ["Sessiontitel bauen"])
    }

    @MainActor func testTitlerSkipsShellsCustomNamesAndExistingTitles() async {
        var asked = 0
        let titler = SessionTitler(isActive: { true }, ensureRunning: {}, ask: { _, _ in asked += 1; return "{}" }, readme: { _ in nil })
        var shell = Session(id: Session.newShellId(), cwd: "/tmp", startedAt: 0, sessionId: "", name: "")
        shell.firstPrompt = "x"
        var named = Session(id: "b", cwd: "/tmp", startedAt: 0, sessionId: "b", name: "")
        named.firstPrompt = "x"; named.customName = "Eigener"
        var titled = Session(id: "c", cwd: "/tmp", startedAt: 0, sessionId: "c", name: "")
        titled.firstPrompt = "x"; titled.localTitle = "Schon da"
        for s in [shell, named, titled] { await titler.run(s) }
        XCTAssertEqual(asked, 0)
    }

    @MainActor func testTitlerFailureIsSilent() async {
        var titles = 0
        let titler = SessionTitler(isActive: { true }, ensureRunning: {}, ask: { _, _ in throw LocalModelError.timeout }, readme: { _ in nil })
        titler.onTitle = { _, _ in titles += 1 }
        var s = Session(id: "a", cwd: "/tmp", startedAt: 0, sessionId: "s", name: "")
        s.firstPrompt = "x"
        await titler.run(s)
        XCTAssertEqual(titles, 0)
    }
}
