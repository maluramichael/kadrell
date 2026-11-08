import XCTest
@testable import Kadrell

final class ProfileTests: XCTestCase {
    /// Der Relaunch-Helfer wartet auf das Ende der alten Instanz (Profil-Lock) und öffnet dann denselben Build.
    func testRelaunchCommandWaitsThenOpensSameBundle() {
        let c = Profile.relaunchCommand(pid: 4242, bundlePath: "/Applications/Kadrell.app", profile: "work")
        XCTAssertEqual(c[0], "/bin/sh")
        XCTAssertEqual(c[1], "-c")
        XCTAssertTrue(c[2].contains("kill -0 4242"), c[2])
        XCTAssertTrue(c[2].contains("/usr/bin/open '/Applications/Kadrell.app' --args --profile work"), c[2])
    }

    /// Standardprofil: kein --profile-Argument. Pfad mit Leerzeichen bleibt in Anführungszeichen.
    func testRelaunchCommandDefaultProfileAndSpaces() {
        let c = Profile.relaunchCommand(pid: 1, bundlePath: "/My Apps/Kadrell.app", profile: nil)
        XCTAssertTrue(c[2].contains("open '/My Apps/Kadrell.app'"), c[2])
        XCTAssertFalse(c[2].contains("--profile"), c[2])
    }

    /// Nach dem Beenden startet die CLI die App zehn Minuten nicht neu; ein Neustart der App räumt den Marker weg.
    func testQuitMarkerBlocksOnlyForTheWindowAndClears() {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-marker-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let t = Date()
        XCTAssertFalse(Profile.quitRecently(in: dir, now: t))
        Profile.markQuit(in: dir, now: t)
        XCTAssertTrue(Profile.quitRecently(in: dir, now: t.addingTimeInterval(5)))
        XCTAssertFalse(Profile.quitRecently(in: dir, now: t.addingTimeInterval(Profile.quitMarkerWindow + 1)))
        Profile.clearQuitMarker(in: dir)
        XCTAssertFalse(Profile.quitRecently(in: dir, now: t.addingTimeInterval(5)))
    }
}
