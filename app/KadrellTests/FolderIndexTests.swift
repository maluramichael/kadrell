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
}
