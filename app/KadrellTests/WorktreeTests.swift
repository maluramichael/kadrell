import XCTest
@testable import Kadrell

final class WorktreeTests: XCTestCase {
    /// `git worktree list --porcelain`: Hauptarbeitskopie zuerst, `branch` fehlt bei detached HEAD,
    /// `locked`/`bare` werden ignoriert statt die Blockerkennung zu stören.
    func testParsePorcelain() {
        let output = """
        worktree /repo
        HEAD 1111111111111111111111111111111111111111
        branch refs/heads/master

        worktree /repo/.claude/worktrees/x
        HEAD 2222222222222222222222222222222222222222
        branch refs/heads/feature-x
        locked claude agent x

        worktree /repo-detached
        HEAD 3333333333333333333333333333333333333333
        detached

        """
        let entries = Worktree.parsePorcelain(output)
        XCTAssertEqual(entries, [
            .init(path: "/repo", branch: "master"),
            .init(path: "/repo/.claude/worktrees/x", branch: "feature-x"),
            .init(path: "/repo-detached", branch: nil),
        ])
    }

    /// Längster Präfix gewinnt: ein Worktree unter dem Repo (z. B. `.claude/worktrees/x`) matcht dessen eigenen
    /// Pfad, nicht den Repo-Root, obwohl beide Präfixe des Ziels sind.
    func testLongestPrefixMatch() {
        let worktrees = [Worktree.Entry(path: "/repo", branch: "master"), Worktree.Entry(path: "/repo/.claude/worktrees/x", branch: "feature-x")]
        XCTAssertEqual(Worktree.longestPrefixMatch("/repo/.claude/worktrees/x/src/a.swift", in: worktrees)?.branch, "feature-x")
        XCTAssertEqual(Worktree.longestPrefixMatch("/repo/src/a.swift", in: worktrees)?.branch, "master")
        XCTAssertNil(Worktree.longestPrefixMatch("/elsewhere/a.swift", in: worktrees))
    }

    /// Kern der Erkennung: jüngster Treffer gewinnt, Nicht-Treffer (fremder Pfad) werden übersprungen,
    /// ein Treffer in der Hauptarbeitskopie schaltet zurück auf nil, ein Repo ohne Nebenworktree bleibt immer nil.
    func testActive() {
        let main = Worktree.Entry(path: "/repo", branch: "master")
        let sibling = Worktree.Entry(path: "/repo-wt-1", branch: "feature-x")
        let worktrees = [main, sibling]

        XCTAssertEqual(Worktree.active(candidates: ["cd /repo-wt-1 && npm test"], in: worktrees), sibling)
        XCTAssertEqual(Worktree.active(candidates: ["/repo-wt-1/src/a.swift"], in: worktrees), sibling)
        // Fremder Pfad ohne Treffer wird übersprungen, der ältere Treffer im Nebenworktree bleibt aktiv.
        XCTAssertEqual(Worktree.active(candidates: ["/Users/dev/.claude/plans/x.md", "/repo-wt-1/a"], in: worktrees), sibling)
        // Treffer in der Hauptarbeitskopie ist jünger: zurück auf das Hauptverzeichnis.
        XCTAssertNil(Worktree.active(candidates: ["/repo/src/a.swift", "/repo-wt-1/a"], in: worktrees))
        // Ohne jeden Treffer: nichts bekannt, Hauptverzeichnis gilt.
        XCTAssertNil(Worktree.active(candidates: ["/Users/dev/.claude/plans/x.md"], in: worktrees))
        // Nur ein Worktree im Repo (kein Nebenworktree möglich): immer nil.
        XCTAssertNil(Worktree.active(candidates: ["/repo-wt-1/a"], in: [main]))
    }

    /// `SessionRegistry.applyActiveWorktree` lädt `git worktree list` nur neu, wenn der neue Kandidat von der
    /// gecachten Liste nicht mehr abgedeckt ist: `match` ist dafür kein `private` mehr, dieser Test hält die
    /// Erwartung an dessen Verhalten fest (Datei-Präfix wie Bash-Kommando).
    func testMatchCoversPathAndCommandCandidates() {
        let worktrees = [Worktree.Entry(path: "/repo", branch: "master"), Worktree.Entry(path: "/repo-wt-1", branch: "feature-x")]
        XCTAssertNotNil(Worktree.match("/repo-wt-1/src/a.swift", in: worktrees))
        XCTAssertNotNil(Worktree.match("cd /repo-wt-1 && npm test", in: worktrees))
        XCTAssertNil(Worktree.match("/elsewhere/a.swift", in: worktrees))
    }
}
