import XCTest
@testable import Kadrell

/// Kachel im Instrument-Look: neutraler Kopf mit Gruppen-Akzent, Fuge unter dem Kopf, Fuge als Rand und Strich-Anzeige
/// statt pulsierendem Punkt. Gezeichnet wird `draw` direkt, 1 px = 1 View-Punkt (die Kachel skaliert selbst).
@MainActor
final class TileTests: XCTestCase {
    private let dark = ColorTheme.named("instrument-dark")
    private var oldTheme = Theme.current, oldScale = Theme.scale, oldOpacity = Settings.tileOpacity, oldMotion = Feedback.reduceMotion

    override func setUp() async throws {
        try await super.setUp()
        oldTheme = Theme.current; oldScale = Theme.scale; oldOpacity = Settings.tileOpacity; oldMotion = Feedback.reduceMotion
        Theme.current = dark
        Settings.tileOpacity = 1
        Feedback.reduceMotion = true
    }

    override func tearDown() async throws {
        Theme.current = oldTheme; Theme.scale = oldScale; Settings.tileOpacity = oldOpacity; Feedback.reduceMotion = oldMotion
        try await super.tearDown()
    }

    /// `status` ist der Rohwert wie von Claude Code („busy“, „waiting“, „idle“).
    private func cell(status: String? = nil, shell: Bool = false, attached: Bool = true, groupName: String = "",
                      groupColor: NSColor = NSColor(hex: 0x6ea8fe), focused: Bool = false) -> CellView {
        var s = Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "A")
        s.rawStatus = status
        s.hasRunningShell = shell
        let v = CellView(session: s)
        v.frame = CGRect(x: 0, y: 0, width: 240, height: 120)
        v.state = CellView.State(session: s, groupName: groupName, groupColor: groupColor, focused: focused, attached: attached)
        return v
    }

    private func renderCell(_ cell: CellView, scale: CGFloat = 1) -> NSBitmapImageRep {
        Theme.scale = scale
        let w = Int(cell.bounds.width), h = Int(cell.bounds.height)
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h, bitsPerSample: 8, samplesPerPixel: 4,
                                   hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        let cg = NSGraphicsContext(bitmapImageRep: rep)!.cgContext
        cg.translateBy(x: 0, y: CGFloat(h))
        cg.scaleBy(x: 1, y: -1)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: cg, flipped: true)
        cell.draw(cell.bounds)
        NSGraphicsContext.restoreGraphicsState()
        return rep
    }

    private func assertColor(_ rep: NSBitmapImageRep, _ x: Int, _ y: Int, _ expected: NSColor, _ msg: String,
                             file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertLessThan(distance(pixel(rep, x: x, y: y), expected), 0.03, "\(msg) bei \(x),\(y): \(pixel(rep, x: x, y: y).hexString)",
                          file: file, line: line)
    }

    func testTileHeaderIsNeutral() {
        for color in [NSColor(hex: 0xff0000), NSColor(hex: 0x0000ff)] {
            for focused in [false, true] {
                let rep = renderCell(cell(groupName: "kadrell", groupColor: color, focused: focused))
                assertColor(rep, 200, 3, Theme.surface, "Kopf neutral (fokussiert: \(focused))")
            }
        }
    }

    func testTileHeaderShowsGroupAccent() {
        let red = NSColor(hex: 0xff0000)
        let rep = renderCell(cell(groupName: "kadrell", groupColor: red))
        let accent = (0..<240).contains { distance(pixel(rep, x: $0, y: 13), red) < 0.05 }
        XCTAssertTrue(accent, "Akzent in Gruppenfarbe in Zeile 13")
        let tinted = (0..<240).contains { distance(pixel(rep, x: $0, y: 3), red) < 0.3 }
        XCTAssertFalse(tinted, "Kopf nicht getönt")
    }

    func testTileHeaderSeamUnderHeader() {
        let rep = renderCell(cell())
        XCTAssertLessThan(luma(rep, x: 200, y: 24), luma(Theme.surface), "dunkle Haarlinie")
        XCTAssertGreaterThan(luma(rep, x: 200, y: 25), luma(rep, x: 200, y: 24) * 1.15, "helle Haarlinie")
        assertColor(rep, 200, 26, Theme.bg, "Körper direkt unter der Fuge")
    }

    func testHeaderSeamStaysHairlineAtScale() {
        let rep = renderCell(cell(), scale: 1.3)
        assertColor(rep, 200, 31, Theme.surface, "Kopf über der Fuge")
        XCTAssertLessThan(luma(rep, x: 200, y: 32), luma(Theme.surface), "dunkle Haarlinie")
        XCTAssertGreaterThan(luma(rep, x: 200, y: 33), luma(rep, x: 200, y: 32) * 1.15, "helle Haarlinie")
        assertColor(rep, 200, 34, Theme.bg, "Körper direkt unter der Fuge")
    }

    func testTileBodyIsNeutral() {
        for color in [NSColor(hex: 0xff0000), NSColor(hex: 0x0000ff)] {
            XCTAssertEqual(cell(groupColor: color).bodyColor, Theme.bg.withAlphaComponent(CGFloat(Settings.tileOpacity)))
        }
    }

    func testTileRimIsSeam() {
        let plain = cell()
        plain.viewWillDraw()
        XCTAssertEqual(plain.layer?.borderWidth, 1)
        XCTAssertEqual(plain.layer?.borderColor, Instrument.seamDark.cgColor)
        let rep = renderCell(plain)
        XCTAssertGreaterThan(luma(rep, x: 1, y: 80), luma(rep, x: 3, y: 80), "heller Innenrand")
        let focused = cell(focused: true)
        focused.viewWillDraw()
        XCTAssertEqual(focused.layer?.borderWidth, 2)
        XCTAssertEqual(focused.layer?.borderColor, Theme.fg.cgColor)
    }

    func testWorkingTileShowsMeter() {
        let rep = renderCell(cell(status: "busy"))
        for x in [7, 10, 13, 16] { assertColor(rep, x, 13, Theme.running, "Strich") }
        for x in [9, 12, 15] { assertColor(rep, x, 13, Theme.surface, "Lücke") }
    }

    func testWaitingTileKeepsDot() {
        let rep = renderCell(cell(status: "waiting"))
        assertColor(rep, 13, 13, Theme.waiting, "Punkt in Warte-Farbe")
        assertColor(rep, 8, 13, Theme.surface, "kein Strich links")
    }

    func testWaitingWithShellTileDotIsWaitingColor() {
        let rep = renderCell(cell(status: "waiting", shell: true))
        assertColor(rep, 13, 13, NSColor(hex: 0xff5e14), "Warte-Farbe schlägt Shell-Farbe")
    }

    // MARK: Stack-Zeilen

    /// Arbeitsfläche 400 × 300 im Stack mit drei Sessions in Gruppe „G“ (rot), Fokus auf der ersten. Mit `attach`
    /// werden die Sessions als Shells gestartet (angehängt), dann mit eigenem Ordner statt „/p“.
    private func stackWorkspace(gap: Double, ids: [String] = ["A", "B", "C"], status: [String: String] = [:], attach: AttachManager? = nil,
                                _ body: (WorkspaceView) -> Void) {
        let oldGap = Settings.tileGap, suffix = ".test-stack-rows"
        let ws = WorkspaceView(frame: NSRect(x: 0, y: 0, width: 400, height: 300), defaultsSuffix: suffix)
        defer {
            Settings.tileGap = oldGap
            for id in ids { attach?.stop(id) }
            ws.close()
            for k in ["workspace.selected", "workspace.mode", "workspace.auto"] { Profile.defaults.removeObject(forKey: k + suffix) }
        }
        Theme.scale = 1
        Settings.tileGap = gap
        ws.attach = attach
        let cwd = attach == nil ? "/p" : NSTemporaryDirectory()
        let sessions = ids.map { id in
            var s = Session(id: id, cwd: cwd, startedAt: 0, sessionId: id, name: id)
            s.rawStatus = status[id]
            return s
        }
        let group = Group(id: "g", name: "G", color: "#ff0000", cwd: "/p", sessionIds: ids, favorite: false)
        ws.reload(groups: [group], sessions: sessions)
        ws.setMode(.stack)
        ws.select(ids, add: false, takeKeyboard: false)
        body(ws)
    }

    private func render(_ view: NSView) -> NSBitmapImageRep {
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(view.bounds.width), pixelsHigh: Int(view.bounds.height),
                                   bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                   bytesPerRow: 0, bitsPerPixel: 0)!
        rep.size = view.bounds.size
        view.cacheDisplay(in: view.bounds, to: rep)
        return rep
    }

    func testStackRowsAreNeutralWithSeams() {
        stackWorkspace(gap: 6) { ws in
            let rows = ws.stackRows.map(\.0)
            XCTAssertEqual(rows.count, 3)
            let rep = render(ws)
            assertColor(rep, 150, Int(rows[0].midY), Theme.surface, "fokussierte Zeile neutral")
            assertColor(rep, 150, Int(rows[2].midY), Theme.panel, "übrige Zeile neutral")
            let below1 = Int(rows[1].maxY)
            XCTAssertLessThan(luma(rep, x: 150, y: below1 - 2), luma(Theme.panel), "dunkle Haarlinie unter Zeile 1")
            XCTAssertGreaterThan(luma(rep, x: 150, y: below1 - 1), luma(Theme.panel), "helle Haarlinie unter Zeile 1")
            assertColor(rep, 150, Int(rows[0].maxY) - 2, Theme.surface, "über dem Körper keine eigene Fuge")
            let left = Int(rows[2].minX), mid = Int(rows[2].midY)
            XCTAssertLessThan(luma(rep, x: left, y: mid), luma(Theme.panel), "Rand außen dunkel")
            XCTAssertGreaterThan(luma(rep, x: left + 1, y: mid), luma(rep, x: left, y: mid), "Rand innen hell")
        }
    }

    func testStackRowShowsMeterForWorkingSession() {
        let ids = ["F", "W", "Q"].map { Session.shellPrefix + "stack-row-\($0)" }
        let attach = AttachManager(cli: ClaudeCLI(binary: "/usr/bin/false", environment: ["SHELL": "/bin/sh"]))
        stackWorkspace(gap: 6, ids: ids, status: [ids[1]: "busy", ids[2]: "waiting"], attach: attach) { ws in
            let rows = ws.stackRows.map(\.0)
            XCTAssertTrue(ids.allSatisfy(attach.isAttached))
            let rep = render(ws)
            let busy = rows[1], waiting = rows[2], minX = Int(busy.minX)
            for dx in [11, 14, 17, 20] { assertColor(rep, minX + dx, Int(busy.midY), Theme.running, "Strich") }
            assertColor(rep, minX + 12, Int(busy.midY), Theme.panel, "Lücke")
            assertColor(rep, Int(waiting.minX) + 16, Int(waiting.midY), Theme.waiting, "Punkt in Warte-Farbe")
        }
    }

    func testFirstStackRowFollowsModuleCorner() {
        stackWorkspace(gap: 0) { ws in
            let rep = render(ws)
            XCTAssertLessThan(rep.colorAt(x: 1, y: 1)!.alphaComponent, 0.05, "außerhalb des Bogens durchsichtig")
            assertColor(rep, 14, 4, Theme.surface, "Zeilenfläche im Bogen")
        }
        stackWorkspace(gap: 6) { ws in
            let rep = render(ws)
            XCTAssertLessThan(rep.colorAt(x: 7, y: 7)!.alphaComponent, 0.05, "außerhalb des Bogens durchsichtig")
            assertColor(rep, 20, 8, Theme.surface, "Zeilenfläche im Bogen")
        }
    }
}
