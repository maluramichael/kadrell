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

    // MARK: Hilfe, Statistik

    /// Hilfe: Fugen statt Divider (unter dem Kopf, vor der Fußzeile), Bereichstitel und Links in der Akzent-Textfarbe,
    /// nirgends die Akzent-Füllfarbe (Solarized Light trennt beide).
    func testAboutOnRaisedSurface() {
        assertOnRaised(AboutView(), at: { CGPoint(x: $0.width - 20, y: 20) })
        XCTAssertEqual(surfaceParts(AboutView()).filter { $0 == .hSeam }.count, 2, "Fugen statt Divider")
        assertAccentTextOnly(AboutView())
    }

    /// Statistik: Fugen statt Divider, das Datum des Rekords in der Akzent-Textfarbe.
    func testStatsOnRaisedSurface() {
        let suite = "de.malura.kadrell.tests.statslook"
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = UserDefaults(suiteName: suite)!
        defer {
            UserDefaults.standard.removePersistentDomain(forName: suite)
            Stats.defaults = Profile.defaults
        }
        Stats.noteConcurrent(23)
        assertOnRaised(StatsView(), at: { CGPoint(x: $0.width - 20, y: 20) })
        XCTAssertEqual(surfaceParts(StatsView()).filter { $0 == .hSeam }.count, 2, "Fugen statt Divider")
        assertAccentTextOnly(StatsView())
    }

    /// Im Bild kommt die Akzent-Textfarbe vor, die Akzent-Füllfarbe nicht.
    private func assertAccentTextOnly<V: View>(_ view: V, line: UInt = #line) {
        let t = ColorTheme.named("solarized-light")
        XCTAssertNotEqual(t.accentText.hexString, t.accent.hexString, "Schema trennt Text- und Füllfarbe", line: line)
        let s = snapshot(view.raisedPanel(), theme: t, scale: 1)
        var text = 0, fill = 0
        for y in 0..<s.rep.pixelsHigh {
            for x in 0..<s.rep.pixelsWide {
                let c = pixel(s.rep, x: x, y: y)
                if distance(c, t.accentText) < 0.04 { text += 1 }
                if distance(c, t.accent) < 0.04 { fill += 1 }
            }
        }
        XCTAssertGreaterThan(text, 0, "Akzent-Textfarbe kommt vor", line: line)
        XCTAssertEqual(fill, 0, "Akzent-Füllfarbe nicht im Text", line: line)
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

    // MARK: Einstellungen

    func testSettingsOnRaisedSurface() {
        // Mitte der Kopfzeile ist leer (Titel links, Version rechts).
        assertOnRaised(SettingsView(model: SettingsModel()), at: { CGPoint(x: $0.width / 2, y: 14) })
    }

    /// Jede Tabelle ist eine Gruppe mit Fugenring, zwischen ihren Zeilen liegen Fugen statt Linien; dazu eine Fuge
    /// unter der Kopfzeile. Neun Bereiche plus zwei Kürzel-Tabellen (Navigation, Kacheln verwalten).
    func testSettingsTablesAreGroupsWithSeams() {
        let parts = surfaceParts(SettingsView(model: SettingsModel()))
        let groups = parts.filter { $0 == .group }.count
        XCTAssertEqual(groups, 11, "jede Tabelle eine Gruppe")
        XCTAssertGreaterThan(parts.filter { $0 == .hSeam }.count, groups, "Fugen zwischen den Zeilen und unter der Kopfzeile")
    }

    /// Werte sitzen in Feldern, alle bündig in der rechten Spalte: Dropdowns, Editor, Pfadfelder (ohne eigenen Kasten
    /// um das Feld) und Kürzel. Ein Kürzel in Aufnahme ist ein Feld mit Akzentring, keine Akzent-Füllung.
    func testSettingsValuesSitInAlignedWells() {
        let model = SettingsModel()
        model.recording = .nextSession
        defer { model.recording = nil }
        let (host, views) = hostedSurfaces(SettingsView(model: model))
        let wells = views.filter { $0.part.isWell }.map { $0.convert($0.bounds, to: host) }
        // Jedes Kürzel ein Feld, dazu Editor und zwei Pfadfelder; was darüber liegt, sind die Dropdowns.
        XCTAssertGreaterThan(wells.count, HotkeyAction.allCases.count + 3, "Kürzel, Felder und Dropdowns sind Felder")
        XCTAssertEqual(Set(wells.map { $0.minX.rounded() }).count, 1, "alle Felder beginnen an derselben Kante: \(Set(wells.map(\.minX)))")
        XCTAssertEqual(views.filter { $0.part == .well(focused: true) }.count, 1, "Kürzel in Aufnahme trägt den Akzentring")
    }

    /// Aktionen sind neutrale Tasten (Primär bleibt dem Fuß vorbehalten). Fest da sind nur Ordner, Bild und kein Bild;
    /// Konten- und „Standard“-Tasten hängen von Accounts und Kürzeln des Profils ab, sie sind nur mitgezählt.
    func testSettingsActionsAreNeutralKeys() {
        let roles = surfaceParts(SettingsView(model: SettingsModel())).compactMap { if case .key(_, let role) = $0 { role } else { nil } }
        XCTAssertGreaterThanOrEqual(roles.count, 3)
        XCTAssertEqual(Set(roles), [.neutral])
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
        hostedSurfaces(view).surfaces.map(\.part)
    }

    /// Hosting-View bei Maßstab 1 in natürlicher Größe samt aller Instrument-Flächen darin (für Lage und Teil).
    private func hostedSurfaces<V: View>(_ view: V) -> (host: NSView, surfaces: [InstrumentSurfaceView]) {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let host = NSHostingView(rootView: view)
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date() + 0.05)
        host.layoutSubtreeIfNeeded()
        return (host, surfaces(in: host))
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
