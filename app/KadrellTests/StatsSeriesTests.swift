import XCTest
@testable import Kadrell

/// Statistik-Seite: Verlauf der Limits speichern, Prompts in Balken und Projekte aufteilen, Linien an Lücken trennen.
final class StatsSeriesTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)
    private var calendar: Calendar {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "UTC")!
        return c
    }

    private func event(_ secondsAgo: Double, _ project: String) -> PromptEvent {
        PromptEvent(time: now.timeIntervalSince1970 - secondsAgo, project: project)
    }

    // MARK: Prompts

    func testEventsCountCopiedLinesOnce() {
        var a = HistorySession(sessionId: "a", path: "/a", cwd: "/x/kadrell")
        a.promptTimes = [1, 2]
        var b = HistorySession(sessionId: "b", path: "/b", cwd: "/x/other")
        b.promptTimes = [2, 3]
        XCTAssertEqual(PromptStats.events([a, b]).map(\.time).sorted(), [1, 2, 3])
    }

    func testWorktreeCountsAsItsProject() {
        XCTAssertEqual(PromptStats.projectName(cwd: "/x/kadrell/.claude/worktrees/stats-charts"), "kadrell")
        XCTAssertEqual(PromptStats.projectName(cwd: "/x/kadrell"), "kadrell")
    }

    func testBarsStackProjectsInOneHour() {
        let slots = ["a": 0, "b": 1]
        let events = [event(10, "a"), event(20, "a"), event(30, "b"), event(40, "c")]
        let bars = PromptStats.bars(events, range: .day, slots: slots, calendar: calendar)
        XCTAssertEqual(bars.map(\.project), ["a", "b", PromptStats.other])
        XCTAssertEqual(bars.map(\.low), [0, 2, 3])
        XCTAssertEqual(bars.map(\.high), [2, 3, 4])
        XCTAssertNil(bars[2].slot)
    }

    func testBucketWidthFollowsRange() {
        let date = Date(timeIntervalSince1970: 1_800_000_000)   // 15.01.2027, 08:00 UTC
        let start = StatsRange.week.barStart(date.addingTimeInterval(3600), calendar: calendar)
        XCTAssertEqual(calendar.component(.hour, from: start) % 3, 0)
        XCTAssertEqual(StatsRange.week.barEnd(start, calendar: calendar).timeIntervalSince(start), 3 * 3600)
        XCTAssertEqual(StatsRange.month.barEnd(StatsRange.month.barStart(date, calendar: calendar), calendar: calendar)
            .timeIntervalSince(StatsRange.month.barStart(date, calendar: calendar)), 24 * 3600)
    }

    /// Die Farbe hängt an den letzten 30 Tagen, nicht am gewählten Zeitraum, und ab dem achten Projekt gibt es keine mehr.
    func testSlotsPreferRecentProjectsAndStop() {
        var events = (0..<5).map { event(Double($0) + 40 * 24 * 3600, "alt") }
        events.append(event(100, "neu"))
        XCTAssertEqual(PromptStats.slots(events, now: now)["neu"], 0)
        XCTAssertEqual(PromptStats.slots(events, now: now)["alt"], 1)
        let many = (0..<12).map { event(Double($0), "p\($0)") }
        XCTAssertEqual(PromptStats.slots(many, now: now).count, PromptStats.colorSlots)
    }

    /// Der Rand des Zeitraums schneidet Balken: Sie liegen sonst über der Achse und der Projektliste.
    func testBarIsCutAtRangeEdges() {
        let start = Date(timeIntervalSince1970: 1000), end = Date(timeIntervalSince1970: 2000)
        let bar = PromptBar(start: start, end: end, project: "a", slot: 0, low: 0, high: 1)
        XCTAssertEqual(bar.visibleSpan(from: .distantPast, to: .distantFuture), start...Date(timeIntervalSince1970: 1900), "ein Zehntel frei")
        let inner = bar.visibleSpan(from: Date(timeIntervalSince1970: 1200), to: Date(timeIntervalSince1970: 1500))
        XCTAssertEqual(inner, Date(timeIntervalSince1970: 1200)...Date(timeIntervalSince1970: 1500))
        let sliver = bar.visibleSpan(from: Date(timeIntervalSince1970: 1950), to: .distantFuture)
        XCTAssertEqual(sliver.lowerBound, sliver.upperBound, "liegt nur die Lücke im Zeitraum, bleibt nichts übrig")
    }

    func testInRangeAndProjects() {
        let events = [event(10, "a"), event(20, "a"), event(30, "b"), event(2 * 24 * 3600, "b")]
        let visible = PromptStats.inRange(events, .day, now: now)
        XCTAssertEqual(visible.count, 3)
        let projects = PromptStats.projects(visible, slots: ["a": 0])
        XCTAssertEqual(projects.map(\.name), ["a", "b"])
        XCTAssertEqual(projects.map(\.count), [2, 1])
        XCTAssertEqual(projects.map(\.slot), [0, nil])
        XCTAssertEqual(projects[0].last, now.addingTimeInterval(-10), "letzter Prompt des Projekts")
    }

    // MARK: Filtern und Sortieren

    /// Gefiltert stehen nur die gewählten Projekte im Chart, die Liste behält alle, damit man sie wieder wählen kann.
    func testFilterKeepsAllProjectsInList() {
        let events = [event(10, "a"), event(20, "b"), event(30, "b"), event(40, "c")]
        let snap = StatsSnapshot.make(range: .day, now: now, samples: [], sources: [], events: events, filter: ["b"])
        XCTAssertEqual(snap.promptTotal, 2)
        XCTAssertEqual(Set(snap.bars.map(\.project)), ["b"])
        XCTAssertEqual(snap.projects.map(\.name), ["b", "a", "c"])
        XCTAssertEqual(snap.projects.map(\.included), [true, false, false])
        XCTAssertNil(snap.projects[1].slot, "herausgefiltert: keine Farbe")
    }

    /// Ein gewähltes Projekt ohne eigene Farbe bekommt eine freie, statt grau als „Andere“ zu erscheinen.
    func testFilteredProjectWithoutColorGetsFreeSlot() {
        let slots = Dictionary(uniqueKeysWithValues: (0..<PromptStats.colorSlots).map { ("p\($0)", $0) })
        let events = [event(10, "p0"), event(20, "rest")]
        let out = PromptStats.slots(slots, filter: ["p0", "rest"], events: events)
        XCTAssertEqual(out["p0"], 0, "behält die eigene Farbe")
        XCTAssertEqual(out["rest"], 1, "erste freie Farbe")
        XCTAssertEqual(PromptStats.slots(slots, filter: [], events: events), slots, "ohne Filter unverändert")
    }

    func testProjectSortOrders() {
        let events = [event(300, "b"), event(200, "b"), event(100, "C"), event(50, "a")]
        let names = { (sort: ProjectSort) in PromptStats.projects(events, slots: [:], sort: sort).map(\.name) }
        XCTAssertEqual(names(.count), ["b", "a", "C"])
        XCTAssertEqual(names(.name), ["a", "b", "C"], "ohne Rücksicht auf Groß- und Kleinschreibung")
        XCTAssertEqual(names(.recent), ["a", "C", "b"])
        XCTAssertEqual(ProjectSort.recent.next, .count, "reihum")
    }

    // MARK: Tagestrenner

    func testDayMarksAtMidnight() {
        let marks = DayMark.marks(range: .week, from: now.addingTimeInterval(-StatsRange.week.span), to: now, calendar: calendar)
        XCTAssertEqual(marks.count, 7)
        XCTAssertTrue(marks.allSatisfy { calendar.component(.hour, from: $0.date) == 0 && calendar.component(.minute, from: $0.date) == 0 })
        XCTAssertFalse(marks.contains { $0.labeled }, "7 Tage: Datum steht an der Achse")
        let day = DayMark.marks(range: .day, from: now.addingTimeInterval(-StatsRange.day.span), to: now, calendar: calendar)
        XCTAssertEqual(day.count, 1, "24 Std.: genau ein Tageswechsel")
        XCTAssertTrue(day[0].labeled)
    }

    func testDayMarksMonthLabelsWeekStartYearUsesMonths() {
        let month = DayMark.marks(range: .month, from: now.addingTimeInterval(-StatsRange.month.span), to: now, calendar: calendar)
        XCTAssertEqual(month.count, 30)
        XCTAssertTrue(month.filter(\.labeled).allSatisfy { calendar.component(.weekday, from: $0.date) == calendar.firstWeekday })
        XCTAssertEqual(month.filter(\.labeled).count, 4)
        let year = DayMark.marks(range: .year, from: now.addingTimeInterval(-StatsRange.year.span), to: now, calendar: calendar)
        XCTAssertEqual(year.count, 12)
        XCTAssertTrue(year.allSatisfy { calendar.component(.day, from: $0.date) == 1 })
    }

    // MARK: Limits

    private func sample(_ secondsAgo: Double, weekly: Int) -> UsageSample {
        UsageSample(t: now.timeIntervalSince1970 - secondsAgo, account: "a", weekly: weekly)
    }

    func testLimitLineBreaksAtGap() {
        // Abstände größer als ein Bucket (6 Minuten): Sonst fasst die Verdichtung die Messungen zusammen.
        let samples = [sample(3 * 3600, weekly: 10), sample(3 * 3600 - 900, weekly: 11), sample(1200, weekly: 20), sample(300, weekly: 21)]
        let points = LimitStats.points(samples, range: .day, now: now, kinds: [.weekly])
        XCTAssertEqual(points.map(\.segment), [0, 0, 1, 1])
    }

    /// Lücken und Ränder füllen gestrichelte Stücke: über die Lücke von Punkt zu Punkt, hinten bis jetzt.
    func testBridgesSpanGapAndTrail() {
        let samples = [sample(3 * 3600, weekly: 10), sample(3 * 3600 - 900, weekly: 11), sample(1200, weekly: 20), sample(300, weekly: 21)]
        let points = LimitStats.points(samples, range: .day, now: now, kinds: [.weekly])
        let bridges = LimitStats.bridges(points, samples: samples, range: .day, now: now, kinds: [.weekly])
        XCTAssertTrue(bridges.allSatisfy(\.bridge))
        XCTAssertEqual(bridges.map(\.value), [11, 20, 21, 21], "Lücke, dann bis jetzt")
        XCTAssertEqual(bridges.last?.date, now)
        XCTAssertEqual(Set(bridges.map(\.series)).count, 2, "zwei getrennte Stücke")
        XCTAssertTrue(Set(bridges.map(\.series)).isDisjoint(with: Set(points.map(\.series))), "eigene Serien, nicht an die Messlinie gehängt")
    }

    /// Vorn beginnt die Linie am Rand, auf der Geraden vom letzten Wert davor zum ersten im Zeitraum.
    func testBridgeLeadsFromValueBeforeRange() {
        let samples = [sample(30 * 3600, weekly: 0), sample(18 * 3600, weekly: 60)]
        let points = LimitStats.points(samples, range: .day, now: now, kinds: [.weekly])
        let bridges = LimitStats.bridges(points, samples: samples, range: .day, now: now, kinds: [.weekly])
        XCTAssertEqual(bridges.first?.date, now.addingTimeInterval(-StatsRange.day.span))
        XCTAssertEqual(bridges.first?.value, 30, "halber Weg von 0 zu 60")
    }

    /// Nichts gemessen im Zeitraum, aber davor: der letzte Wert gilt über den ganzen Zeitraum.
    func testBridgeCarriesOldValueThroughEmptyRange() {
        let samples = [sample(2 * 24 * 3600, weekly: 42)]
        let limits = StatsSnapshot.make(range: .day, now: now, samples: samples,
                                        sources: [LimitSource(id: "a", title: "A", detail: "", kinds: [.weekly], current: nil)], events: []).limits[0]
        XCTAssertTrue(limits.points.isEmpty)
        XCTAssertFalse(limits.isEmpty)
        XCTAssertEqual(limits.bridges.map(\.value), [42, 42])
        XCTAssertEqual(limits.bridges.map(\.date), [now.addingTimeInterval(-StatsRange.day.span), now])
    }

    /// Über ein Jahr fasst ein Punkt viele Messungen zusammen: Die Spitze vor dem Reset bleibt, der kleinere Wert fällt weg.
    func testLimitBucketsKeepPeak() {
        let width = StatsRange.year.span / LimitStats.maxPoints
        let base = (now.timeIntervalSince1970 / width).rounded(.down) * width
        let samples = [UsageSample(t: base + 1, account: "a", weekly: 90), UsageSample(t: base + 2, account: "a", weekly: 3)]
        let points = LimitStats.points(samples, range: .year, now: now, kinds: [.weekly])
        XCTAssertEqual(points.map(\.value), [90])
    }

    func testLimitLineSkipsOldAndMissingValues() {
        let samples = [sample(2 * 24 * 3600, weekly: 5), UsageSample(t: now.timeIntervalSince1970 - 60, account: "a", session: 7)]
        XCTAssertTrue(LimitStats.points(samples, range: .day, now: now, kinds: [.weekly]).isEmpty)
        XCTAssertEqual(LimitStats.points(samples, range: .day, now: now, kinds: [.session]).map(\.value), [7])
    }

    /// Der Stand aus dem Account-Cache füllt die Kachel vor der ersten neuen Messung, doppelt zählt er nicht.
    func testSnapshotAddsNewerCachedValueOnce() {
        let old = sample(600, weekly: 10)
        let source = { (current: UsageSample?) in
            LimitSource(id: "a", title: "A", detail: "", kinds: [.weekly], current: current)
        }
        let newer = StatsSnapshot.make(range: .day, now: now, samples: [old], sources: [source(sample(60, weekly: 12))], events: [])
        XCTAssertEqual(newer.limits[0].points.map(\.value), [10, 12])
        let same = StatsSnapshot.make(range: .day, now: now, samples: [old], sources: [source(old)], events: [])
        XCTAssertEqual(same.limits[0].points.map(\.value), [10])
    }

    // MARK: Tastatur

    func testRangeKeys() {
        XCTAssertEqual(StatsModel.range(after: .week, characters: "1", special: nil), .day)
        XCTAssertEqual(StatsModel.range(after: .week, characters: "4", special: nil), .year)
        XCTAssertNil(StatsModel.range(after: .week, characters: "5", special: nil))
        XCTAssertNil(StatsModel.range(after: .week, characters: "x", special: nil))
        XCTAssertEqual(StatsModel.range(after: .week, characters: nil, special: .rightArrow), .month)
        XCTAssertEqual(StatsModel.range(after: .day, characters: nil, special: .leftArrow), .day, "bleibt am Rand stehen")
    }

    func testListKeys() {
        XCTAssertEqual(StatsListAction(characters: nil, special: .upArrow), .up)
        XCTAssertEqual(StatsListAction(characters: nil, special: .downArrow), .down)
        XCTAssertEqual(StatsListAction(characters: " ", special: nil), .toggle)
        XCTAssertEqual(StatsListAction(characters: "s", special: nil), .sort)
        XCTAssertEqual(StatsListAction(characters: nil, special: .delete), .clear)
        XCTAssertNil(StatsListAction(characters: "x", special: nil))
        XCTAssertNil(StatsModel.range(after: .week, characters: " ", special: nil), "Leertaste wählt keinen Zeitraum")
    }

    func testCursorMoves() {
        let names = ["a", "b", "c"]
        XCTAssertEqual(StatsModel.moved(nil, in: names, down: true), "a")
        XCTAssertEqual(StatsModel.moved(nil, in: names, down: false), "c")
        XCTAssertEqual(StatsModel.moved("a", in: names, down: true), "b")
        XCTAssertEqual(StatsModel.moved("c", in: names, down: true), "c", "bleibt am Ende")
        XCTAssertEqual(StatsModel.moved("gone", in: names, down: true), "a", "verschwundenes Projekt: von vorn")
        XCTAssertNil(StatsModel.moved("a", in: [], down: true))
    }
}

/// Die Verlaufsdatei: anhängen, nach einem Neustart wieder lesen, kaputte Zeilen überspringen.
@MainActor
final class UsageHistoryTests: XCTestCase {
    private var url: URL!

    override func setUp() async throws {
        url = FileManager.default.temporaryDirectory.appendingPathComponent("usage-\(UUID().uuidString).jsonl")
    }

    override func tearDown() async throws { try? FileManager.default.removeItem(at: url) }

    func testAppendSurvivesRestart() async {
        let history = UsageHistory(url: url)
        history.append(UsageSample(t: 1, account: "a", session: 5, weekly: 6, fable: 7))
        history.append(UsageSample(t: 2, account: "b", weekly: 8))
        let loaded = await UsageHistory(url: url).samples()
        XCTAssertEqual(loaded, [UsageSample(t: 1, account: "a", session: 5, weekly: 6, fable: 7), UsageSample(t: 2, account: "b", weekly: 8)])
    }

    func testSkipsEmptyMeasurement() async {
        let history = UsageHistory(url: url)
        history.append(UsageSample(t: 1, account: "a"))
        let loaded = await history.samples()
        XCTAssertTrue(loaded.isEmpty)
    }

    func testAppendAfterLoadIsVisible() async {
        let history = UsageHistory(url: url)
        _ = await history.samples()
        history.append(UsageSample(t: 3, account: "a", weekly: 1))
        let loaded = await history.samples()
        XCTAssertEqual(loaded.count, 1)
    }

    func testParseSkipsBrokenLine() {
        let data = Data("{\"t\":1,\"account\":\"a\",\"weekly\":2}\n{\"t\":2,\"acc".utf8)
        XCTAssertEqual(UsageHistory.parse(data), [UsageSample(t: 1, account: "a", weekly: 2)])
    }
}
