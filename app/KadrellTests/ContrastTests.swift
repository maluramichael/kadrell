import XCTest
@testable import Kadrell

/// WCAG-AA (4,5:1) für Sekundärtext und die Warte-Beschriftung in allen Farbschemata, gegen jede Fläche, auf der
/// sie tatsächlich gezeichnet werden (`panel`, `surface`). Ein neues Schema, das hier durchfällt, ist ein Bug,
/// keine Geschmacksfrage.
final class ContrastTests: XCTestCase {
    func testSecondaryTextMeetsAA() {
        for t in ColorTheme.all {
            for bg in [t.panel, t.surface] {
                XCTAssertGreaterThanOrEqual(t.sub.contrastRatio(with: bg), 4.5, "\(t.id): sub gegen \(bg.hexString)")
                XCTAssertGreaterThanOrEqual(t.muted.contrastRatio(with: bg), 4.5, "\(t.id): muted gegen \(bg.hexString)")
                XCTAssertGreaterThanOrEqual(t.waitingText.contrastRatio(with: bg), 4.5, "\(t.id): waitingText gegen \(bg.hexString)")
            }
        }
    }

    /// `pillText` liefert für jede Statusfarbe (Beschriftung auf gefüllten Badges) mindestens AA.
    func testPillTextMeetsAA() {
        let saved = Theme.current
        defer { Theme.current = saved }
        for t in ColorTheme.all {
            Theme.current = t
            for status: NSColor in [t.running, t.waiting, t.idle, t.error] {
                XCTAssertGreaterThanOrEqual(Theme.pillText(on: status).contrastRatio(with: status), 4.5, "\(t.id): pillText gegen \(status.hexString)")
            }
        }
    }

    /// `ensuringContrast` lässt eine schon konforme Farbe unangetastet (kein unnötiger Charakterverlust).
    func testEnsuringContrastNoopsWhenAlreadyCompliant() {
        let bg = NSColor(hex: 0x000000), fg = NSColor(hex: 0xffffff)
        XCTAssertEqual(fg.ensuringContrast(4.5, against: [bg], toward: NSColor(hex: 0x808080)), fg)
    }
}
