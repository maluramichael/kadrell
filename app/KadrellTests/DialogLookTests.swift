import XCTest
import SwiftUI
@testable import Kadrell

/// Aussehen der einzelnen Dialoge auf der gemeinsamen Hülle: kein eigener Grund (die erhabene Fläche scheint durch),
/// Werte in eingelassenen Feldern, Aktionen auf Tasten, Auswahl als Umriss statt Akzentbalken.
@MainActor
final class DialogLookTests: XCTestCase {
    private let tmp = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-dialoglook-\(UUID().uuidString)")

    override func setUpWithError() throws {
        try super.setUpWithError()
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: tmp)
        super.tearDown()
    }

    // MARK: ⌘N

    /// Rechts in der Titelzeile steht nichts: dort muss die erhabene Fläche durchscheinen, kein eigener Dialoggrund.
    /// Das Suchfeld ist ein eingelassenes Feld, Suchfeld, Liste und Zusammenfassung trennen Fugen.
    func testNewSessionOnRaisedSurface() {
        assertOnRaised(newSessionView(), at: { CGPoint(x: $0.width - 20, y: 14) })
        let parts = surfaceParts(newSessionView())
        XCTAssertTrue(parts.contains { $0.isWell }, "Suchfeld ist ein Feld")
        XCTAssertEqual(parts.filter { $0 == .hSeam }.count, 2, "Fugen statt Divider")
    }

    /// Ausgewählte Zeile: Fläche mit Umriss in `fg`, kein Akzentbalken mehr am linken Rand.
    func testNewSessionSelectionIsOutlineNotBar() {
        let t = ColorTheme.named("instrument-dark")
        XCTAssertTrue(surfaceParts(newSessionView()).contains(.row(outline: true, ring: false)))
        let s = snapshot(newSessionView().raisedPanel(), theme: t, scale: 1)
        // x = 2 liegt gleich innen am Fugenring der Fläche, früher mitten im 3 pt breiten Balken.
        let accentAt = stride(from: 30.0, to: s.size.height - 10, by: 1).filter { y in
            distance(pixel(s, at: CGPoint(x: 2, y: y)), t.accent) < 0.1
        }
        XCTAssertEqual(accentAt, [], "kein Akzentbalken links")
        // Umriss: Zeilenfläche rückt 6 pt ein, der Umriss deckt Pixel 6 ganz. Eine Spalte in `fg` von über 20 pt.
        let outline = stride(from: 30.0, to: s.size.height - 10, by: 1).filter { y in
            distance(pixel(s, at: CGPoint(x: 6.25, y: y)), t.fg) < 0.15
        }
        XCTAssertGreaterThan(outline.count, 20, "Umriss der Auswahl in fg")
    }

    // MARK: Rückfragen

    func testConfirmOnRaisedSurface() {
        assertOnRaised(ConfirmView(title: "T", info: "i", button: "OK", destructive: true, onConfirm: {}, onCancel: {}),
                       at: { CGPoint(x: $0.width - 20, y: 18) })
    }

    /// Abbrechen ist eine neutrale Taste, Bestätigen destruktiv rot oder primär im Akzent; reine Mitteilungen haben
    /// nur die Primärtaste.
    func testConfirmUsesKeys() {
        let confirm = { (destructive: Bool, infoOnly: Bool) in
            ConfirmView(title: "T", info: "i", button: "OK", destructive: destructive, infoOnly: infoOnly, onConfirm: {}, onCancel: {})
        }
        assertKeys(confirm(true, false), [.neutral, .destructive])
        assertKeys(confirm(false, false), [.neutral, .primary])
        assertKeys(confirm(false, true), [.primary])
    }

    // MARK: Gruppe bearbeiten, Umbenennen

    func testEditGroupOnRaisedSurface() {
        let view = { EditGroupView(model: EditGroupModel(group: self.group)) }
        assertOnRaised(view(), at: { CGPoint(x: $0.width - 20, y: 14) })
        XCTAssertTrue(surfaceParts(view()).contains { $0.isWell }, "Namensfeld ist ein Feld")
    }

    func testRenameOnRaisedSurface() {
        let s = Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "Alpha")
        let view = { RenameSessionView(model: RenameSessionModel(session: s)) }
        assertOnRaised(view(), at: { CGPoint(x: $0.width - 20, y: 14) })
        XCTAssertTrue(surfaceParts(view()).contains { $0.isWell }, "Namensfeld ist ein Feld")
    }

    // MARK: Helfer

    private var group: Kadrell.Group {
        Kadrell.Group(id: "g", name: "Projekt", color: "#89b4fa", cwd: tmp.path, sessionIds: [], favorite: false)
    }

    /// ⌘N mit einer Gruppe im Temp-Ordner: die Liste hat mindestens eine (ausgewählte) Zeile.
    private func newSessionView() -> NewSessionView {
        let model = NewSessionModel(groups: [group], counts: [:], index: FolderIndex(directory: tmp), askFinder: false)
        return NewSessionView(model: model, onStart: { _, _ in })
    }

    /// Rendert `view` auf der erhabenen Fläche in Instrument Light (dort sind `raised`, `bg` und `panel` verschieden)
    /// und prüft, dass am Punkt die Fläche selbst zu sehen ist, kein eigener Grund des Dialogs.
    func assertOnRaised<V: View>(_ view: V, at point: (CGSize) -> CGPoint, file: StaticString = #filePath, line: UInt = #line) {
        let t = ColorTheme.named("instrument-light")
        let s = snapshot(view.raisedPanel(), theme: t, scale: 1)
        let p = point(s.size)
        XCTAssertLessThan(distance(pixel(s, at: p), t.raised), 0.02, "erhabene Fläche bei \(p)", file: file, line: line)
    }

    /// Teile aller Instrument-Flächen, die `view` im Hosting anlegt.
    private func surfaceParts<V: View>(_ view: V) -> [InstrumentSurfaceView.Part] {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let host = NSHostingView(rootView: view)
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date() + 0.05)
        host.layoutSubtreeIfNeeded()
        return surfaces(in: host).map(\.part)
    }

    /// Genau diese Tasten, unabhängig von der Reihenfolge der Hosting-Views.
    private func assertKeys<V: View>(_ view: V, _ expected: [KeyRole], line: UInt = #line) {
        let roles = surfaceParts(view).compactMap { if case .key(_, let role) = $0 { role } else { nil } }
        XCTAssertEqual(roles.count, expected.count, "Anzahl Tasten", line: line)
        XCTAssertEqual(Set(roles), Set(expected), line: line)
    }

    private func surfaces(in view: NSView) -> [InstrumentSurfaceView] {
        view.subviews.flatMap { sub in (sub as? InstrumentSurfaceView).map { [$0] } ?? surfaces(in: sub) }
    }
}

private extension InstrumentSurfaceView.Part {
    var isWell: Bool { if case .well = self { true } else { false } }
}
