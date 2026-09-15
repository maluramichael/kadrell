import XCTest
@testable import Kadrell

final class GitTests: XCTestCase {
    func testBranchFromRepoWorktreeAndDetached() throws {
        let fm = FileManager.default
        let root = fm.temporaryDirectory.appendingPathComponent("kadrell-git-\(UUID().uuidString)")
        defer { try? fm.removeItem(at: root) }
        let repo = root.appendingPathComponent("repo")
        try fm.createDirectory(at: repo.appendingPathComponent(".git/worktrees/wt"), withIntermediateDirectories: true)
        try fm.createDirectory(at: repo.appendingPathComponent("src/deep"), withIntermediateDirectories: true)
        try "ref: refs/heads/feature/x\n".write(to: repo.appendingPathComponent(".git/HEAD"), atomically: true, encoding: .utf8)
        XCTAssertEqual(Git.branch(at: repo.path), "feature/x")
        XCTAssertEqual(Git.branch(at: repo.appendingPathComponent("src/deep").path), "feature/x")

        let wt = root.appendingPathComponent("wt")
        try fm.createDirectory(at: wt, withIntermediateDirectories: true)
        try "gitdir: \(repo.path)/.git/worktrees/wt\n".write(to: wt.appendingPathComponent(".git"), atomically: true, encoding: .utf8)
        try "ref: refs/heads/master\n".write(to: repo.appendingPathComponent(".git/worktrees/wt/HEAD"), atomically: true, encoding: .utf8)
        XCTAssertEqual(Git.branch(at: wt.path), "master")

        try "7080dfd1234567890abcdef\n".write(to: repo.appendingPathComponent(".git/HEAD"), atomically: true, encoding: .utf8)
        XCTAssertEqual(Git.branch(at: repo.path), "7080dfd")

        XCTAssertNil(Git.branch(at: root.appendingPathComponent("none").path))
    }
}
