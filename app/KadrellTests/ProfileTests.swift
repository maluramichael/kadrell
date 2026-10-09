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
}
