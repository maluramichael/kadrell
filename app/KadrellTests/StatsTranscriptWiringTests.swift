import XCTest
@testable import Kadrell

/// Nachrichten zählt nur, was während des Laufens dazukommt. Ein Transcript zum ersten Mal zu lesen darf den
/// alten Verlauf nicht nachträglich einrechnen.
final class StatsTranscriptWiringTests: XCTestCase {
    private let suite = "de.malura.kadrell.tests.statstranscript"
    private var dir: URL!
    private let sessionId = "11111111-2222-3333-4444-555555555555"
    private var file: URL { dir.appendingPathComponent("projects/proj/\(sessionId).jsonl") }

    private let message = #"{"type":"user","message":{"content":"eine Nachricht"}}"# + "\n"

    override func setUpWithError() throws {
        try super.setUpWithError()
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = UserDefaults(suiteName: suite)!
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("statstranscript-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: dir)
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = Profile.defaults
        super.tearDown()
    }

    private func write(_ text: String) throws { try text.write(to: file, atomically: true, encoding: .utf8) }

    private func refresh(_ cache: [String: Transcript.Entry]) -> [String: Transcript.Entry] {
        Transcript.refresh([sessionId], configDir: dir.path, cache: cache, wantText: false)
    }

    func testFirstReadCountsNothing() throws {
        try write(message + message + message)
        _ = refresh([:])
        XCTAssertEqual(Stats.count(.messages), 0)
    }

    func testGrowthCountsNewMessages() throws {
        try write(message)
        let cache = refresh([:])
        try write(message + message + message)
        _ = refresh(cache)
        XCTAssertEqual(Stats.count(.messages), 2)
    }

    /// Unverändertes Transcript: derselbe Stand darf beim nächsten Poll nicht noch einmal zählen.
    func testUnchangedTranscriptCountsNothing() throws {
        try write(message)
        var cache = refresh([:])
        cache = refresh(cache)
        _ = refresh(cache)
        XCTAssertEqual(Stats.count(.messages), 0)
    }

    /// Nach `/clear` schrumpft die Datei: das ist kein Zuwachs und zählt nicht.
    func testShrunkTranscriptCountsNothing() throws {
        try write(message + message + message)
        let cache = refresh([:])
        try write(message)
        _ = refresh(cache)
        XCTAssertEqual(Stats.count(.messages), 0)
    }
}
