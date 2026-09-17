import XCTest
import SwiftTerm
@testable import Kadrell

@MainActor
final class ScrollbackTests: XCTestCase {
    /// SwiftTerm hält ohne Option nur 500 Zeilen. Mit der Einstellung bleibt mehr Verlauf, verkleinern kürzt ihn.
    func testScrollbackKeepsMoreThanDefault() async throws {
        var options = TerminalOptions.default
        options.scrollback = 3_000
        let t = TerminalView(frame: NSRect(x: 0, y: 0, width: 800, height: 400), options: options)
        t.feed(text: (1...2_500).map { "zeile \($0)" }.joined(separator: "\r\n"))
        func lines() -> [String] { String(decoding: t.getBufferAsData(kind: .normal), as: UTF8.self).components(separatedBy: "\n").filter { $0.hasPrefix("zeile") } }
        for _ in 0..<50 where lines().count < 2_500 { try await Task.sleep(for: .milliseconds(20)) }
        XCTAssertEqual(lines().first, "zeile 1")
        XCTAssertEqual(lines().count, 2_500)
        t.changeScrollback(1_000)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertLessThan(lines().count, 1_100)
        XCTAssertEqual(lines().last, "zeile 2500")
    }
}
