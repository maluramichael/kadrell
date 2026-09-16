import XCTest
import SwiftTerm
@testable import Kadrell

@MainActor
final class SyncProbe: TerminalViewDelegate {
    var got: [UInt8] = []
    func send(source: TerminalView, data: ArraySlice<UInt8>) { got += data }
    func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {}
    func setTerminalTitle(source: TerminalView, title: String) {}
    func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
    func scrolled(source: TerminalView, position: Double) {}
    func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {}
    func bell(source: TerminalView) {}
    func clipboardCopy(source: TerminalView, content: Data) {}
    func clipboardRead(source: TerminalView) -> Data? { nil }
    func iTermContent(source: TerminalView, content: ArraySlice<UInt8>) {}
    func rangeChanged(source: TerminalView, startY: Int, endY: Int) {}
}

@MainActor
final class SyncInputTests: XCTestCase {
    func testForwardEncodesKeysForTerminalWithoutKeyboard() {
        let t = TerminalView(frame: NSRect(x: 0, y: 0, width: 400, height: 200))
        let p = SyncProbe()
        t.terminalDelegate = p
        func bytes(_ chars: String, _ ign: String, _ code: UInt16, _ mods: NSEvent.ModifierFlags = []) -> [UInt8] {
            p.got = []
            let e = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: 0, windowNumber: 0, context: nil,
                                     characters: chars, charactersIgnoringModifiers: ign, isARepeat: false, keyCode: code)!
            KadrellTerminalView.forward(e, to: t)
            return p.got
        }
        let up = String(Character(UnicodeScalar(NSUpArrowFunctionKey)!))
        XCTAssertEqual(bytes("a", "a", 0), [97])
        XCTAssertEqual(bytes("@", "l", 37, .option), [64])
        XCTAssertEqual(bytes("\r", "\r", 36), [13])
        XCTAssertEqual(bytes("\u{7f}", "\u{7f}", 51), [127])
        XCTAssertEqual(bytes("\u{3}", "c", 8, .control), [3])
        XCTAssertEqual(bytes(up, up, 126, [.function, .numericPad]), [27, 91, 65])
    }
}
