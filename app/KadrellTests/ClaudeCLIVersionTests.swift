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
}
