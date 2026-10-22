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

    /// Ohne eigene Angabe ist der Akzent die Arbeitsfarbe, die zehn älteren Schemata sehen also unverändert aus.
    func testAccentDefaultsToRunning() {
        for t in ColorTheme.all where !t.id.hasPrefix("instrument") {
            XCTAssertEqual(t.accent, t.running, t.id)
        }
    }

    func testInstrumentThemes() throws {
        let dark = ColorTheme.named("instrument-dark"), light = ColorTheme.named("instrument-light")
        XCTAssertEqual(dark.id, "instrument-dark"); XCTAssertTrue(dark.dark)
        XCTAssertEqual(light.id, "instrument-light"); XCTAssertFalse(light.dark)
        XCTAssertEqual(dark.panel.hexString, "#151514"); XCTAssertEqual(dark.bg.hexString, "#1c1c1a")
        XCTAssertEqual(dark.waiting.hexString, "#ff5e14"); XCTAssertEqual(dark.accent.hexString, "#ff8a4c")
        XCTAssertEqual(light.panel.hexString, "#e3e1db"); XCTAssertEqual(light.bg.hexString, "#f2f1ec")
        XCTAssertEqual(light.waiting.hexString, "#ff4f00"); XCTAssertEqual(light.accent.hexString, "#a83500")
        XCTAssertEqual(ColorTheme.all[0].id, "mocha")
    }

    /// Akzent als Text (Links, Abschnittstitel) mindestens 4,5:1 auf panel und surface.
    func testAccentTextMeetsAA() {
        for t in ColorTheme.all where t.id.hasPrefix("instrument") {
            for bg in [t.panel, t.surface, t.bg] {
                XCTAssertGreaterThanOrEqual(t.accent.contrastRatio(with: bg), 4.5, "\(t.id): accent gegen \(bg.hexString)")
            }
        }
    }

    /// Die Terminalfarben 1 bis 6 (Rot bis Cyan) sind in den Instrument-Schemata als Text auf dem Terminalgrund lesbar.
    func testInstrumentAnsiMeetsAA() {
        for t in ColorTheme.all where t.id.hasPrefix("instrument") {
            for (i, c) in t.ansi.enumerated() where (1...6).contains(i) {
                XCTAssertGreaterThanOrEqual(c.contrastRatio(with: t.bg), 4.5, "\(t.id): ansi[\(i)] \(c.hexString) gegen \(t.bg.hexString)")
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
}
