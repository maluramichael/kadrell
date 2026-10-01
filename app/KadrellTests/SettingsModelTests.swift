import XCTest
@testable import Kadrell

@MainActor
final class SettingsModelTests: XCTestCase {
    /// Wartet auf die Bedingung statt auf eine feste Zeit, großzügige Deadline für ausgelastete Macs.
    private func waitUntil(_ condition: () -> Bool) async throws {
        let deadline = Date().addingTimeInterval(5)
        while !condition(), Date() < deadline { try await Task.sleep(for: .milliseconds(10)) }
    }

    func testOverlayWidthIsCappedToParentAndScreen() {
        XCTAssertEqual(OverlayPanel.maxWidth(parent: 700, screen: nil), 652)
        XCTAssertEqual(OverlayPanel.maxWidth(parent: 700, screen: 600), 552)
        XCTAssertEqual(OverlayPanel.maxWidth(parent: 700, screen: 1440), 652)
    }

    /// Änderungen gelten ohne Speichern. Der Wert wird am Ende zurückgesetzt, der Test läuft gegen die echten Defaults.
    func testChangesApplyImmediately() async throws {
        let original = Settings.stackShowPath
        let model = SettingsModel()
        var applied = 0
        model.onApply = { applied += 1 }
        model.binding(\.stackShowPath).wrappedValue = !original
        try await waitUntil { Settings.stackShowPath == !original }
        XCTAssertEqual(Settings.stackShowPath, !original)
        model.binding(\.stackShowPath).wrappedValue = original
        try await waitUntil { Settings.stackShowPath == original && applied == 2 }
        XCTAssertEqual(Settings.stackShowPath, original)
        XCTAssertEqual(applied, 2)
    }

    /// SwiftUI zeichnet neu, sobald der Beobachter benachrichtigt wird. Da muss die App das neue Theme schon angewendet haben,
    /// sonst zeigt der Dialog immer den Stand der vorigen Änderung.
    func testObserversNotifiedOnlyAfterApply() async throws {
        final class Box: @unchecked Sendable { var applied = false; var appliedAtNotify: Bool? }
        let original = Settings.stackShowPath
        let model = SettingsModel(), box = Box()
        model.onApply = { box.applied = true }
        withObservationTracking { _ = model.locale } onChange: { box.appliedAtNotify = box.applied }
        model.binding(\.stackShowPath).wrappedValue = !original
        try await waitUntil { box.appliedAtNotify != nil }
        Settings.stackShowPath = original
        XCTAssertEqual(box.appliedAtNotify, true)
    }
}
