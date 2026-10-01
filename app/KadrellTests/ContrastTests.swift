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

    /// Alle Status-Textfarben erreichen AA gegen `panel` und `bg`.
    func testStatusTextMeetsAA() {
        for t in ColorTheme.all {
            for bg in [t.panel, t.bg] {
                for (name, c) in [("running", t.runningText), ("waiting", t.waitingText), ("idle", t.idleText), ("error", t.errorText)] {
                    XCTAssertGreaterThanOrEqual(c.contrastRatio(with: bg), 4.5, "\(t.id): \(name)Text gegen \(bg.hexString)")
                }
            }
        }
    }

    /// ANSI-Farbslots 1-6 und 9-14 (die hellen sind dieselben) erreichen AA gegen `bg`, Schwarz/Weiß bleibt roh.
    func testAnsiContrast() {
        for t in ColorTheme.all {
            for i in [1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 14] {
                XCTAssertGreaterThanOrEqual(t.ansi[i % 8].contrastRatio(with: t.bg), 4.5, "\(t.id): ansi \(i)")
            }
            for i in [0, 7] { XCTAssertEqual(t.ansi[i], t.ansiRaw[i], "\(t.id): ansi \(i) unverändert") }
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

    /// Der Metal-Terminal nimmt RGB als vormultipliziert: aus Weiß bei 30 % Deckkraft wird Grau mit 30 % Alpha.
    func testPremultipliedScalesRGBByAlpha() {
        let c = NSColor(srgbRed: 1, green: 0.5, blue: 0, alpha: 0.3).premultiplied
        XCTAssertEqual(c.redComponent, 0.3, accuracy: 0.001)
        XCTAssertEqual(c.greenComponent, 0.15, accuracy: 0.001)
        XCTAssertEqual(c.blueComponent, 0, accuracy: 0.001)
        XCTAssertEqual(c.alphaComponent, 0.3, accuracy: 0.001)
    }

    func testAttrsCacheMatchesFreshBuildAndFollowsSize() {
        let a = Theme.attrs(12, .red, bold: true, truncate: false)
        let font = a[.font] as? NSFont
        XCTAssertEqual(font?.pointSize, 12)
        XCTAssertEqual(font, Theme.font(12, bold: true))
        XCTAssertEqual((a[.paragraphStyle] as? NSParagraphStyle)?.lineBreakMode, .byWordWrapping)
        let b = Theme.attrs(12, .red)
        XCTAssertEqual((b[.paragraphStyle] as? NSParagraphStyle)?.lineBreakMode, .byTruncatingTail)
        let old = Theme.scale
        defer { Theme.scale = old }
        Theme.scale = old * 2
        XCTAssertEqual((Theme.attrs(24, .red)[.font] as? NSFont)?.pointSize, 24)
    }
}
