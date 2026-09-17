import XCTest
@testable import Kadrell

final class UpdateCheckerTests: XCTestCase {
    func testNewerVersionDetected() {
        XCTAssertTrue(UpdateChecker.isNewer("1.39.0", than: "1.38.0"))
        XCTAssertTrue(UpdateChecker.isNewer("2.0.0", than: "1.38.9"))
        XCTAssertTrue(UpdateChecker.isNewer("1.38.1", than: "1.38.0"))
    }

    func testSameOrOlderVersionIsNotNewer() {
        XCTAssertFalse(UpdateChecker.isNewer("1.38.0", than: "1.38.0"))
        XCTAssertFalse(UpdateChecker.isNewer("1.37.9", than: "1.38.0"))
        XCTAssertFalse(UpdateChecker.isNewer("1.38.0", than: "1.39.0"))
    }

    func testUnparsableVersionIsNotNewer() {
        XCTAssertFalse(UpdateChecker.isNewer("", than: "1.38.0"))
        XCTAssertFalse(UpdateChecker.isNewer("1.38.0", than: "not-a-version"))
    }

    func testManifestDecoding() throws {
        let json = """
        {"version": "1.39.0", "url": "https://kadrell.malura.de/download/Kadrell-1.39.0.dmg", "sha256": "abc", "notes": ["Feature: X"]}
        """
        let m = try JSONDecoder().decode(UpdateManifest.self, from: Data(json.utf8))
        XCTAssertEqual(m.version, "1.39.0")
        XCTAssertEqual(m.notes, ["Feature: X"])
    }
}
