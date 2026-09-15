import XCTest
@testable import Kadrell

final class AgentLocalTests: XCTestCase {
    func testReadsOwnPidsFromSessionFiles() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-agents-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("sessions"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let json = #"{"pid":123,"sessionId":"s1","cwd":"/tmp/p","startedAt":1,"kind":"interactive","name":"p-1a","status":"busy","updatedAt":2}"#
        try json.write(to: dir.appendingPathComponent("sessions/123.json"), atomically: true, encoding: .utf8)
        try "kaputt".write(to: dir.appendingPathComponent("sessions/456.json"), atomically: true, encoding: .utf8)

        let agents = Agent.local(pids: [123, 456, 999], configDir: dir.path)

        XCTAssertEqual(agents.count, 1)
        XCTAssertEqual(agents.first?.pid, 123)
        XCTAssertEqual(agents.first?.status, "busy")
        XCTAssertEqual(agents.first?.sessionId, "s1")
        XCTAssertNil(agents.first?.shortId)
    }
}
