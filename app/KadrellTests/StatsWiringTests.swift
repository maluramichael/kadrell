import XCTest
@testable import Kadrell

/// Dass die Zähler an den richtigen Stellen hochgehen, nicht nur dass sie hochgehen können.
final class StatsWiringTests: XCTestCase {
    private let suite = "de.malura.kadrell.tests.statswiring"
    private var dir: URL!

    override func setUp() {
        super.setUp()
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = UserDefaults(suiteName: suite)!
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("statswiring-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: dir)
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = Profile.defaults
        super.tearDown()
    }

    @MainActor private func registry() -> SessionRegistry {
        SessionRegistry(cli: ClaudeCLI(binary: "/usr/bin/false", environment: [:]),
                        url: dir.appendingPathComponent("sessions.json"))
    }

    private func session(id: String) -> Session {
        Session(id: id, cwd: dir.path, startedAt: 0, sessionId: id, name: "")
    }

    @MainActor func testAddingSessionCountsAsSession() {
        registry().add(session(id: "abc"))
        XCTAssertEqual(Stats.count(.sessions), 1)
        XCTAssertEqual(Stats.count(.terminals), 0)
    }

    @MainActor func testAddingShellCountsAsTerminal() {
        registry().add(session(id: Session.shellPrefix + "one"))
        XCTAssertEqual(Stats.count(.terminals), 1)
        XCTAssertEqual(Stats.count(.sessions), 0)
    }

    /// Eine Id, die es schon gibt, wird verworfen und darf auch nicht mitzählen.
    @MainActor func testDuplicateIsNotCounted() {
        let r = registry()
        r.add(session(id: "abc"))
        r.add(session(id: "abc"))
        XCTAssertEqual(Stats.count(.sessions), 1)
    }

    @MainActor func testRecordFollowsNumberOfOpenSessions() {
        let r = registry()
        r.add(session(id: "a"))
        r.add(session(id: "b"))
        XCTAssertEqual(Stats.record.count, 2)
        r.remove(["a", "b"])
        r.add(session(id: "c"))
        XCTAssertEqual(Stats.record.count, 2)
    }
}
