import XCTest
import SwiftUI
@testable import Kadrell

/// `/` in Einstellungen und Hilfe: Treffer nach Wörtern, Tasten, und die gefilterten Einstellungen.
@MainActor
final class ListFilterTests: XCTestCase {
    func testMatchesAllWordsIgnoringCaseAndUmlauts() {
        XCTAssertTrue(ListFilter.matches("Terminal-Schriftgröße", query: "schriftgrosse"))
        XCTAssertTrue(ListFilter.matches("Fertige Sessions automatisch trennen", query: "sessions TRENNEN"))
        XCTAssertFalse(ListFilter.matches("Fertige Sessions automatisch trennen", query: "sessions farbe"))
    }

    /// `/` öffnet das Feld, außer in einem Textfeld; Esc beendet erst den Filter, danach schließt es den Dialog.
    func testSlashOpensAndEscEndsBeforeItClosesTheDialog() {
        let filter = ListFilter()
        XCTAssertFalse(filter.handle(key("\u{1b}", code: 53), in: nil), "ohne Filter geht Esc an den Dialog")
        XCTAssertTrue(filter.handle(key("/", code: 44), in: nil))
        XCTAssertTrue(filter.isOpen)
        filter.query = "farbe"
        XCTAssertTrue(filter.isActive)
        XCTAssertTrue(filter.handle(key("\u{1b}", code: 53), in: nil), "Esc beendet den Filter")
        XCTAssertFalse(filter.isOpen)
        XCTAssertEqual(filter.query, "")
        XCTAssertFalse(filter.handle(key("a", code: 0), in: nil))
        let field = NSTextField(), window = NSWindow(contentRect: NSRect(x: -20000, y: -20000, width: 200, height: 100), styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = field
        window.makeFirstResponder(field)
        XCTAssertFalse(filter.handle(key("/", code: 44), in: window), "im Textfeld ist / ein Zeichen")
        window.close()
    }

    /// Gefilterte Einstellungen zeigen nur die passenden Zeilen, ohne Treffer gar keine.
    func testSettingsShowOnlyMatchingRows() {
        let all = wells(filter: ListFilter())
        let some = ListFilter(); some.open(); some.query = "Farbschema"
        let none = ListFilter(); none.open(); none.query = "qqqxyz"
        XCTAssertGreaterThan(all, 10)
        XCTAssertEqual(wells(filter: some), 2, "Farbschema und das Suchfeld")
        XCTAssertEqual(wells(filter: none), 1, "nur das Suchfeld")
    }

    /// Gefilterte Hilfe wird kürzer, ohne Treffer bleibt nur der Kopf mit dem Suchfeld.
    func testHelpShrinksWithFilter() {
        let some = ListFilter(); some.open(); some.query = "Statistik"
        let none = ListFilter(); none.open(); none.query = "qqqxyz"
        let all = helpHeight(ListFilter()), few = helpHeight(some), zero = helpHeight(none)
        XCTAssertGreaterThan(all, few + 200)
        XCTAssertGreaterThan(few, zero)
    }

    private func helpHeight(_ filter: ListFilter) -> CGFloat {
        let host = NSHostingView(rootView: AboutView(filter: filter).frame(width: 640))
        host.layoutSubtreeIfNeeded()
        return host.fittingSize.height
    }

    private func key(_ chars: String, code: UInt16) -> NSEvent {
        NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil,
                         characters: chars, charactersIgnoringModifiers: chars, isARepeat: false, keyCode: code)!
    }

    private final class KeyWindow: NSWindow { override var canBecomeKey: Bool { true } }

    private func wells(filter: ListFilter) -> Int {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let window = KeyWindow(contentRect: NSRect(x: -20000, y: -20000, width: 1400, height: 3000), styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: SettingsView(model: SettingsModel(), filter: filter).frame(width: 1400))
        host.frame = NSRect(x: 0, y: 0, width: 1400, height: 3000)
        window.contentView = host
        window.makeKeyAndOrderFront(nil)
        for _ in 0..<4 {
            host.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date() + 0.05)
        }
        defer { window.close() }
        return surfaces(in: host).filter { $0.part.isWell }.count
    }
}
