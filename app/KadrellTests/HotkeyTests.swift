import XCTest
@testable import Kadrell

final class HotkeyTests: XCTestCase {
    func testDefaultsAreUniqueAndUsable() {
        let keys = HotkeyAction.allCases.map(\.defaultKey)
        XCTAssertEqual(Set(keys).count, keys.count)
        for k in keys { XCTAssertTrue(k.isUsable, k.display) }
    }

    func testStringRoundtripAndDisplay() {
        let k = Hotkey([.option, .shift], "←")
        XCTAssertEqual(Hotkey(string: k.string), k)
        XCTAssertEqual(Hotkey(string: "\(NSEvent.ModifierFlags.option.rawValue)||"), Hotkey(.option, "|"))
        XCTAssertNil(Hotkey(string: ""))
        XCTAssertEqual(k.display, "⌥⇧←")
        XCTAssertEqual(Hotkey(.command, "Esc").menuEquivalent, "\u{1b}")
    }

    func testUsable() {
        XCTAssertFalse(Hotkey([], "a").isUsable)
        XCTAssertFalse(Hotkey(.shift, "a").isUsable)
        XCTAssertTrue(Hotkey([], "F5").isUsable)
        XCTAssertTrue(Hotkey(.control, "a").isUsable)
    }

    func testEventMatching() throws {
        let e = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [.option, .numericPad, .function], timestamp: 0,
                                               windowNumber: 0, context: nil, characters: "", charactersIgnoringModifiers: "", isARepeat: false, keyCode: 123))
        XCTAssertEqual(Hotkey(event: e), Hotkey(.option, "←"))
        let z = try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: .option, timestamp: 0,
                                               windowNumber: 0, context: nil, characters: "Ω", charactersIgnoringModifiers: "z", isARepeat: false, keyCode: 6))
        XCTAssertEqual(Hotkey(event: z), HotkeyAction.zoom.defaultKey)
    }

    func testTileIndex() {
        XCTAssertEqual(HotkeyAction.focus1.tileIndex, 0)
        XCTAssertEqual(HotkeyAction.focus9.tileIndex, 8)
        XCTAssertNil(HotkeyAction.focusLeft.tileIndex)
        XCTAssertNil(HotkeyAction.focusSidebar.tileIndex)
        XCTAssertEqual(HotkeyAction.focusSidebar.defaultKey, Hotkey(.command, "1"))
        XCTAssertEqual(HotkeyAction.nextLayout.defaultKey, Hotkey(.command, "l"))
    }
}
