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
        model.binding(\.stackShowPath).wrappedValue = !original
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(Settings.stackShowPath, !original)
        model.binding(\.stackShowPath).wrappedValue = original
        try await Task.sleep(for: .milliseconds(50))
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
        try await Task.sleep(for: .milliseconds(50))
        Settings.stackShowPath = original
        XCTAssertEqual(box.appliedAtNotify, true)
    }

    // MARK: Farbschema: Instrument für neue Profile, bestehende behalten Mocha

    /// Eigene, leere Suite statt der Profil-Defaults; `tearDown` räumt sie samt plist wieder weg.
    private let themeSuite = "de.malura.kadrell.tests.theme"
    private func freshDefaults() -> UserDefaults {
        UserDefaults.standard.removePersistentDomain(forName: themeSuite)
        return UserDefaults(suiteName: themeSuite)!
    }

    override func tearDown() {
        UserDefaults.standard.removePersistentDomain(forName: themeSuite)
        super.tearDown()
    }

    func testExistingProfileKeepsMocha() {
        let d = freshDefaults()
        d.set(true, forKey: "helpShown")
        Settings.pinThemeForExistingProfile(d)
        XCTAssertEqual(d.string(forKey: "colorTheme"), "mocha")
    }

    func testFreshProfileGetsInstrument() {
        let d = freshDefaults()
        Settings.pinThemeForExistingProfile(d)
        XCTAssertNil(d.string(forKey: "colorTheme"))
    }

    func testChosenThemeIsUntouched() {
        let d = freshDefaults()
        d.set(true, forKey: "helpShown"); d.set("nord", forKey: "colorTheme")
        Settings.pinThemeForExistingProfile(d)
        XCTAssertEqual(d.string(forKey: "colorTheme"), "nord")
    }

    /// Ohne gespeicherten Wert gilt Instrument Dark. Der Wert des Profils wird am Ende wiederhergestellt.
    func testDefaultThemeIsInstrumentDark() {
        let saved = Profile.defaults.object(forKey: "colorTheme")
        defer { Profile.defaults.set(saved, forKey: "colorTheme") }
        Profile.defaults.removeObject(forKey: "colorTheme")
        XCTAssertEqual(Settings.colorTheme, "instrument-dark")
    }
}
