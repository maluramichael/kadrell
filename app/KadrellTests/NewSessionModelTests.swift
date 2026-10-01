import XCTest
@testable import Kadrell

@MainActor
final class NewSessionModelTests: XCTestCase {
    /// ⏎ auf einem getippten, nicht existierenden Pfad: kein stiller No-op, sondern ein Hinweis mit dem Pfad;
    /// ⌘⏎ (`createAndStart`) legt ihn an und startet dort.
    func testMissingPathOffersCreateAndStart() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-newsession-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let index = FolderIndex(directory: root)
        let model = NewSessionModel(groups: [], counts: [:], index: index, finderPermission: { .denied })
        var started: (Group?, String)?
        model.onStart = { g, p in started = (g, p) }

        let missing = root.path + "/does-not-exist"
        model.query = missing
        model.start()
        XCTAssertNil(started)
        XCTAssertEqual(model.notFoundPath, missing)

        model.createAndStart()
        XCTAssertTrue(FolderIndex.isDirectory(missing))
        XCTAssertEqual(started?.1, missing)
        XCTAssertNil(model.notFoundPath)
    }

    /// Query ändert sich: der Hinweis vom letzten Fehlversuch verschwindet wieder.
    func testNotFoundResetsOnQueryChange() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-newsession-\(UUID().uuidString)")
        let index = FolderIndex(directory: root)
        let model = NewSessionModel(groups: [], counts: [:], index: index, finderPermission: { .denied })
        model.query = root.path + "/nope"
        model.start()
        XCTAssertNotNil(model.notFoundPath)
        model.query = root.path + "/still-nope"
        XCTAssertNil(model.notFoundPath)
    }

    private func model(_ p: FinderPermission, calls: @escaping () -> Void) -> NewSessionModel {
        let index = FolderIndex(directory: FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-newsession-\(UUID().uuidString)"))
        return NewSessionModel(groups: [], counts: [:], index: index, finderPermission: { p }, finderFolder: { calls(); return nil })
    }

    func testFinderNotAskedOffersButtonWithoutQuery() async {
        var calls = 0
        let m = model(.notAsked) { calls += 1 }
        XCTAssertNil(m.finderTask)
        XCTAssertEqual(calls, 0)
        XCTAssertTrue(m.offersFinderButton)
        m.suggestFinderFolder()
        await m.finderTask?.value
        XCTAssertEqual(calls, 1)
        XCTAssertFalse(m.offersFinderButton)
    }

    func testFinderGrantedQueriesAutomatically() async {
        var calls = 0
        let m = model(.granted) { calls += 1 }
        await m.finderTask?.value
        XCTAssertEqual(calls, 1)
        XCTAssertFalse(m.offersFinderButton)
    }

    func testFinderDeniedDoesNothing() {
        var calls = 0
        let m = model(.denied) { calls += 1 }
        XCTAssertNil(m.finderTask)
        XCTAssertEqual(calls, 0)
        XCTAssertFalse(m.offersFinderButton)
    }
}
