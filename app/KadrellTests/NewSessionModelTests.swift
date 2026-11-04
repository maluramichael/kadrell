import XCTest
@testable import Kadrell

@MainActor
final class NewSessionModelTests: XCTestCase {
    /// XCTest legt je Test eine eigene Instanz an: jeder Test hat seinen eigenen Ordner.
    private let root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-newsession-\(UUID().uuidString)")

    override func setUpWithError() throws {
        try FileManager.default.createDirectory(at: root.appendingPathComponent("projects/kadrell"), withIntermediateDirectories: true)
    }

    override func tearDown() { try? FileManager.default.removeItem(at: root) }

    /// Der Temp-Ordner spielt Home und Startordner, damit nichts unter dem echten Home entsteht.
    private func model() -> NewSessionModel {
        NewSessionModel(groups: [], counts: [:], index: FolderIndex(directory: root), askFinder: false,
                        startFolder: root.path + "/projects/", home: root.path)
    }

    // MARK: Ziel bestimmen

    /// Bloßer Name: neuer Ordner im Startordner. Ein vorhandener Ordner ist kein Ziel.
    func testCreationTargetForBareName() {
        let start = root.path + "/projects/"
        XCTAssertEqual(FolderIndex.creationTarget("wetterbot", startFolder: start, home: root.path), root.path + "/projects/wetterbot")
        XCTAssertNil(FolderIndex.creationTarget("kadrell", startFolder: start, home: root.path), "gibt es schon")
        XCTAssertNil(FolderIndex.creationTarget("zwei worte", startFolder: start, home: root.path), "Leerzeichen trennen Suchwörter")
        XCTAssertNil(FolderIndex.creationTarget("", startFolder: start, home: root.path))
    }

    /// Pfade: absolut wörtlich, relativ zum Startordner, der Elternteil darf abgekürzt sein.
    func testCreationTargetForPaths() {
        let start = root.path + "/projects/"
        XCTAssertEqual(FolderIndex.creationTarget(root.path + "/projects/neu/", startFolder: start, home: root.path), root.path + "/projects/neu")
        XCTAssertEqual(FolderIndex.creationTarget("kadrell/sub", startFolder: start, home: root.path), root.path + "/projects/kadrell/sub")
        XCTAssertEqual(FolderIndex.creationTarget("proj/neu", startFolder: start, home: root.path), root.path + "/projects/neu", "abgekürzt ab Home")
        XCTAssertEqual(FolderIndex.creationTarget(root.path + "/a/b/c", startFolder: start, home: root.path), root.path + "/a/b/c",
                       "fehlende Zwischenordner entstehen mit")
    }

    /// Nie außerhalb von Home, nie mit „..“ hinaus.
    func testCreationTargetStaysInsideHome() {
        let start = root.path + "/projects/"
        XCTAssertNil(FolderIndex.creationTarget("/opt/kadrell-test-neu", startFolder: start, home: root.path))
        XCTAssertNil(FolderIndex.creationTarget(root.path + "/../ausbruch", startFolder: start, home: root.path))
        XCTAssertNil(FolderIndex.creationTarget("../ausbruch", startFolder: start, home: root.path))
        XCTAssertNil(FolderIndex.creationTarget(root.path, startFolder: start, home: root.path), "Home selbst gibt es schon")
    }

    // MARK: Ablauf im Dialog

    /// Neuer Name ohne passenden Treffer: „Ordner anlegen“ steht oben, erstes ⏎ fragt nach, zweites legt an und startet.
    func testCreateRowNeedsSecondReturn() {
        let m = model()
        var started: String?
        m.onStart = { _, p in started = p }
        m.query = "wetterbot"
        let target = root.path + "/projects/wetterbot"
        XCTAssertEqual(m.candidates.first?.isCreate, true)
        XCTAssertEqual(m.candidates.first?.path, target)

        m.start()
        XCTAssertEqual(m.confirmPath, target)
        XCTAssertNil(started)
        XCTAssertFalse(FolderIndex.isDirectory(target))

        m.start()
        XCTAssertTrue(FolderIndex.isDirectory(target))
        XCTAssertEqual(started, target)
    }

    /// Passt ein vorhandener Ordner zum Namen, startet ⏎ weiter dort; „Ordner anlegen“ steht ganz unten.
    func testExistingMatchStaysFirst() {
        let m = model()
        m.query = root.path + "/projects/kad"
        XCTAssertEqual(m.candidates.first?.path, root.path + "/projects/kadrell")
        XCTAssertEqual(m.candidates.last?.isCreate, true)
    }

    /// Tippen nach der Nachfrage hebt sie wieder auf.
    func testConfirmResetsOnQueryChange() {
        let m = model()
        m.query = "wetterbot"
        m.start()
        XCTAssertNotNil(m.confirmPath)
        m.query = "wetterbot2"
        XCTAssertNil(m.confirmPath)
    }

    /// ⌘⏎ legt ohne Nachfrage an und startet.
    func testCommandReturnCreatesDirectly() {
        let m = model()
        var started: String?
        m.onStart = { _, p in started = p }
        m.query = root.path + "/neu/tief"
        m.createAndStart()
        XCTAssertTrue(FolderIndex.isDirectory(root.path + "/neu/tief"))
        XCTAssertEqual(started, root.path + "/neu/tief")
    }

    /// Außerhalb von Home: kein Angebot, ⏎ meldet den fehlenden Ordner, ⌘⏎ legt nichts an.
    func testOutsideHomeOnlyReportsMissing() {
        let m = model()
        var started: String?
        m.onStart = { _, p in started = p }
        m.query = "/kadrell-test-gibt-es-nicht"
        XCTAssertTrue(m.candidates.isEmpty)
        m.start()
        XCTAssertEqual(m.notFoundPath, "/kadrell-test-gibt-es-nicht")
        m.createAndStart()
        XCTAssertNil(started)
        XCTAssertFalse(FileManager.default.fileExists(atPath: "/kadrell-test-gibt-es-nicht"))
        m.query = "/kadrell-test-gibt-es-auch-nicht"
        XCTAssertNil(m.notFoundPath, "Hinweis verschwindet beim Tippen")
    }
}
