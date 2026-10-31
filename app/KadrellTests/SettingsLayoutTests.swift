import XCTest
import SwiftUI
@testable import Kadrell

/// Einstellungen in Spalten: ausgewogen geteilt, Reihenfolge bleibt, Tab läuft erst links, dann rechts.
@MainActor
final class SettingsLayoutTests: XCTestCase {
    /// Teilung in Reihenfolge: links die ersten Abschnitte, so dass die längere Spalte möglichst kurz ist.
    func testSplitKeepsOrderAndBalancesColumns() {
        XCTAssertEqual(ColumnBalance.split([100, 100, 100, 100]), 2)
        XCTAssertEqual(ColumnBalance.split([300, 100, 100, 100]), 1)
        XCTAssertEqual(ColumnBalance.split([100, 100, 100, 300]), 3)
        XCTAssertEqual(ColumnBalance.split([50]), 1, "ein Abschnitt bleibt links")
        XCTAssertEqual(ColumnBalance.split([]), 0)
        // Abstände zählen mit: drei kleine Abschnitte links wiegen einen großen rechts auf.
        XCTAssertEqual(ColumnBalance.split([100, 100, 100, 330], spacing: 20), 3)
    }

    func testTwoColumnsOnlyFromMinimumWidth() {
        XCTAssertEqual(ColumnBalance.count(width: 1199, minColumnWidth: 600), 1)
        XCTAssertEqual(ColumnBalance.count(width: 1200, minColumnWidth: 600), 2)
        XCTAssertEqual(ColumnBalance.count(width: 0, minColumnWidth: 600), 1)
    }

    /// Steuerelemente wachsen mit der Spalte bis 300 pt und werden in schmalen Spalten schmaler, nie unter 120 pt.
    func testControlWidthFollowsColumn() {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        XCTAssertEqual(ColumnBalance.controlWidth(forColumn: 0), 300)
        XCTAssertEqual(ColumnBalance.controlWidth(forColumn: 2000), 300)
        XCTAssertLessThan(ColumnBalance.controlWidth(forColumn: 600), 300)
        XCTAssertEqual(ColumnBalance.controlWidth(forColumn: 200), 120)
    }

    /// Tab folgt der Reihenfolge der Abschnitte: erst die linke Spalte von oben nach unten, dann die rechte. Ohne
    /// eigenen Fokusbereich je Spalte liefe Tab zeilenweise quer (A, C, B, D).
    func testTabRunsDownLeftColumnThenRight() throws {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let field = { (n: String) in TextField(n, text: .constant(n)).frame(height: 100) }
        let (window, host) = hosted(SettingsColumns { field("A"); field("B"); field("C"); field("D") }.frame(width: 1300), size: NSSize(width: 1300, height: 400))
        defer { window.close() }
        let fields = textFields(in: host).sorted { $0.placeholderString ?? "" < $1.placeholderString ?? "" }
        XCTAssertEqual(fields.count, 4)
        let frames = fields.map { $0.convert($0.bounds, to: nil) }
        XCTAssertEqual(frames[0].minX, frames[1].minX, accuracy: 1, "A über B")
        XCTAssertLessThanOrEqual(frames[0].maxX, frames[2].minX + 0.5, "C rechts")
        XCTAssertEqual(frames[0].maxY, frames[2].maxY, accuracy: 1, "A und C oben auf einer Höhe")
        window.makeFirstResponder(fields[0])
        var order = [focused(window, fields)]
        for _ in 0..<3 {
            window.sendEvent(NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber,
                                              context: nil, characters: "\t", charactersIgnoringModifiers: "\t", isARepeat: false, keyCode: 48)!)
            RunLoop.main.run(until: Date() + 0.05)
            order.append(focused(window, fields))
        }
        XCTAssertEqual(order, ["A", "B", "C", "D"])
    }

    /// Breit zwei Spalten, schmal eine: die Felder beginnen je Spalte an derselben Kante.
    func testSettingsUseTwoColumnsWhenWide() {
        let wide = wellEdges(width: 1400)
        XCTAssertEqual(wide.count, 2, "zwei Spalten: \(wide)")
        let narrow = wellEdges(width: 900)
        XCTAssertEqual(narrow.count, 1, "eine Spalte: \(narrow)")
    }

    // MARK: Helfer

    /// Linke Kanten aller Felder der Einstellungen bei `width`, gerundet.
    private func wellEdges(width: CGFloat) -> Set<CGFloat> {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let (window, host) = hosted(SettingsView(model: SettingsModel()).frame(width: width), size: NSSize(width: width, height: 3000))
        defer { window.close() }
        let wells = surfaces(in: host).filter { $0.part.isWell }.map { $0.convert($0.bounds, to: host).minX.rounded() }
        return Set(wells)
    }

    private final class KeyWindow: NSWindow { override var canBecomeKey: Bool { true } }

    /// Rahmenloses Key-Fenster weit außerhalb des Bildschirms, Inhalt gelegt und mehrmals durch die RunLoop (die
    /// Breite wird gemessen, danach teilen sich die Spalten).
    private func hosted<V: View>(_ view: V, size: NSSize) -> (NSWindow, NSView) {
        let window = KeyWindow(contentRect: NSRect(origin: CGPoint(x: -20000, y: -20000), size: size), styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let host = NSHostingView(rootView: view)
        host.frame = NSRect(origin: .zero, size: size)
        window.contentView = host
        window.makeKeyAndOrderFront(nil)
        for _ in 0..<3 {
            host.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date() + 0.05)
        }
        return (window, host)
    }

    private func focused(_ window: NSWindow, _ fields: [NSTextField]) -> String {
        let r = window.firstResponder
        let hit = fields.first { $0 === r || ($0.currentEditor() != nil && $0.currentEditor() === r) }
        return hit?.placeholderString ?? "?"
    }

    private func textFields(in view: NSView) -> [NSTextField] {
        view.subviews.flatMap { sub in (sub as? NSTextField).map { [$0] } ?? textFields(in: sub) }
    }
}
