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

    /// Bilder und Binärdateien gehören der Standard-App, Text und Ordner dem Editor.
    func testPrefersEditorOnlyForTextAndFolders() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-link-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        func write(_ name: String, _ bytes: [UInt8]) -> String {
            let p = dir.appendingPathComponent(name).path
            FileManager.default.createFile(atPath: p, contents: Data(bytes))
            return p
        }
        XCTAssertTrue(LinkTarget.prefersEditor(dir.path, isDirectory: true))
        XCTAssertTrue(LinkTarget.prefersEditor(write("a.swift", Array("let x = 1\n".utf8)), isDirectory: false))
        XCTAssertTrue(LinkTarget.prefersEditor(write("Makefile", Array("all:\n".utf8)), isDirectory: false))
        XCTAssertTrue(LinkTarget.prefersEditor(write("a.ts", Array("const x = 1\n".utf8)), isDirectory: false), "TypeScript ist kein Video")
        XCTAssertFalse(LinkTarget.prefersEditor(write("a.png", [0x89, 0x50, 0x4E, 0x47, 0, 0, 0]), isDirectory: false))
        XCTAssertFalse(LinkTarget.prefersEditor(write("a.svg", Array("<svg/>".utf8)), isDirectory: false), "Bild, auch wenn Text")
        XCTAssertFalse(LinkTarget.prefersEditor(write("a.pdf", [0x25, 0x50, 0x44, 0x46, 0, 1]), isDirectory: false))
        XCTAssertFalse(LinkTarget.prefersEditor(write("noext", [1, 2, 0, 3]), isDirectory: false))
        XCTAssertTrue(LinkTarget.prefersEditor(dir.path + "/fehlt.txt", isDirectory: false))
    }
}
