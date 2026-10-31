import XCTest
@testable import Kadrell

/// Abgleich gemeldeter Kacheln mit `sessions/<pid>.json`, wenn Claude Code keinen Hook schickt (Abbruch per Esc/⌃C).
final class ReportFallbackTests: XCTestCase {
    private func file(_ status: String?, at ms: Double?, waitingFor: String? = nil) throws -> Agent {
        var obj: [String: Any] = ["cwd": "/p", "kind": "interactive", "startedAt": 1, "sessionId": "s", "name": "n", "pid": 1]
        obj["status"] = status
        obj["statusUpdatedAt"] = ms
        obj["waitingFor"] = waitingFor
        return try JSONDecoder().decode(Agent.self, from: JSONSerialization.data(withJSONObject: obj))
    }

    func testNewerIdleEndsWorking() throws {
        let fix = ReportFallback.correction(current: .running, reportedAt: 1_000, file: try file("idle", at: 2_000), now: 2_100)
        XCTAssertEqual(fix?.status, "idle")
        XCTAssertNil(fix?.waitingFor)
    }

    func testNewerIdleEndsWaiting() throws {
        XCTAssertEqual(ReportFallback.correction(current: .waiting, reportedAt: 1_000, file: try file("idle", at: 2_000), now: 2_100)?.status, "idle")
    }

    func testNewerBusyEndsWaitingAfterApproval() throws {
        XCTAssertEqual(ReportFallback.correction(current: .waiting, reportedAt: 1_000, file: try file("busy", at: 2_000), now: 2_100)?.status, "busy")
    }

    func testNewerWaitingKeepsItsReason() throws {
        let fix = ReportFallback.correction(current: .running, reportedAt: 1_000, file: try file("waiting", at: 2_000, waitingFor: "input needed"), now: 2_100)
        XCTAssertEqual(fix?.status, "waiting")
        XCTAssertEqual(fix?.waitingFor, "input needed")
    }

    func testOlderFileKeepsReportWithinGrace() throws {
        XCTAssertNil(ReportFallback.correction(current: .running, reportedAt: 10_000, file: try file("idle", at: 9_000), now: 10_000 + ReportFallback.grace - 1))
        XCTAssertNil(ReportFallback.correction(current: .running, reportedAt: 10_000, file: try file("busy", at: 9_999), now: 99_000), "busy von der Runde selbst")
    }

    func testOlderIdleAfterGraceMeansTurnNeverStarted() throws {
        XCTAssertEqual(ReportFallback.correction(current: .running, reportedAt: 10_000, file: try file("idle", at: 9_000), now: 10_000 + ReportFallback.grace + 1)?.status, "idle")
        XCTAssertNil(ReportFallback.correction(current: .waiting, reportedAt: 10_000, file: try file("idle", at: 9_000), now: 99_000),
                     "Warten kann ohne Dateistand gemeldet sein (Notification), bleibt")
    }

    func testIdleOrUnknownLeavesReportAlone() throws {
        XCTAssertNil(ReportFallback.correction(current: .idle, reportedAt: 1_000, file: try file("busy", at: 2_000), now: 2_100))
        XCTAssertNil(ReportFallback.correction(current: .running, reportedAt: 1_000, file: nil, now: 99_000))
        XCTAssertNil(ReportFallback.correction(current: .running, reportedAt: 1_000, file: try file("idle", at: nil), now: 99_000), "ältere CLI ohne Zeitstempel")
        XCTAssertNil(ReportFallback.correction(current: .running, reportedAt: 1_000, file: try file("busy", at: 2_000), now: 2_100), "gleicher Status")
    }
}
