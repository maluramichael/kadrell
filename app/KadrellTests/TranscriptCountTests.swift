import XCTest
@testable import Kadrell

/// `Transcript.userMessages` zählt echte Eingaben des Nutzers in einem Transcript-Ausschnitt.
final class TranscriptCountTests: XCTestCase {
    private func data(_ lines: [String]) -> Data { Data(lines.joined(separator: "\n").utf8) }

    func testCountsPlainTextMessage() {
        XCTAssertEqual(Transcript.userMessages(jsonl: data([#"{"type":"user","message":{"content":"Hallo"}}"#])), 1)
    }

    func testCountsTextBlockMessage() {
        let line = #"{"type":"user","message":{"content":[{"type":"text","text":"Bau das"}]}}"#
        XCTAssertEqual(Transcript.userMessages(jsonl: data([line])), 1)
    }

    func testIgnoresAssistantLines() {
        let line = #"{"type":"assistant","message":{"content":[{"type":"text","text":"Fertig"}]}}"#
        XCTAssertEqual(Transcript.userMessages(jsonl: data([line])), 0)
    }

    /// Tool-Ergebnisse kommen als `type: user` zurück, sind aber keine Eingabe.
    func testIgnoresToolResults() {
        let line = #"{"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}}"#
        XCTAssertEqual(Transcript.userMessages(jsonl: data([line])), 0)
    }

    func testIgnoresMetaAndSidechain() {
        let meta = #"{"type":"user","isMeta":true,"message":{"content":"Caveat"}}"#
        let side = #"{"type":"user","isSidechain":true,"message":{"content":"Subagent"}}"#
        XCTAssertEqual(Transcript.userMessages(jsonl: data([meta, side])), 0)
    }

    /// Die Ausgabe eines Slash-Commands steht als `<command-name>…` in einer user-Zeile.
    func testIgnoresCommandOutput() {
        let line = #"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#
        XCTAssertEqual(Transcript.userMessages(jsonl: data([line])), 0)
    }

    /// Ein Delta kann mitten in einer Zeile beginnen: der Rest ist kein JSON und darf nicht mitzählen.
    func testIgnoresTruncatedFirstLine() {
        let broken = #"e":"user","message":{"content":"halb"}}"#
        let good = #"{"type":"user","message":{"content":"ganz"}}"#
        XCTAssertEqual(Transcript.userMessages(jsonl: data([broken, good])), 1)
    }

    func testCountsSeveralMessages() {
        let lines = [
            #"{"type":"user","message":{"content":"eins"}}"#,
            #"{"type":"assistant","message":{"content":[{"type":"text","text":"ok"}]}}"#,
            #"{"type":"user","message":{"content":[{"type":"tool_result","content":"x"}]}}"#,
            #"{"type":"user","message":{"content":[{"type":"text","text":"zwei"}]}}"#,
        ]
        XCTAssertEqual(Transcript.userMessages(jsonl: data(lines)), 2)
    }

    func testEmptyDataCountsNothing() {
        XCTAssertEqual(Transcript.userMessages(jsonl: Data()), 0)
    }
}
