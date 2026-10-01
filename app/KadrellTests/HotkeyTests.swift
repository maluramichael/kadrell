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

    private func key(_ chars: String, _ plain: String, _ mods: NSEvent.ModifierFlags, code: UInt16 = 23) throws -> NSEvent {
        try XCTUnwrap(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: 0, windowNumber: 0, context: nil,
                                       characters: chars, charactersIgnoringModifiers: plain, isARepeat: false, keyCode: code))
    }

    /// QWERTZ: ⌥5 tippt „[“, das gehört dem Prompt und nicht „Kachel 5“.
    func testOptionCharacterYieldsToTyping() throws {
        let bracket = try key("[", "5", .option)
        XCTAssertEqual(Hotkey(event: bracket), HotkeyAction.focus5.defaultKey)
        XCTAssertTrue(Hotkey.yieldsToTyping(bracket))
        XCTAssertNil(Hotkeys.action(for: bracket))
        XCTAssertTrue(Hotkey.yieldsToTyping(try key("\\", "7", [.option, .shift])))
        XCTAssertFalse(Hotkey.yieldsToTyping(try key("∞", "5", .option)))
        XCTAssertEqual(Hotkeys.action(for: try key("∞", "5", .option)), .focus5)
        XCTAssertFalse(Hotkey.yieldsToTyping(try key("", "", [.option, .numericPad, .function], code: 123)))
        XCTAssertFalse(Hotkey.yieldsToTyping(try key("[", "5", [.option, .command])))
        XCTAssertFalse(Hotkey.yieldsToTyping(try key("[", "5", [.option, .control])))
        XCTAssertFalse(Hotkey.yieldsToTyping(try key("", "n", .option, code: 45)))
        XCTAssertFalse(Hotkey.yieldsToTyping(try key("[", "5", [])))
    }
}
