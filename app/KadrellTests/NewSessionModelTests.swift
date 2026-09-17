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
        let model = NewSessionModel(groups: [], counts: [:], index: index, askFinder: false)
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
        let model = NewSessionModel(groups: [], counts: [:], index: index, askFinder: false)
        model.query = root.path + "/nope"
        model.start()
        XCTAssertNotNil(model.notFoundPath)
        model.query = root.path + "/still-nope"
        XCTAssertNil(model.notFoundPath)
    }
}
