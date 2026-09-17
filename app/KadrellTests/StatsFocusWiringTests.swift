import XCTest
@testable import Kadrell

/// Kachelwechsel zählt nur, wenn der Fokus wirklich auf eine andere Session springt, nicht bei jedem Relayout.
@MainActor
final class StatsFocusWiringTests: XCTestCase {
    private let suite = "de.malura.kadrell.tests.statsfocus"

    func testSwitchingTilesCountsOncePerSwitch() {
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = UserDefaults(suiteName: suite)!
        let a = Session.shellPrefix + "focus-a", b = Session.shellPrefix + "focus-b"
        let sessions = [a, b].map { Session(id: $0, cwd: NSTemporaryDirectory(), startedAt: 0, sessionId: $0, name: "Shell") }
        let group = Group(id: "g", name: "Projekt", color: "#89b4fa", cwd: NSTemporaryDirectory(), sessionIds: [a, b], favorite: false)
        let attach = AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ProcessInfo.processInfo.environment))
        let ws = WorkspaceView(frame: NSRect(x: 0, y: 0, width: 600, height: 400), defaultsSuffix: ".test-focus")
        defer {
            attach.detachAll(); ws.close()
            for k in ["workspace.selected", "workspace.mode", "workspace.auto"] { Profile.defaults.removeObject(forKey: k + ".test-focus") }
            UserDefaults.standard.removePersistentDomain(forName: suite)
            Stats.defaults = Profile.defaults
        }
        ws.attach = attach
        ws.reload(groups: [group], sessions: sessions)
        ws.select([a, b], add: false, takeKeyboard: false)

        ws.setFocus(a, takeKeyboard: false)
        let before = Stats.count(.focusSwitches)
        ws.setFocus(b, takeKeyboard: false)
        XCTAssertEqual(Stats.count(.focusSwitches), before + 1, "Wechsel auf eine andere Kachel zählt")
        ws.setFocus(b, takeKeyboard: false)
        XCTAssertEqual(Stats.count(.focusSwitches), before + 1, "derselbe Fokus noch einmal zählt nicht")

        // Ein Klick im Baum zeigt eine andere Session, ohne über setFocus zu gehen. Aus Sicht des Nutzers
        // ist das derselbe Wechsel und muss genauso zählen.
        ws.select([a], add: false, takeKeyboard: false)
        XCTAssertEqual(Stats.count(.focusSwitches), before + 2, "Auswahl im Baum zählt auch")
        ws.select([a], add: false, takeKeyboard: false)
        XCTAssertEqual(Stats.count(.focusSwitches), before + 2, "dieselbe Session noch einmal zählt nicht")
    }
}
