import XCTest
@testable import Kadrell

final class StatsTests: XCTestCase {
    private let suite = "de.malura.kadrell.tests.stats"

    override func setUp() {
        super.setUp()
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = UserDefaults(suiteName: suite)!
    }

    override func tearDown() {
        UserDefaults.standard.removePersistentDomain(forName: suite)
        Stats.defaults = Profile.defaults
        super.tearDown()
    }

    func testCountStartsAtZero() {
        XCTAssertEqual(Stats.count(.messages), 0)
    }

    func testBumpCountsUp() {
        Stats.bump(.sessions)
        Stats.bump(.sessions)
        XCTAssertEqual(Stats.count(.sessions), 2)
    }

    func testBumpByMoreThanOne() {
        Stats.bump(.messages, by: 5)
        XCTAssertEqual(Stats.count(.messages), 5)
    }

    func testBumpByZeroWritesNothing() {
        Stats.bump(.messages, by: 0)
        XCTAssertNil(Stats.defaults.object(forKey: "stats.messages"))
    }

    func testCountersAreIndependent() {
        Stats.bump(.terminals, by: 3)
        XCTAssertEqual(Stats.count(.terminals), 3)
        XCTAssertEqual(Stats.count(.focusSwitches), 0)
    }

    func testRecordKeepsHighestValue() {
        Stats.noteConcurrent(7)
        Stats.noteConcurrent(4)
        XCTAssertEqual(Stats.record.count, 7)
    }

    func testRecordStoresDateOfNewHigh() {
        XCTAssertNil(Stats.record.date)
        Stats.noteConcurrent(3)
        let first = Stats.record.date
        XCTAssertNotNil(first)
        Stats.noteConcurrent(9)
        XCTAssertEqual(Stats.record.count, 9)
        XCTAssertNotNil(Stats.record.date)
    }

    /// Ein niedrigerer Stand darf das Rekorddatum nicht überschreiben.
    func testRecordDateSurvivesLowerValue() {
        Stats.noteConcurrent(5)
        let date = Stats.record.date
        Stats.noteConcurrent(1)
        XCTAssertEqual(Stats.record.date, date)
    }

    func testSinceIsSetOnFirstBump() {
        XCTAssertNil(Stats.since)
        Stats.bump(.sessions)
        XCTAssertNotNil(Stats.since)
    }

    /// Der Zählbeginn bleibt der erste, nicht der jüngste.
    func testSinceDoesNotMove() {
        Stats.bump(.sessions)
        let first = Stats.since
        Stats.bump(.messages)
        Stats.noteConcurrent(4)
        XCTAssertEqual(Stats.since, first)
    }
}
