import XCTest
@testable import Kadrell

final class LinkTargetTests: XCTestCase {
    private var root: String!

    override func setUpWithError() throws {
        root = (NSTemporaryDirectory() as NSString).appendingPathComponent("linktarget-\(UUID().uuidString)")
        let fm = FileManager.default
        try fm.createDirectory(atPath: root + "/docs/specs", withIntermediateDirectories: true)
        try "x".write(toFile: root + "/docs/specs/a.md", atomically: true, encoding: .utf8)
        root = (root as NSString).resolvingSymlinksInPath
    }

    override func tearDownWithError() throws { try? FileManager.default.removeItem(atPath: root) }

    func testRelativePathGoesFromSessionFolder() {
        XCTAssertEqual(LinkTarget.resolve("docs/specs/a.md", base: root), .path(root + "/docs/specs/a.md", isDirectory: false))
        XCTAssertEqual(LinkTarget.resolve("./docs/../docs/specs", base: root), .path(root + "/docs/specs", isDirectory: true))
    }

    func testPositionSuffixIsDroppedOnlyWhenFileHasNoSuchName() {
        XCTAssertEqual(LinkTarget.resolve("docs/specs/a.md:12:4", base: root), .path(root + "/docs/specs/a.md", isDirectory: false))
    }

    func testAbsoluteAndHomePaths() throws {
        XCTAssertEqual(LinkTarget.resolve(root + "/docs/specs/a.md", base: "/nowhere"), .path(root + "/docs/specs/a.md", isDirectory: false))
        XCTAssertEqual(LinkTarget.resolve("~/docs/specs/a.md", base: "/nowhere", home: root), .path(root + "/docs/specs/a.md", isDirectory: false))
    }

    func testMissingFileOpensNothing() {
        XCTAssertNil(LinkTarget.resolve("docs/gibt-es-nicht.md", base: root))
    }

    func testUrlsStayUrls() {
        XCTAssertEqual(LinkTarget.resolve("https://kadrell.malura.de/x", base: root), .url("https://kadrell.malura.de/x"))
        XCTAssertEqual(LinkTarget.resolve("mailto:a@b.de", base: root), .url("mailto:a@b.de"))
    }
}
