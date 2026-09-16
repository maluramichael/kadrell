import XCTest
@testable import Kadrell

final class FeedbackTests: XCTestCase {
    func testTransitions() {
        let old: [String: SessionStatus] = ["a": .running, "b": .running, "c": .idle, "d": .waiting]
        let new: [String: SessionStatus] = ["a": .idle, "b": .waiting, "c": .idle, "d": .waiting, "e": .waiting]
        let t = Feedback.transitions(from: old, to: new)
        XCTAssertEqual(t.done, ["a"])
        // Unveränderte (d) und neu angehängte (e) Sessions zählen nicht.
        XCTAssertEqual(t.waiting, ["b"])
        XCTAssertTrue(Feedback.transitions(from: new, to: new).waiting.isEmpty)
    }

    func testProgress() {
        XCTAssertEqual(Feedback.progress(since: 10, duration: 1, now: 10), 0)
        XCTAssertEqual(Feedback.progress(since: 10, duration: 1, now: 10.5)!, 0.875, accuracy: 0.001)
        XCTAssertNil(Feedback.progress(since: 10, duration: 1, now: 11))
    }
}
