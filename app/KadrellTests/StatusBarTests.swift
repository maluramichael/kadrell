import XCTest
@testable import Kadrell

@MainActor
final class StatusBarTests: XCTestCase {
    private func bar(width: CGFloat = 1100) -> (StatusBarView, NSWindow) {
        let bar = StatusBarView(frame: .zero)
        let w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: width, height: 30), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        bar.frame = NSRect(x: 0, y: 0, width: width, height: 30)
        w.contentView = bar
        bar.crumb = (group: "Projekt", session: "Alpha")
        return (bar, w)
    }

    private func children(_ v: NSView) -> [NSAccessibilityElement] { (v.accessibilityChildren() ?? []).compactMap { $0 as? NSAccessibilityElement } }

    func testErrorModuleWithCrumbAndClick() throws {
        let (bar, w) = bar()
        defer { w.close() }
        bar.errorText = "Sessions können nicht gespeichert werden"
        var shown = 0
        bar.onShowError = { shown += 1 }
        bar.display()
        let el = try XCTUnwrap(children(bar).first { $0.accessibilityLabel() == "Fehler" })
        XCTAssertEqual(el.accessibilityValue() as? String, "Sessions können nicht gespeichert werden")
        XCTAssertTrue(el.accessibilityPerformPress())
        XCTAssertEqual(shown, 1)
    }

    func testLongErrorTruncatedAndCoexistsWithVersionWarning() throws {
        let (bar, w) = bar()
        defer { w.close() }
        bar.errorText = String(repeating: "x", count: 300)
        bar.versionWarning = "1.0"
        bar.display()
        let labels = children(bar).compactMap { $0.accessibilityLabel() }
        XCTAssertTrue(labels.contains("Fehler"))
        XCTAssertTrue(labels.contains("Ältere claude-Version"))
        let err = try XCTUnwrap(children(bar).first { $0.accessibilityLabel() == "Fehler" })
        XCTAssertEqual((err.accessibilityValue() as? String)?.count, 300, "voller Text bleibt im Wert")
    }

    func testNoErrorNoElementAndCenterShowsCount() {
        let (bar, w) = bar()
        defer { w.close() }
        bar.crumb = nil
        bar.sessionCount = 3
        bar.display()
        let labels = children(bar).compactMap { $0.accessibilityLabel() }
        XCTAssertFalse(labels.contains("Fehler"))
        XCTAssertTrue(labels.contains("kadrell · 3 sessions"))
    }

    func testUsageClockAndCrumbAreReadable() throws {
        let (bar, w) = bar()
        defer { w.close() }
        bar.usage = Usage(session: 10, weekly: 40, fable: nil)
        bar.display()
        let texts = (bar.accessibilityChildren() ?? []).compactMap { $0 as? NSAccessibilityElement }.filter { $0.accessibilityRole() == .staticText }
        let labels = texts.compactMap { $0.accessibilityLabel() }
        XCTAssertTrue(labels.contains { $0.contains("7 Tage") && $0.contains("40") }, "\(labels)")
        XCTAssertTrue(labels.contains { $0.hasPrefix("Uhrzeit") })
        XCTAssertTrue(labels.contains("Projekt › Alpha"))
    }

    func testSecondsToNextMinute() {
        XCTAssertEqual(StatusBarView.secondsToNextMinute(Date(timeIntervalSince1970: 120 + 45)), 15, accuracy: 0.001)
    }
}
