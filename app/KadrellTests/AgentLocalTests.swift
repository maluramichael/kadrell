import XCTest
@testable import Kadrell

final class AgentLocalTests: XCTestCase {
    func testReadsOwnPidsFromSessionFiles() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-agents-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("sessions"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let json = #"{"pid":123,"sessionId":"s1","cwd":"/tmp/p","startedAt":9999999999999,"kind":"interactive","name":"p-1a","status":"busy","updatedAt":2}"#
        try json.write(to: dir.appendingPathComponent("sessions/123.json"), atomically: true, encoding: .utf8)
        try "kaputt".write(to: dir.appendingPathComponent("sessions/456.json"), atomically: true, encoding: .utf8)

        let agents = Agent.local(pids: [123, 456, 999], configDir: dir.path)

        XCTAssertEqual(agents.count, 1)
        XCTAssertEqual(agents.first?.pid, 123)
        XCTAssertEqual(agents.first?.status, "busy")
        XCTAssertEqual(agents.first?.sessionId, "s1")
        XCTAssertNil(agents.first?.shortId)
    }

    /// pid wiederverwendet: die liegengebliebene Datei ist älter als der laufende Prozess und zählt nicht.
    func testIgnoresSessionFileOlderThanProcess() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-agents-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("sessions"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let pid = Int(ProcessInfo.processInfo.processIdentifier)
        let file = dir.appendingPathComponent("sessions/\(pid).json")
        try #"{"pid":\#(pid),"sessionId":"alt","cwd":"/p","startedAt":1000,"kind":"interactive","name":"p"}"#.write(to: file, atomically: true, encoding: .utf8)
        XCTAssertTrue(Agent.local(pids: [pid], configDir: dir.path).isEmpty)
        let now = Int(Date().timeIntervalSince1970 * 1000)
        try #"{"pid":\#(pid),"sessionId":"neu","cwd":"/p","startedAt":\#(now),"kind":"interactive","name":"p"}"#.write(to: file, atomically: true, encoding: .utf8)
        XCTAssertEqual(Agent.local(pids: [pid], configDir: dir.path).first?.sessionId, "neu")
    }
}
