import XCTest
@testable import Kadrell

@MainActor
final class AttentionTrackerTests: XCTestCase {
    private let defaults = UserDefaults(suiteName: "AttentionTrackerTests")!
    private var sounds: [Feedback.Sound] = []
    private var attention = 0
    private var notified: [(String, Bool)] = []
    private var badge = -1
    private var tips: [String?] = []

    /// Frische Defaults je Aufruf: XCTest legt alle Instanzen vorab an, Tipps und Marken sollen nicht durchsickern.
    private func tracker() -> AttentionTracker {
        defaults.removePersistentDomain(forName: "AttentionTrackerTests")
        let t = AttentionTracker(defaults: defaults)
        t.play = { [unowned self] in sounds.append($0) }
        t.requestAttention = { [unowned self] in attention += 1 }
        t.notify = { [unowned self] key, _, waiting in notified.append((key, waiting)) }
        t.setBadge = { [unowned self] in badge = $0 }
        t.announce = { _ in }
        t.onTip = { [unowned self] in tips.append($0) }
        return t
    }

    private func sessions(_ ids: String...) -> [String: Session] {
        Dictionary(uniqueKeysWithValues: ids.map { ($0, Session(id: $0, cwd: "/tmp", startedAt: 0, sessionId: $0, name: $0)) })
    }

    func testWaitingInBackgroundMarksNewPlaysAndBounces() {
        let t = tracker(), s = sessions("a", "b")
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .running, "b": .running], focused: "b", isKey: true)
        let changed = t.update(sessions: s, waiting: ["a"], statuses: ["a": .waiting, "b": .running], focused: "b", isKey: true)
        XCTAssertEqual(changed.waiting, ["a"])
        XCTAssertEqual(t.unseen, ["a"])
        XCTAssertEqual(sounds, [.waiting])
        XCTAssertEqual(notified.map(\.0), ["a"])
        XCTAssertEqual(attention, 0, "Fenster ist vorn: kein Dock-Hüpfen")
        XCTAssertEqual(badge, 1)
        XCTAssertNotNil(t.tip, "erste wartende Session zeigt den Tipp")
        XCTAssertEqual(defaults.stringArray(forKey: "sessions.unseen"), ["a"], "Marke übersteht einen Neustart")
    }

    func testFocusedSessionInKeyWindowStaysQuiet() {
        let t = tracker(), s = sessions("a")
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .running], focused: "a", isKey: true)
        let changed = t.update(sessions: s, waiting: [], statuses: ["a": .idle], focused: "a", isKey: true)
        XCTAssertEqual(changed.done, ["a"])
        XCTAssertTrue(t.unseen.isEmpty)
        XCTAssertTrue(sounds.isEmpty)
        XCTAssertTrue(notified.isEmpty)
    }

    func testBackgroundWindowCountsAsNotLookingAndBounces() {
        let t = tracker(), s = sessions("a")
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .running], focused: "a", isKey: false)
        _ = t.update(sessions: s, waiting: ["a"], statuses: ["a": .waiting], focused: "a", isKey: false)
        XCTAssertEqual(t.unseen, ["a"])
        XCTAssertEqual(attention, 1)
    }

    func testRunningAgainClearsMarkAndMarkSeen() {
        let t = tracker(), s = sessions("a", "b")
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .running, "b": .running], focused: nil, isKey: true)
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .idle, "b": .idle], focused: nil, isKey: true)
        XCTAssertEqual(t.unseen, ["a", "b"])
        XCTAssertEqual(sounds, [.done])
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .running, "b": .idle], focused: nil, isKey: true)
        XCTAssertEqual(t.unseen, ["b"], "wieder am Arbeiten: Marke weg")
        XCTAssertTrue(t.markSeen("b"))
        XCTAssertFalse(t.markSeen("b"))
        XCTAssertTrue(t.unseen.isEmpty)
    }

    func testVisibleTilesCountAsSeen() {
        let t = tracker(), s = sessions("a", "b")
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .running, "b": .running], focused: "a", looking: ["a", "b"], isKey: true)
        // b ist sichtbar, aber nicht fokussiert: fertig werden markiert keine neue Marke.
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .running, "b": .idle], focused: "a", looking: ["a", "b"], isKey: true)
        XCTAssertTrue(t.unseen.isEmpty, "sichtbare Kachel bekommt kein NEU")
        XCTAssertTrue(sounds.isEmpty)
    }

    func testBringingUnseenIntoViewClearsMark() {
        let t = tracker(), s = sessions("a")
        // a wird im Hintergrund fertig und bekommt die Marke.
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .running], focused: nil, looking: [], isKey: false)
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .idle], focused: nil, looking: [], isKey: false)
        XCTAssertEqual(t.unseen, ["a"])
        // Jetzt in den Blick geholt: Marke fällt weg, ohne dass sich der Status ändert.
        _ = t.update(sessions: s, waiting: [], statuses: ["a": .idle], focused: "a", looking: ["a"], isKey: true)
        XCTAssertTrue(t.unseen.isEmpty, "in den Blick geholt = gesehen")
    }

    func testPruneAndPersistedStart() {
        _ = tracker()
        defaults.set(["a", "gone"], forKey: "sessions.unseen")
        let t = AttentionTracker(defaults: defaults)
        XCTAssertEqual(t.unseen, ["a", "gone"])
        t.prune(to: sessions("a"))
        XCTAssertEqual(t.unseen, ["a"])
    }

    func testTipOnlyOnce() {
        let t = tracker()
        t.showTipOnce("tip.test", "x")
        t.tip = nil
        t.showTipOnce("tip.test", "x")
        XCTAssertEqual(tips, ["x", nil])
    }

    func testFocusHookFiresOnlyOnChange() {
        let t = tracker(), s = sessions("a", "b")
        var fired: [String] = []
        _ = t.update(sessions: s, waiting: [], statuses: [:], focused: "a", isKey: true)
        t.fireFocusHook = { fired.append($0.id) }
        for f in ["a", "a", "b"] { _ = t.update(sessions: s, waiting: [], statuses: [:], focused: f, isKey: true) }
        XCTAssertEqual(fired, ["a", "b"], "vor claude kein Hook, danach nur bei Wechsel")
    }
}
