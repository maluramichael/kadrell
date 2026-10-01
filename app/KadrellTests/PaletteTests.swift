import XCTest
@testable import Kadrell

@MainActor
final class PaletteTests: XCTestCase {
    private func session(_ id: String, status: String?) -> Session {
        var s = Session(id: id, cwd: "/p", startedAt: 0, sessionId: id, name: id)
        s.rawStatus = status
        return s
    }

    private func open(_ palette: PaletteWindow, prefix: String) -> NSWindow {
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 600), styleMask: [.titled], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        palette.open(over: w, prefix: prefix)
        return w
    }

    private func wait(_ cond: @autoclosure () -> Bool) {
        let end = Date().addingTimeInterval(3)
        while !cond() && Date() < end { RunLoop.current.run(until: Date().addingTimeInterval(0.02)) }
    }

    func testDetachedWaitingSessionDoesNotJumpAhead() {
        let p = PaletteWindow()
        p.source.sessions = [("a", "waiting"), ("b", "idle"), ("c", "waiting")].map { (session($0.0, status: $0.1), nil, []) }
        p.source.isAttached = { $0 != "a" }
        let w = open(p, prefix: "")
        defer { p.close(); w.close() }
        XCTAssertEqual(p.items.first?.sessionKey, "c")
        XCTAssertEqual(p.items.first { $0.sessionKey == "a" }?.attached, false)
    }

    func testTerminalSearchFindsCaseInsensitiveAndUmlauts() {
        let p = PaletteWindow()
        p.source.buffers = { [(self.session("a", status: nil), nil, ["alles gut", "Ein FEHLER trat auf", "Ärger"])] }
        let w = open(p, prefix: "/fehler")
        defer { p.close(); w.close() }
        wait(!p.items.isEmpty)
        XCTAssertEqual(p.items.map(\.label), ["Ein FEHLER trat auf"])
    }

    func testTerminalSearchUmlaut() {
        let p = PaletteWindow()
        p.source.buffers = { [(self.session("a", status: nil), nil, ["Ärger im Büro"])] }
        let w = open(p, prefix: "/ärger")
        defer { p.close(); w.close() }
        wait(!p.items.isEmpty)
        XCTAssertEqual(p.items.count, 1)
    }

    func testUnknownHostOffersConnect() {
        let p = PaletteWindow()
        p.source.hosts = { ["other"] }
        var connected: (String, String?)?
        p.source.onConnect = { connected = ($0, $1) }
        let w = open(p, prefix: "@foo@bar.de")
        defer { p.close(); w.close() }
        XCTAssertEqual(p.items.count, 1)
        p.items.first?.run()
        XCTAssertEqual(connected?.0, "foo@bar.de")
        XCTAssertNil(connected?.1)
    }

    func testOptionLikeTextIsNotOffered() {
        let p = PaletteWindow()
        p.source.hosts = { ["other"] }
        let w = open(p, prefix: "@-oProxy")
        defer { p.close(); w.close() }
        XCTAssertFalse(p.items.contains { $0.label.hasPrefix("Verbinden") })
    }
}
