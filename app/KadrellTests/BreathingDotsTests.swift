import XCTest
@testable import Kadrell

/// Atmender Punkt arbeitender Sessions: Core-Animation-Ebenen statt Takt-Neuzeichnen, ruhiger Zyklus, gemeinsame
/// Phase, „Bewegung reduzieren“ steht still mit Schein.
@MainActor
final class BreathingDotsTests: XCTestCase {
    private func host() -> NSView {
        let v = NSView(frame: NSRect(x: 0, y: 0, width: 120, height: 40))
        v.wantsLayer = true
        return v
    }

    private func withReduceMotion(_ on: Bool, _ body: () throws -> Void) rethrows {
        let saved = Feedback.reduceMotion
        defer { Feedback.reduceMotion = saved }
        Feedback.reduceMotion = on
        try body()
    }

    func testSyncAddsMovesAndRemovesDots() {
        let v = host(), dots = BreathingDots()
        let a = CGRect(x: 10, y: 10, width: 8, height: 8), b = CGRect(x: 40, y: 10, width: 8, height: 8)
        dots.sync(["a": (a, .systemBlue), "b": (b, .systemRed)], in: v)
        XCTAssertEqual(dots.frames, ["a": v.convertToLayer(a), "b": v.convertToLayer(b)])
        XCTAssertEqual(dots.color("b"), NSColor.systemRed.cgColor)
        let moved = a.offsetBy(dx: 0, dy: 20)
        dots.sync(["a": (moved, .systemGreen)], in: v)
        XCTAssertEqual(dots.frames, ["a": v.convertToLayer(moved)], "b ist weg, a sitzt an der neuen Stelle")
        XCTAssertEqual(dots.color("a"), NSColor.systemGreen.cgColor, "Farbe folgt dem Schema")
        dots.sync([:], in: v)
        XCTAssertTrue(dots.frames.isEmpty)
    }

    /// Unter den Unter-Views (Terminal, Knöpfe), über dem, was die View selbst zeichnet.
    func testContainerSitsBelowSubviews() throws {
        let v = host()
        v.addSubview(NSView(frame: NSRect(x: 0, y: 0, width: 10, height: 10)))
        let dots = BreathingDots()
        dots.sync(["a": (CGRect(x: 10, y: 10, width: 8, height: 8), .systemBlue)], in: v)
        let first = try XCTUnwrap(v.layer?.sublayers?.first)
        XCTAssertEqual(first.sublayers?.count, 1, "erste Unterebene ist der Behälter der Punkte")
    }

    func testBreathesSlowlyAndInPhase() throws {
        try withReduceMotion(false) {
            let v = host(), dots = BreathingDots()
            dots.sync(["a": (CGRect(x: 10, y: 10, width: 8, height: 8), .systemBlue)], in: v)
            XCTAssertTrue(dots.isBreathing("a"))
            XCTAssertGreaterThanOrEqual(BreathingDots.period, 2.5, "ruhig: mindestens 2,5 s je Atemzug")
            XCTAssertLessThanOrEqual(BreathingDots.period, 3)
            let layer = try XCTUnwrap(v.layer?.sublayers?.first?.sublayers?.first)
            let group = try XCTUnwrap(layer.animation(forKey: "breath") as? CAAnimationGroup)
            XCTAssertEqual(group.duration * 2, BreathingDots.period, accuracy: 0.001, "ein- und ausatmen")
            XCTAssertTrue(group.autoreverses)
            XCTAssertEqual(group.repeatCount, .infinity)
            // Beginn auf einem Vielfachen der Periode in Medienzeit: alle Punkte atmen gemeinsam.
            let begin = group.beginTime - layer.convertTime(0, from: nil)
            let r = begin.truncatingRemainder(dividingBy: BreathingDots.period)
            XCTAssertLessThan(min(abs(r), BreathingDots.period - abs(r)), 0.01, "Beginn \(begin)")
        }
    }

    /// Flach und weich statt Blinken: die Deckkraft bleibt hoch, der Punkt wird nie viel kleiner als der Statuspunkt.
    func testBreathIsShallow() {
        XCTAssertGreaterThanOrEqual(BreathingDots.Pose.exhaled.opacity, 0.7, "der alte Puls ging bis 30 % hinunter")
        XCTAssertGreaterThanOrEqual(BreathingDots.Pose.exhaled.scale, 0.8)
        XCTAssertEqual(BreathingDots.Pose.inhaled.scale, 1, "eingeatmet so groß wie der Statuspunkt")
        XCTAssertEqual(BreathingDots.pose(at: 0), .inhaled)
        let mid = BreathingDots.pose(at: BreathingDots.period / 2)
        XCTAssertEqual(mid.scale, BreathingDots.Pose.exhaled.scale, accuracy: 0.001)
        XCTAssertEqual(mid.opacity, BreathingDots.Pose.exhaled.opacity, accuracy: 0.001)
    }

    func testReduceMotionStandsStillWithGlow() throws {
        try withReduceMotion(true) {
            let v = host(), dots = BreathingDots()
            dots.sync(["a": (CGRect(x: 10, y: 10, width: 8, height: 8), .systemBlue)], in: v)
            XCTAssertFalse(dots.isBreathing("a"))
            let layer = try XCTUnwrap(v.layer?.sublayers?.first?.sublayers?.first)
            XCTAssertEqual(layer.opacity, 1)
            XCTAssertGreaterThan(layer.shadowOpacity, 0.3, "Schein bleibt: die Form unterscheidet ihn vom Statuspunkt")

            Feedback.reduceMotion = false
            NotificationCenter.default.post(name: Feedback.reduceMotionChanged, object: nil)
            XCTAssertTrue(dots.isBreathing("a"), "Bedienungshilfe aus: atmet ohne Neuzeichnen wieder")
            Feedback.reduceMotion = true
            NotificationCenter.default.post(name: Feedback.reduceMotionChanged, object: nil)
            XCTAssertFalse(dots.isBreathing("a"))
        }
    }

    /// Baum: eine arbeitende, angehängte Session bekommt den Punkt an der Stelle ihres Statuspunkts, eine fertige nicht.
    func testSidebarBreathesForWorkingSession() {
        let busyId = Session.shellPrefix + "breath-busy", idleId = Session.shellPrefix + "breath-idle"
        let attach = AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ["SHELL": "/bin/sh"]))
        var busy = Session(id: busyId, cwd: NSTemporaryDirectory(), startedAt: 0, sessionId: busyId, name: "B")
        busy.rawStatus = "busy"
        let idle = Session(id: idleId, cwd: NSTemporaryDirectory(), startedAt: 0, sessionId: idleId, name: "I")
        attach.attachNow(busy)
        attach.attachNow(idle)
        defer { attach.stop(busyId); attach.stop(idleId) }
        let savedScale = Theme.scale
        defer { Theme.scale = savedScale }
        Theme.scale = 1
        let tree = SidebarView(frame: NSRect(x: 0, y: 0, width: 240, height: 200))
        tree.attach = attach
        tree.reload(groups: [Group(id: "g", name: "G", color: "#89b4fa", cwd: "/p", sessionIds: [busyId, idleId])], sessions: [busy, idle])
        tree.viewWillDraw()
        XCTAssertEqual(Set(tree.breathing.frames.keys), [busyId])
        let expected = ClassicSidebarRenderer().dotRect(tree.rowRect(1))
        XCTAssertEqual(tree.breathing.frames[busyId], tree.convertToLayer(expected))
        XCTAssertEqual(tree.breathing.color(busyId), Theme.running.cgColor)
    }
}
