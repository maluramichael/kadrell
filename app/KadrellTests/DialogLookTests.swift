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

    // MARK: Wächter

    /// Kein Dialog fällt in den flachen Look zurück: keine Linien in `line`, keine Divider, kein eigener Dialoggrund,
    /// kein weicher Schatten. Gilt für alle Dialoge, die rechte Sidebar und die Hover-Toolbar des Baums.
    func testDialogsUseInstrumentParts() throws {
        let sources = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().appendingPathComponent("Kadrell")
        var files = try ["UI", "Extensions/UI"].flatMap { try swiftFiles(under: sources.appendingPathComponent($0)) }
        files += ["Sidebar/SidebarView.swift", "App/SheetPresenter.swift"].map { sources.appendingPathComponent($0) }
        XCTAssertGreaterThan(files.count, 15, "Quellen gefunden")
        let forbidden = ["Theme.line", "lineColor", "Divider()", "Theme.panelColor", "NSShadow()"]
        var offenders: [String] = []
        for file in files {
            for (i, line) in try String(contentsOf: file, encoding: .utf8).components(separatedBy: "\n").enumerated() {
                for word in forbidden where line.contains(word) { offenders.append("\(file.lastPathComponent):\(i + 1) \(word)") }
            }
        }
        XCTAssertEqual(offenders, [], "flacher Look in Dialogen")
    }

    /// Der Wächter liest auch Unterordner: Dateien, die später in einen Unterordner von `UI` kommen, fallen darunter.
    func testGuardReadsSubfolders() throws {
        let nested = tmp.appendingPathComponent("a/b")
        try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
        for name in ["oben.swift", "a/b/tief.swift", "a/b/notiz.md"] { try Data().write(to: tmp.appendingPathComponent(name)) }
        XCTAssertEqual(Set(try swiftFiles(under: tmp).map(\.lastPathComponent)), ["oben.swift", "tief.swift"])
    }

    // MARK: Helfer

    /// Alle Swift-Dateien unter `dir`, rekursiv.
    private func swiftFiles(under dir: URL) throws -> [URL] {
        let enumerator = try XCTUnwrap(FileManager.default.enumerator(at: dir, includingPropertiesForKeys: nil))
        return enumerator.compactMap { $0 as? URL }.filter { $0.pathExtension == "swift" }
    }

    private var group: Kadrell.Group {
        Kadrell.Group(id: "g", name: "Projekt", color: "#89b4fa", cwd: tmp.path, sessionIds: [], favorite: false)
    }

    /// ⌘N mit einer Gruppe im Temp-Ordner: die Liste hat mindestens eine (ausgewählte) Zeile.
    private func newSessionView() -> NewSessionView {
        let model = NewSessionModel(groups: [group], counts: [:], index: FolderIndex(directory: tmp), askFinder: false)
        return NewSessionView(model: model, onStart: { _, _ in })
    }

    /// Genau diese Tasten, unabhängig von der Reihenfolge der Hosting-Views.
    private func assertKeys<V: View>(_ view: V, _ expected: [KeyRole], line: UInt = #line) {
        let roles = surfaceParts(view).compactMap { if case .key(_, let role) = $0 { role } else { nil } }
        XCTAssertEqual(roles.count, expected.count, "Anzahl Tasten", line: line)
        XCTAssertEqual(Set(roles), Set(expected), line: line)
    }
}
