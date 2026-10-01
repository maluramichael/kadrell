import XCTest
@testable import Kadrell

final class ClaudeCLIVersionTests: XCTestCase {
    func testParseVersionFromTypicalOutput() {
        XCTAssertEqual(ClaudeCLI.parseVersion("2.1.274 (Claude Code)")?.major, 2)
        XCTAssertEqual(ClaudeCLI.parseVersion("2.1.274 (Claude Code)")?.minor, 1)
        XCTAssertEqual(ClaudeCLI.parseVersion("2.1.274 (Claude Code)")?.patch, 274)
    }

    func testParseVersionFailsOnUnknownFormat() {
        XCTAssertNil(ClaudeCLI.parseVersion("command not found: claude"))
        XCTAssertNil(ClaudeCLI.parseVersion(""))
    }

    func testMinVersionComparison() throws {
        let older = try XCTUnwrap(ClaudeCLI.parseVersion("2.1.272"))
        let min = ClaudeCLI.minVersion
        let newer = try XCTUnwrap(ClaudeCLI.parseVersion("2.1.274"))
        let nextMinor = try XCTUnwrap(ClaudeCLI.parseVersion("2.2.0"))
        XCTAssertTrue((older.major, older.minor, older.patch) < (min.major, min.minor, min.patch))
        XCTAssertFalse((newer.major, newer.minor, newer.patch) < (min.major, min.minor, min.patch))
        XCTAssertFalse((nextMinor.major, nextMinor.minor, nextMinor.patch) < (min.major, min.minor, min.patch))
    }

    func testRunSeparatedKeepsStderrOutOfStdout() async throws {
        let r = try await ProcessRunner.runSeparated("/bin/sh", ["-c", #"echo "[warn]" >&2; echo "[]""#])
        XCTAssertEqual(r.status, 0)
        XCTAssertEqual(r.stdout.trimmingCharacters(in: .whitespacesAndNewlines), "[]")
        XCTAssertEqual(r.stderr.trimmingCharacters(in: .whitespacesAndNewlines), "[warn]")
    }

    func testRunSeparatedHandlesLargeStderrWithoutDeadlock() async throws {
        let r = try await ProcessRunner.runSeparated("/bin/sh", ["-c", "head -c 300000 /dev/zero | tr '\\0' e >&2; echo ok"], timeout: 10)
        XCTAssertEqual(r.stderr.count, 300000)
        XCTAssertEqual(r.stdout.trimmingCharacters(in: .whitespacesAndNewlines), "ok")
    }

    func testRunSeparatedTimeoutKillsHangingProcess() async throws {
        let t0 = Date()
        let r = try await ProcessRunner.runSeparated("/bin/sh", ["-c", "echo partial; sleep 30"], timeout: 1)
        XCTAssertLessThan(Date().timeIntervalSince(t0), 10)
        XCTAssertEqual(r.stdout.trimmingCharacters(in: .whitespacesAndNewlines), "partial")
    }

    func testRunSeparatedEmptyOutput() async throws {
        let r = try await ProcessRunner.runSeparated("/usr/bin/true", [])
        XCTAssertEqual(r.stdout, "")
        XCTAssertEqual(r.stderr, "")
    }

    func testParseAgentsIgnoresEverythingButJSON() throws {
        XCTAssertEqual(try ClaudeCLI.parseAgents("[]"), [])
        XCTAssertEqual(try ClaudeCLI.parseAgents("  \n"), [])
    }
}
