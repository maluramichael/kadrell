import XCTest
@testable import Kadrell

/// Auswahl vor dem Start: zuletzt gewählter Agent oben, Tippen filtert über den Namen, ⏎ meldet die markierte Zeile.
@MainActor
final class AgentPickTests: XCTestCase {
    private let options = [AgentPickModel.Option(kind: .claude, detail: "opus · bypassPermissions"),
                           AgentPickModel.Option(kind: .pi, detail: "qwen3:4b")]

    func testLastChoiceComesFirstAndIsSelected() {
        XCTAssertEqual(AgentPickModel(folder: "/p", options: options, last: .claude).list.map(\.kind), [.claude, .pi])
        let m = AgentPickModel(folder: "/p", options: options, last: .pi)
        XCTAssertEqual(m.list.map(\.kind), [.pi, .claude])
        var picked: AgentKind?
        m.onPick = { picked = $0 }
        m.start()
        XCTAssertEqual(picked, .pi, "⏎ ohne Tippen nimmt den zuletzt gewählten")
    }

    func testArrowsMoveAndStayInside() {
        let m = AgentPickModel(folder: "/p", options: options, last: .claude)
        var picked: AgentKind?
        m.onPick = { picked = $0 }
        m.move(-1)
        XCTAssertEqual(m.selected, 0)
        m.move(1); m.move(1)
        XCTAssertEqual(m.selected, 1)
        m.start()
        XCTAssertEqual(picked, .pi)
    }

    /// Nur der Name zählt: „pi“ trifft nicht Claude, obwohl dessen Startflags p und i enthalten.
    func testTypingFiltersByNameOnly() {
        let m = AgentPickModel(folder: "/p", options: options, last: .claude)
        m.move(1)
        m.query = "pi"
        XCTAssertEqual(m.list.map(\.kind), [.pi])
        XCTAssertEqual(m.selected, 0, "Markierung springt auf den ersten Treffer")
        m.query = "cl"
        XCTAssertEqual(m.list.map(\.kind), [.claude])
        m.query = "xyz"
        XCTAssertTrue(m.list.isEmpty)
        var picked: AgentKind?
        m.onPick = { picked = $0 }
        m.start()
        XCTAssertNil(picked, "ohne Treffer startet nichts")
        m.query = ""
        XCTAssertEqual(m.list.map(\.kind), [.claude, .pi])
    }

    /// Wer im Schnellstart schon Pi gewählt hatte, findet Pi oben, bis zur ersten eigenen Wahl.
    func testLastAgentFallsBackToOldQuickStartChoice() {
        let keys = ["lastAgent", "local.quickStart.usePi"]
        let saved = keys.map { Profile.defaults.object(forKey: $0) }
        defer { for (k, v) in zip(keys, saved) { Profile.defaults.set(v, forKey: k) } }
        keys.forEach { Profile.defaults.removeObject(forKey: $0) }
        XCTAssertEqual(Settings.lastAgent, .claude)
        Profile.defaults.set(true, forKey: "local.quickStart.usePi")
        XCTAssertEqual(Settings.lastAgent, .pi)
        Settings.lastAgent = .claude
        XCTAssertEqual(Settings.lastAgent, .claude)
    }
}
