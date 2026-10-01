import XCTest
@testable import Kadrell

/// Kachel-Beschriftung und Dialog-Buttons erreichen in allen Farbschemata mindestens 4,5:1.
@MainActor
final class CellContrastTests: XCTestCase {
    private func eachTheme(_ body: (ColorTheme) -> Void) {
        let saved = Theme.current
        defer { Theme.current = saved }
        for t in ColorTheme.all { Theme.current = t; body(t) }
    }

    func testShadeLabelsReadable() {
        eachTheme { t in
            for alpha: CGFloat in [0.35, 0.72] {
                let surfaces = CellView.shadeSurfaces(alpha: alpha, group: Theme.muted)
                for c in [Theme.muted, Theme.error] {
                    let label = CellView.shadeLabel(c, alpha: alpha, group: Theme.muted)
                    for s in surfaces { XCTAssertGreaterThanOrEqual(label.contrastRatio(with: s), 4.5, "\(t.id): \(c.hexString) auf \(s.hexString)") }
                }
            }
        }
    }

    func testHeaderTextReadable() {
        eachTheme { t in
            for hex in Theme.palette {
                let g = Theme.group(hex)
                for f: CGFloat in [0.12, 0.22] {
                    let head = g.mixed(f, into: Theme.surface)
                    for c in [Theme.muted, g] {
                        XCTAssertGreaterThanOrEqual(CellView.headText(c, on: head).contrastRatio(with: head), 4.5, "\(t.id): \(hex) bei \(f)")
                    }
                }
            }
        }
    }

    func testOnColorReadable() {
        eachTheme { t in
            for fill in [Theme.running, Theme.error] {
                let text = NSColor(Theme.onColor(fill))
                XCTAssertGreaterThanOrEqual(text.contrastRatio(with: fill), 4.5, "\(t.id): \(fill.hexString)")
            }
        }
    }
}
