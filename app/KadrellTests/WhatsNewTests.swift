import XCTest
@testable import Kadrell

final class WhatsNewTests: XCTestCase {
    private let changelog = """
    # Changelog

    Neueste Version oben.

    ## 1.2.0 (2026-01-02)

    - Feature: Zeile eins.
    - Fix: Zeile zwei.

    ## 1.1.0 (2026-01-01)

    - Feature: Alte Version.
    """

    func testNotesOfCurrentVersion() {
        XCTAssertEqual(WhatsNew.notes(version: "1.2.0", changelog: changelog), ["Feature: Zeile eins.", "Fix: Zeile zwei."])
    }

    func testNotesOfOlderVersion() {
        XCTAssertEqual(WhatsNew.notes(version: "1.1.0", changelog: changelog), ["Feature: Alte Version."])
    }

    func testNotesOfUnknownVersionAreEmpty() {
        XCTAssertEqual(WhatsNew.notes(version: "9.9.9", changelog: changelog), [])
    }

    /// "1.2.0" darf nicht zufällig auf "1.2.0-beta" o. ä. matchen.
    func testVersionMatchIsExact() {
        let ambiguous = "## 1.2\n\n- Feature: falsche Version.\n\n## 1.2.0\n\n- Feature: richtige Version.\n"
        XCTAssertEqual(WhatsNew.notes(version: "1.2.0", changelog: ambiguous), ["Feature: richtige Version."])
    }
}
