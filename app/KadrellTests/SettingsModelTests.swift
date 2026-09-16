import XCTest
@testable import Kadrell

@MainActor
final class SettingsModelTests: XCTestCase {
    /// Änderungen gelten ohne Speichern. Der Wert wird am Ende zurückgesetzt, der Test läuft gegen die echten Defaults.
    func testChangesApplyImmediately() async throws {
        let original = Settings.stackShowPath
        let model = SettingsModel()
        var applied = 0
        model.onApply = { applied += 1 }
        model.stackShowPath = !original
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(Settings.stackShowPath, !original)
        model.stackShowPath = original
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(Settings.stackShowPath, original)
        XCTAssertEqual(applied, 2)
    }
}
