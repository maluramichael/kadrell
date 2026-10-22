import SwiftUI
import XCTest
@testable import Kadrell

/// Randlose NSTextFields zeichnen ihre Zeile von Haus aus an der Oberkante des Rahmens. Unsere Eingabefelder sind
/// höher als eine Zeile, der Text klebte dadurch oben statt in der Mitte. `CenteredTextFieldCell` rückt ihn mittig.
@MainActor
final class TextFieldCenteringTests: XCTestCase {
    func testDrawingRectIsVerticallyCentered() {
        let cell = CenteredTextFieldCell(textCell: "Ag")
        cell.font = Theme.font(19.5)
        let bounds = NSRect(x: 0, y: 0, width: 300, height: 33)
        let r = cell.drawingRect(forBounds: bounds)
        XCTAssertGreaterThan(r.minY, 0, "Zeile steht immer noch an der Oberkante")
        XCTAssertEqual(r.minY, bounds.maxY - r.maxY, accuracy: 0.5, "oben und unten gleich viel Luft")
    }

    /// Ein Rahmen, der nicht höher als eine Zeile ist, bleibt unangetastet.
    func testTightBoundsUnchanged() {
        let cell = CenteredTextFieldCell(textCell: "Ag")
        cell.font = Theme.font(19.5)
        let bounds = NSRect(x: 0, y: 0, width: 300, height: 12)
        XCTAssertEqual(cell.drawingRect(forBounds: bounds).height, 12, accuracy: 0.01)
    }

    /// Die zentrierende Zelle ersetzt die Standardzelle des Feldes: dabei darf das Feld nicht seine
    /// Bearbeitbarkeit verlieren (eine frische NSTextFieldCell ist weder editierbar noch auswählbar).
    func testPathFieldStaysEditable() {
        let host = NSHostingView(rootView: PathField(text: .constant(""), autofocus: false, onTab: {}, onSubmit: {}, onMove: { _ in }))
        host.frame = NSRect(x: 0, y: 0, width: 300, height: 33)
        host.layoutSubtreeIfNeeded()
        guard let field = firstTextField(in: host) else { return XCTFail("kein NSTextField im PathField") }
        XCTAssertTrue(field.cell is CenteredTextFieldCell)
        XCTAssertTrue(field.isEditable)
        XCTAssertTrue(field.isSelectable)
    }

    /// Das Pfadfeld meldet Beginn und Ende der Bearbeitung nach oben: daran hängt der Akzentring des Feldes.
    func testPathFieldReportsFocus() {
        var seen: [Bool] = []
        let rep = PathFieldRep(text: .constant(""), autofocus: false, onTab: {}, onSubmit: {}, onMove: { _ in }, onFocus: { seen.append($0) })
        let c = rep.makeCoordinator(), f = NSTextField()
        c.controlTextDidBeginEditing(Notification(name: NSControl.textDidBeginEditingNotification, object: f))
        c.controlTextDidEndEditing(Notification(name: NSControl.textDidEndEditingNotification, object: f))
        XCTAssertEqual(seen, [true, false])
    }

    /// Der Akzentring kommt mit dem Fokus, nicht erst mit dem ersten Tastendruck, und geht beim Verlassen auch
    /// dann, wenn nichts getippt wurde.
    func testPathFieldReportsFocusWithoutTyping() throws {
        var seen: [Bool] = []
        let host = NSHostingView(rootView: PathFieldRep(text: .constant(""), autofocus: false, onTab: {}, onSubmit: {}, onMove: { _ in },
                                                        onFocus: { seen.append($0) }))
        host.frame = NSRect(x: 0, y: 0, width: 200, height: 30)
        let other = NSTextField(frame: NSRect(x: 0, y: 40, width: 100, height: 20))
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 300, height: 100), styleMask: [.titled], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        defer { w.close() }
        w.contentView = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 100))
        w.contentView?.addSubview(host)
        w.contentView?.addSubview(other)
        host.layoutSubtreeIfNeeded()
        let field = try XCTUnwrap(firstTextField(in: host))
        XCTAssertTrue(w.makeFirstResponder(field))
        XCTAssertEqual(seen, [true], "Fokus ohne Tastendruck")
        XCTAssertTrue(w.makeFirstResponder(other))
        XCTAssertEqual(seen, [true, false], "verlassen ohne Änderung")
    }

    private func firstTextField(in view: NSView) -> NSTextField? {
        if let f = view as? NSTextField { return f }
        return view.subviews.lazy.compactMap { self.firstTextField(in: $0) }.first
    }
}
