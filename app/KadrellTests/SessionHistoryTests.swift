import XCTest
@testable import Kadrell

/// Verlaufssuche: Transcripts lesen (nur eigene Prompts und Titel), inkrementell nachlesen, suchen.
final class SessionHistoryTests: XCTestCase {
    private let id = "2bf3b100-502a-44fa-aecf-499333e74ad8"
    private var root: URL!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("history-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root.appendingPathComponent("projects/-p"), withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(at: root) }

    private func user(_ text: String, cwd: String = "/p", extra: String = "") -> String {
        #"{"type":"user","cwd":"\#(cwd)"\#(extra),"message":{"role":"user","content":"\#(text)"}}"#
    }

    private func parse(_ lines: [String]) -> HistorySession {
        var e = HistorySession(sessionId: id, path: "/x")
        _ = SessionHistoryParser.read(Data((lines.joined(separator: "\n") + "\n").utf8), into: &e)
        return e
    }

    private func write(_ lines: [String], name: String? = nil) throws -> String {
        let path = root.appendingPathComponent("projects/-p/\(name ?? id).jsonl").path
        try Data((lines.joined(separator: "\n") + "\n").utf8).write(to: URL(fileURLWithPath: path))
        return path
    }

    func testReadsPromptsCwdAndLatestTitle() {
        let e = parse([user("Suche mit Qwen und Ollama", cwd: "/a"), user("weiter", cwd: "/b"),
                       #"{"type":"ai-title","aiTitle":"Alt"}"#, #"{"type":"ai-title","aiTitle":"Lokale Modelle"}"#])
        XCTAssertEqual(e.prompts, ["Suche mit Qwen und Ollama", "weiter"])
        XCTAssertEqual(e.cwd, "/a", "Ordner der ersten Eingabe, dort liegt das Transcript")
        XCTAssertEqual(e.title, "Lokale Modelle")
    }

    func testCustomTitleWinsOverAiTitle() {
        let e = parse([#"{"type":"custom-title","customTitle":"Mein Name"}"#, #"{"type":"ai-title","aiTitle":"KI"}"#, user("x")])
        XCTAssertEqual(e.title, "Mein Name")
    }

    func testSkipsAnswersToolResultsAndMachineMessages() {
        let e = parse([
            #"{"type":"assistant","message":{"content":[{"type":"text","text":"Antwort"}]}}"#,
            #"{"type":"user","message":{"content":[{"type":"tool_result","content":"Ausgabe"}]}}"#,
            user("Zusammenfassung", extra: #","isCompactSummary":true"#),
            user("Task fertig", extra: #","promptSource":"system""#),
            user("Peer", extra: #","origin":{"kind":"peer"}"#),
            user("Another Claude session sent a message: hallo"),
            user("echt", extra: #","origin":{"kind":"human"},"promptSource":"typed""#),
        ])
        XCTAssertEqual(e.prompts, ["echt"])
    }

    /// Claude schreibt gerade: die angefangene letzte Zeile bleibt liegen und wird beim nächsten Lauf gelesen.
    func testLeavesPartialLastLine() {
        var e = HistorySession(sessionId: id, path: "/x")
        let full = user("eins") + "\n"
        let used = SessionHistoryParser.read(Data((full + user("zw")).utf8), into: &e)
        XCTAssertEqual(used, full.utf8.count)
        XCTAssertEqual(e.prompts, ["eins"])
    }

    func testRefreshReadsOnlyGrownPartAndDropsDeleted() async throws {
        let path = try write([user("eins")])
        _ = try write([user("weg")], name: "11111111-1111-1111-1111-111111111111")
        _ = try write([user("nie")], name: "\(id).orphaned-1-x")
        var (entries, changed) = await SessionHistoryParser.refresh([:], root: root.path)
        XCTAssertTrue(changed)
        XCTAssertEqual(entries.count, 2, "beiseitegelegte Fassungen zählen nicht")
        XCTAssertEqual(entries[path]?.prompts, ["eins"])

        let h = try FileHandle(forWritingTo: URL(fileURLWithPath: path))
        h.seekToEndOfFile()
        h.write(Data((user("zwei") + "\n").utf8))
        try h.close()
        try FileManager.default.removeItem(atPath: root.appendingPathComponent("projects/-p/11111111-1111-1111-1111-111111111111.jsonl").path)
        (entries, changed) = await SessionHistoryParser.refresh(entries, root: root.path)
        XCTAssertTrue(changed)
        XCTAssertEqual(entries.count, 1)
        XCTAssertEqual(entries[path]?.prompts, ["eins", "zwei"])

        (entries, changed) = await SessionHistoryParser.refresh(entries, root: root.path)
        XCTAssertFalse(changed, "unverändert: nichts neu gelesen")
    }

    private func session(_ title: String, prompts: [String], at t: Double, cwd: String = "/p/proj") -> HistorySession {
        var s = HistorySession(sessionId: UUID().uuidString, path: "/\(t)", cwd: cwd, prompts: prompts, lastActive: t)
        s.aiTitle = title
        return s
    }

    func testSearchNeedsEveryWordAndIgnoresCaseAndAccents() {
        let index = HistoryIndex([session("Benchmark", prompts: ["lokale Suche mit Qwen und Ollama"], at: 1),
                                  session("Logo", prompts: ["Qwen Image für Logos"], at: 2)])
        XCTAssertEqual(index.search("qwen OLLAMA").map(\.session.title), ["Benchmark"])
        XCTAssertEqual(index.search("fur logos").map(\.session.title), ["Logo"])
    }

    func testTitleHitsFirstThenNewest() {
        let index = HistoryIndex([session("Alt", prompts: ["ollama"], at: 1), session("Neu", prompts: ["ollama"], at: 3),
                                  session("Ollama Setup", prompts: ["x"], at: 2)])
        XCTAssertEqual(index.search("ollama").map(\.session.title), ["Ollama Setup", "Neu", "Alt"])
    }

    func testEmptyQueryListsNewestFirstAndSnippetStartsNearHit() {
        let long = String(repeating: "a", count: 100) + " Minimodelle lokal"
        let index = HistoryIndex([session("A", prompts: [long], at: 1), session("B", prompts: ["x"], at: 2)])
        XCTAssertEqual(index.search("").map(\.session.title), ["B", "A"])
        let snippet = index.search("minimodelle").first?.snippet ?? ""
        XCTAssertTrue(snippet.hasPrefix("…"))
        XCTAssertTrue(snippet.contains("Minimodelle lokal"))
    }

    func testFuzzyFallbackOnTitleOnly() {
        let index = HistoryIndex([session("Kadrell Statusleiste", prompts: ["x"], at: 1)])
        XCTAssertEqual(index.search("kdrstat").count, 1)
        XCTAssertTrue(index.search("zzzq").isEmpty)
    }
}

extension SessionHistoryTests {
    /// Ein eigener Titel, der nur der Ordnername ist, verdrängt Claudes Titel nicht.
    func testFolderNameAsCustomTitleFallsBackToAiTitle() {
        var s = HistorySession(sessionId: "x", path: "/x", cwd: "/p/kadrell")
        s.customTitle = "kadrell"
        s.aiTitle = "Verlaufssuche bauen"
        XCTAssertEqual(s.title, "Verlaufssuche bauen")
        s.customTitle = "Mein Name"
        XCTAssertEqual(s.title, "Mein Name")
    }

    func testFuzzyNeedsLettersCloseTogether() {
        XCTAssertTrue(HistoryIndex.tight(Array("kdrst"), Array("kadrell status")))
        XCTAssertFalse(HistoryIndex.tight(Array("kdrst"), Array("kai der große rest von irgendwas statt")))
    }
}
