import XCTest
@testable import Kadrell

final class ExtensionSupervisorTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_000_000)
    private func at(_ s: TimeInterval) -> Date { t0.addingTimeInterval(s) }

    private func running() -> ExtensionSupervisor {
        var s = ExtensionSupervisor()
        _ = s.handle(.enable, now: t0)
        _ = s.handle(.spawned, now: t0)
        _ = s.handle(.ready, now: t0)
        return s
    }

    /// Abstürzen, neu starten lassen und wieder bis `running` bringen (Neustart 1 s nach dem Absturz).
    private func crashAndRecover(_ s: inout ExtensionSupervisor, at t: TimeInterval) -> [SupervisorAction] {
        let actions = s.handle(.exited(expected: false, reason: "boom"), now: at(t))
        _ = s.handle(.restartDue, now: at(t + 1))
        _ = s.handle(.spawned, now: at(t + 1))
        _ = s.handle(.ready, now: at(t + 1))
        return actions
    }

    func testEnableSpawnsAndReadyRuns() {
        var s = ExtensionSupervisor()
        XCTAssertEqual(s.state, .off)
        XCTAssertEqual(s.handle(.enable, now: t0), [.spawn])
        XCTAssertEqual(s.state, .starting)
        _ = s.handle(.spawned, now: t0)
        _ = s.handle(.ready, now: t0)
        XCTAssertEqual(s.state, .running)
    }

    func testCrashBackoffSequence() {
        var s = running()
        XCTAssertEqual(crashAndRecover(&s, at: 0), [.clearUI, .scheduleRestart(1)])
        XCTAssertEqual(crashAndRecover(&s, at: 10), [.clearUI, .scheduleRestart(5)])
        XCTAssertEqual(s.handle(.exited(expected: false, reason: "boom"), now: at(20)), [.clearUI])
        guard case .failed(let reason) = s.state else { return XCTFail("\(s.state)") }
        XCTAssertTrue(reason.contains("3"), reason)
        XCTAssertEqual(s.handle(.restartDue, now: at(21)), [], "ein dauerhaft ausgeschalteter Neustart darf nichts starten")
    }

    func testCrashesOutsideWindowReset() {
        var s = running()
        for t in [0.0, 200, 400] {
            XCTAssertEqual(crashAndRecover(&s, at: t), [.clearUI, .scheduleRestart(1)], "t=\(t)")
        }
    }

    /// Stabile Läufe von unter 120 s setzen den Backoff nicht zurück, die 3 Abstürze liegen aber nicht in einem Fenster.
    func testThirtySecondBackoffReachable() {
        var s = running()
        XCTAssertEqual(crashAndRecover(&s, at: 0), [.clearUI, .scheduleRestart(1)])
        XCTAssertEqual(crashAndRecover(&s, at: 100), [.clearUI, .scheduleRestart(5)])
        XCTAssertEqual(crashAndRecover(&s, at: 210), [.clearUI, .scheduleRestart(30)])
        XCTAssertEqual(s.state, .running)
    }

    func testReloadDoesNotCount() {
        var s = running()
        for i in 0..<5 {
            let t = TimeInterval(i)
            XCTAssertEqual(s.handle(.reload, now: at(t)), [.terminate, .clearUI])
            XCTAssertEqual(s.state, .reloading)
            XCTAssertEqual(s.handle(.exited(expected: true, reason: ""), now: at(t)), [.spawn])
            _ = s.handle(.spawned, now: at(t))
            _ = s.handle(.ready, now: at(t))
            XCTAssertEqual(s.state, .running)
        }
    }

    func testDisableTerminatesAndClears() {
        var s = running()
        XCTAssertEqual(s.handle(.disable, now: t0), [.terminate, .clearUI])
        XCTAssertEqual(s.state, .off)
        XCTAssertEqual(s.handle(.exited(expected: true, reason: ""), now: t0), [])
        XCTAssertEqual(s.state, .off)
    }

    func testPingTimeoutCountsAsCrash() {
        var s = running()
        XCTAssertEqual(s.handle(.pingTimeout, now: t0), [.terminate, .clearUI, .scheduleRestart(1)])
        guard case .failed(let reason) = s.state else { return XCTFail("\(s.state)") }
        XCTAssertTrue(reason.contains("5 s"), reason)
        XCTAssertEqual(s.handle(.exited(expected: true, reason: ""), now: t0), [], "das erwartete Ende danach zählt nicht doppelt")
    }

    func testReadyTimeoutAndFloodCountAsCrash() {
        var s = ExtensionSupervisor()
        _ = s.handle(.enable, now: t0)
        XCTAssertEqual(s.handle(.readyTimeout, now: t0), [.terminate, .clearUI, .scheduleRestart(1)])
        _ = s.handle(.restartDue, now: at(1))
        _ = s.handle(.ready, now: at(1))
        XCTAssertEqual(s.handle(.flood("zu viele"), now: at(2)), [.terminate, .clearUI, .scheduleRestart(5)])
        XCTAssertEqual(s.state, .failed("zu viele"))
    }

    func testEnableFromFailedResetsCounter() {
        var s = running()
        _ = crashAndRecover(&s, at: 0)
        _ = crashAndRecover(&s, at: 10)
        _ = s.handle(.exited(expected: false, reason: "boom"), now: at(20))
        XCTAssertEqual(s.handle(.enable, now: at(30)), [.spawn])
        _ = s.handle(.ready, now: at(30))
        XCTAssertEqual(s.handle(.exited(expected: false, reason: "boom"), now: at(31)), [.clearUI, .scheduleRestart(1)])
    }

    func testRestartDueSpawns() {
        var s = running()
        _ = s.handle(.exited(expected: false, reason: "boom"), now: t0)
        XCTAssertEqual(s.handle(.restartDue, now: at(1)), [.spawn])
        XCTAssertEqual(s.state, .starting)
    }

    func testDisableWhileWaitingForRestartCancelsIt() {
        var s = running()
        _ = s.handle(.exited(expected: false, reason: "boom"), now: t0)
        XCTAssertEqual(s.handle(.disable, now: at(0.5)), [.clearUI])
        XCTAssertEqual(s.handle(.restartDue, now: at(1)), [])
    }

    private func failedPermanently() -> ExtensionSupervisor {
        var s = running()
        _ = crashAndRecover(&s, at: 0)
        _ = crashAndRecover(&s, at: 10)
        _ = s.handle(.exited(expected: false, reason: "boom"), now: at(20))
        return s
    }

    func testReloadInFailedWithPendingRestartSpawnsAndCancelsRestart() {
        var s = running()
        _ = s.handle(.exited(expected: false, reason: "boom"), now: t0)
        XCTAssertEqual(s.handle(.reload, now: at(0.5)), [.spawn])
        XCTAssertEqual(s.state, .starting)
        XCTAssertEqual(s.handle(.restartDue, now: at(1)), [])
    }

    func testReloadInPermanentFailedSpawns() {
        var s = failedPermanently()
        XCTAssertEqual(s.handle(.reload, now: at(30)), [.spawn])
        XCTAssertEqual(s.state, .starting)
    }

    func testReloadInFailedResetsCounters() {
        var s = failedPermanently()
        _ = s.handle(.reload, now: at(30))
        _ = s.handle(.spawned, now: at(30))
        XCTAssertEqual(s.handle(.exited(expected: false, reason: "boom"), now: at(31)), [.clearUI, .scheduleRestart(1)])
    }

    func testReloadInOffIsNoop() {
        var s = ExtensionSupervisor()
        XCTAssertEqual(s.handle(.reload, now: t0), [])
        XCTAssertEqual(s.state, .off)
    }

    func testReloadWhileStartingBehavesLikeRunning() {
        var s = ExtensionSupervisor()
        _ = s.handle(.enable, now: t0)
        XCTAssertEqual(s.handle(.reload, now: t0), [.terminate, .clearUI])
        XCTAssertEqual(s.state, .reloading)
    }

    func testEnableWhileRunningIsNoop() {
        var s = running()
        XCTAssertEqual(s.handle(.enable, now: t0), [])
        XCTAssertEqual(s.state, .running)
    }

    func testReadyIsIgnoredOutsideStarting() {
        var off = ExtensionSupervisor()
        XCTAssertEqual(off.handle(.ready, now: t0), [])
        XCTAssertEqual(off.state, .off)
        var failed = failedPermanently()
        let before = failed.state
        XCTAssertEqual(failed.handle(.ready, now: at(30)), [])
        XCTAssertEqual(failed.state, before)
        var reloading = running()
        _ = reloading.handle(.reload, now: t0)
        XCTAssertEqual(reloading.handle(.ready, now: t0), [])
        XCTAssertEqual(reloading.state, .reloading)
    }
}
