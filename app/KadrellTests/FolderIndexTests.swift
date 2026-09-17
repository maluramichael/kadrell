import XCTest
@testable import Kadrell

final class FolderIndexTests: XCTestCase {
    func testRankPrefersNamePrefixThenContainsThenFuzzy() {
        XCTAssertEqual(FolderIndex.rank("kad", path: "/x/kadrell"), 0)
        XCTAssertEqual(FolderIndex.rank("agent", path: "/x/claude-agent-overview"), 1)
        XCTAssertEqual(FolderIndex.rank("cao", path: "/x/claude-agent-overview"), 2)
        XCTAssertEqual(FolderIndex.rank("prj", path: "/x/projects/kadrell"), 3)
        XCTAssertNil(FolderIndex.rank("zzz", path: "/x/kadrell"))
        XCTAssertEqual(FolderIndex.rank("", path: "/x/kadrell"), 0)
    }

    func testRankWordsMustAllMatchLastInName() {
        XCTAssertEqual(FolderIndex.rank("projects kad", path: "/dev/projects/kadrell"), 0)
        XCTAssertNil(FolderIndex.rank("acme kad", path: "/dev/projects/kadrell"))
    }

    func testExpandAbbreviatedAndScan() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-folders-\(UUID().uuidString)").path
        defer { try? FileManager.default.removeItem(atPath: root) }
        for d in ["development/projects/kadrell/.git", "development/projects/kalender/.git", "development/other", "downloads"] {
            try FileManager.default.createDirectory(atPath: root + "/" + d, withIntermediateDirectories: true)
        }
        XCTAssertEqual(FolderIndex.expandAbbreviated("dev/pro/kad", base: root), [root + "/development/projects/kadrell"])
        XCTAssertEqual(FolderIndex.expandAbbreviated("development/projects/k", base: root),
                       [root + "/development/projects/kadrell", root + "/development/projects/kalender"])
        XCTAssertEqual(FolderIndex.expandAbbreviated("dev/projects/", base: root),
                       [root + "/development/projects/kadrell", root + "/development/projects/kalender"])
        XCTAssertEqual(FolderIndex.expandAbbreviated(root + "/d/pr/kdrl"), [root + "/development/projects/kadrell"])
        XCTAssertEqual(FolderIndex.scanRepos(roots: [root]), [root + "/development/projects/kadrell", root + "/development/projects/kalender"])
    }

    /// Rang 84: `folders-uses.json` statt eines UserDefaults-Blobs, und wächst nicht unbegrenzt weiter.
    @MainActor
    func testRecordUsePersistsAndPrunesMissingAndOldPaths() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-folders-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let existingDir = dir.appendingPathComponent("existing").path
        try FileManager.default.createDirectory(atPath: existingDir, withIntermediateDirectories: true)

        // Vorbelegte folders-uses.json: ein längst gelöschter Ordner, dazu der (noch existierende) Testordner.
        let usesURL = dir.appendingPathComponent("folders-uses.json")
        let gone = FolderIndex.Use(count: 9, last: Date().timeIntervalSince1970)
        let existing = FolderIndex.Use(count: 5, last: Date().timeIntervalSince1970 - 100 * 86400)
        try JSONEncoder().encode(["\(dir.path)/gone": gone, existingDir: existing]).write(to: usesURL, options: .atomic)

        let index = FolderIndex(directory: dir)
        XCTAssertEqual(index.uses.count, 2)
        index.recordUse(existingDir)
        // Gelöschter Ordner fliegt raus, der existierende bleibt (recordUse aktualisiert `last`, damit
        // übersteht er den 90-Tage-Filter trotz des alten Zeitstempels aus der Datei).
        XCTAssertNil(index.uses["\(dir.path)/gone"])
        XCTAssertEqual(index.uses[existingDir]?.count, 6)

        let reloaded = FolderIndex(directory: dir)
        XCTAssertEqual(reloaded.uses.keys.sorted(), [existingDir])
    }
}
