import XCTest
@testable import Kadrell

final class ProfileTests: XCTestCase {
    private func resolve(_ args: [String] = [], _ env: [String: String] = [:], underTest: Bool = false) -> (name: String?, invalid: String?) {
        Profile.resolveName(arguments: ["kadrell"] + args, environment: env, underTest: underTest)
    }

    func testValidName() {
        XCTAssertEqual(resolve(["--profile", "work-1.a_b"]).name, "work-1.a_b")
        XCTAssertNil(resolve(["--profile", "work"]).invalid)
    }

    func testInvalidNames() {
        for bad in ["a b", "a/b", "grün", "../x"] {
            let r = resolve(["--profile", bad])
            XCTAssertNil(r.name, bad)
            XCTAssertEqual(r.invalid, bad)
        }
        XCTAssertEqual(resolve([], ["KADRELL_PROFILE": "a b"]).invalid, "a b")
    }

    func testArgumentBeatsEnvironment() {
        XCTAssertEqual(resolve(["--profile", "a"], ["KADRELL_PROFILE": "b"]).name, "a")
        XCTAssertEqual(resolve([], ["KADRELL_PROFILE": "b"]).name, "b")
    }

    func testNothingGiven() {
        XCTAssertNil(resolve().name)
        XCTAssertNil(resolve().invalid)
        XCTAssertNil(resolve(["--profile"]).name)
        XCTAssertNil(resolve([], ["KADRELL_PROFILE": ""]).name)
        XCTAssertNil(resolve([], ["KADRELL_PROFILE": ""]).invalid)
    }

    func testUnderTest() {
        XCTAssertEqual(resolve(underTest: true).name, "tmp")
        XCTAssertEqual(resolve([], ["KADRELL_PROFILE": ""], underTest: true).name, "tmp")
        XCTAssertEqual(resolve(["--profile", "x"], underTest: true).name, "x")
        XCTAssertEqual(resolve(["--profile", "tmp"], underTest: true).name, "tmp")
        XCTAssertEqual(resolve(["--profile", "a b"], underTest: true).invalid, "a b")
    }

    func testCopiedPreferencesDropsState() {
        let gone = ["workspace.selected", "workspace.selectedFoo", "stats.total", "stats.", "autoswitch.enabled",
                    "sessions.unseen", "sidebar.flatOrder", "windows.open"]
        let keep = ["language", "hotkeys", "accounts.index", "helpShown"]
        let base = Dictionary(uniqueKeysWithValues: (gone + keep).map { ($0, 1 as Any) })
        let out = Profile.copiedPreferences(base)
        XCTAssertEqual(Set(out.keys), Set(keep))
    }

    func testTestsUseIsolatedProfile() {
        XCTAssertTrue(Profile.defaults !== UserDefaults.standard)
        XCTAssertTrue(Profile.directory.path.hasPrefix(FileManager.default.temporaryDirectory.path))
    }
}
