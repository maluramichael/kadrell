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

    /// Unter dem Körper liest sich die Kante wie jede waagrechte Fuge von oben nach unten dunkel, dann hell: der Körper
    /// lässt seine helle Unterkante weg (dunkel ist sein Layer-Rand), die Zeile darunter bringt die helle Linie mit.
    /// Ist der Körper das Ende der Spalte, behält er seinen Innenrand wie jede Kachel.
    func testSeamBelowStackBodyReadsDarkThenLight() {
        for scheme in ["instrument-dark", "instrument-light", "github-light"] {
            Theme.current = .named(scheme)
            stackWorkspace(gap: 6) { ws in
                let body = ws.subviews.compactMap { $0 as? CellView }.first { !$0.isHidden }!
                let below = ws.stackRows[1].0
                XCTAssertEqual(below.minY, body.frame.maxY, "\(scheme): Zeile B folgt direkt auf den Körper")
                XCTAssertTrue(body.state.rowBelow, "\(scheme)")
                let rep = render(ws), y = Int(body.frame.maxY)
                assertColor(rep, 150, y - 2, Theme.bg, "\(scheme): Körper ohne helle Unterkante")
                XCTAssertGreaterThan(luma(rep, x: 150, y: y), luma(Theme.panel) + 0.01, "\(scheme): helle Oberkante der Zeile unter dem Körper")
                assertColor(rep, 150, y + 1, Theme.panel, "\(scheme): darunter die Zeile")
                XCTAssertLessThan(luma(rep, x: Int(below.minX), y: y), luma(rep, x: Int(below.minX) + 1, y: y), "\(scheme): Seitenrand außen dunkel")
                assertColor(rep, 150, Int(ws.stackRows[0].0.maxY) - 1, Theme.surface, "\(scheme): Zeile über dem Körper ohne helle Unterkante")
                ws.setFocus("C", takeKeyboard: false)
                let last = ws.subviews.compactMap { $0 as? CellView }.first { !$0.isHidden }!
                XCTAssertFalse(last.state.rowBelow, "\(scheme): Körper am Ende der Spalte")
                let end = render(ws)
                // In GitHub Light ist der Körper reinweiß, heller geht nicht.
                if luma(Theme.bg) < 0.99 { XCTAssertGreaterThan(luma(end, x: 150, y: Int(last.frame.maxY) - 2), luma(Theme.bg), "\(scheme): dort heller Innenrand wie bei jeder Kachel") }
            }
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

    // MARK: Modul-Ecke beim Scrollen

    func testModuleMaskFollowsArc() throws {
        Theme.scale = 1                                                    // r = 14, Mitte (14, 14)
        let p = try XCTUnwrap(WorkspaceView.moduleMask(for: CGRect(x: -100, y: 6, width: 300, height: 200)))
        XCTAssertFalse(p.contains(CGPoint(x: 101, y: 1), using: .evenOdd), "Fläche (1, 7): außerhalb des Bogens")
        XCTAssertTrue(p.contains(CGPoint(x: 110, y: 2), using: .evenOdd), "Fläche (10, 8): innerhalb")
        XCTAssertTrue(p.contains(CGPoint(x: 150, y: 2), using: .evenOdd), "rechts der Ecke")
        XCTAssertNil(WorkspaceView.moduleMask(for: CGRect(x: 306, y: 6, width: 300, height: 200)))
        XCTAssertNil(WorkspaceView.moduleMask(for: CGRect(x: 6, y: 6, width: 300, height: 200)), "Kachel in der Ecke bei Abstand 6")
        XCTAssertNil(WorkspaceView.moduleMask(for: CGRect(x: -400, y: 6, width: 300, height: 200)), "ganz links hinaus")
    }

    /// Die Maske schneidet nur das Eckquadrat außerhalb des Bogens ab. Alles andere der Kachel bleibt, auch der Teil
    /// innerhalb des Kreises, der unter oder rechts vom Quadrat liegt (dort läge bei gerade-ungerade mit vollem Kreis ein Loch).
    func testModuleMaskKeepsEverythingButCornerCut() throws {
        Theme.scale = 1
        let frame = CGRect(x: -100, y: 0, width: 300, height: 200)
        let p = try XCTUnwrap(WorkspaceView.moduleMask(for: frame))
        func kept(_ x: CGFloat, _ y: CGFloat) -> Bool { p.contains(CGPoint(x: x - frame.minX, y: y - frame.minY), using: .evenOdd) }
        XCTAssertFalse(kept(1, 1), "Ecke außerhalb des Bogens")
        XCTAssertFalse(kept(7, 1), "oben nahe dem Bogen außerhalb")
        XCTAssertTrue(kept(10, 10), "im Bogen")
        XCTAssertTrue(kept(20, 20), "im Kreis, aber außerhalb des Quadrats")
        XCTAssertTrue(kept(25, 5), "außerhalb von Kreis und Quadrat")
        XCTAssertTrue(kept(5, 25), "unterhalb des Quadrats")
        XCTAssertTrue(kept(100, 100), "Mitte der Kachel")
        XCTAssertTrue(kept(-50, 100), "links der Arbeitsfläche bleibt der Kachel, die Fläche schneidet es ab")
    }

    func testScrolledTileUnderCornerIsMasked() {
        let oldGap = Settings.tileGap, oldColumns = Settings.scrollColumns
        let suffix = ".test-scroll-mask"
        let ws = WorkspaceView(frame: NSRect(x: 0, y: 0, width: 600, height: 400), defaultsSuffix: suffix)
        defer {
            Settings.tileGap = oldGap; Settings.scrollColumns = oldColumns
            ws.close()
            for k in ["workspace.selected", "workspace.mode", "workspace.auto"] { Profile.defaults.removeObject(forKey: k + suffix) }
        }
        Theme.scale = 1
        let ids = ["a", "b", "c"]
        let sessions = ids.map { Session(id: $0, cwd: "/p", startedAt: 0, sessionId: $0, name: $0) }
        ws.reload(groups: [Group(id: "g", name: "G", color: "#89b4fa", cwd: "/p", sessionIds: ids, favorite: false)], sessions: sessions)
        ws.setMode(.scroll)
        ws.setScrollColumns(2)
        ws.select(ids, add: false, takeKeyboard: false)
        func tiles() -> [CellView] {
            ws.subviews.compactMap { $0 as? CellView }.filter { !$0.isHidden }.sorted { $0.frame.minX < $1.frame.minX }
        }
        for gap in [0.0, 6.0] {
            Settings.tileGap = gap
            ws.scrollBy(-10_000)
            ws.relayout()
            var t = tiles()
            XCTAssertNil(t[0].layer?.mask, "gap \(gap): Kachel in der Ecke ohne Maske")
            XCTAssertGreaterThan(t[0].moduleCornerRadius, 0, "gap \(gap): konzentrischer Radius")
            ws.scrollBy(100)
            t = tiles()
            XCTAssertLessThan(t[0].frame.minX, 0, "gap \(gap): erste Kachel liegt links angeschnitten")
            let mask = t[0].layer?.mask as? CAShapeLayer
            XCTAssertNotNil(mask, "gap \(gap): angeschnittene Kachel hat Maske")
            XCTAssertEqual(mask?.fillRule, .evenOdd)
            XCTAssertEqual(mask?.frame, t[0].layer?.bounds)
            XCTAssertEqual(t[0].moduleCornerRadius, 0, "gap \(gap): eckig")
            XCTAssertNil(t[1].layer?.mask, "gap \(gap): zweite Kachel ohne Maske")
            ws.scrollBy(-100)
            t = tiles()
            XCTAssertNil(t[0].layer?.mask, "gap \(gap): wieder in der Ecke, Maske weg")
            XCTAssertGreaterThan(t[0].moduleCornerRadius, 0)
        }
    }

    /// Die Maske schneidet wirklich außerhalb des Bogens ab (Lage und Richtung der Ebene, nicht nur der Pfad).
    func testMaskedTileIsClippedOutsideArc() {
        Theme.scale = 1
        let size = CGSize(width: 160, height: 100), k = 4
        let c = cell()
        c.frame = CGRect(origin: .zero, size: size)
        c.moduleMask = WorkspaceView.moduleMask(for: CGRect(origin: .zero, size: size))
        let win = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: .borderless, backing: .buffered, defer: false)
        defer { win.close() }
        win.isReleasedWhenClosed = false
        let root = FlippedView(frame: CGRect(origin: .zero, size: size)); root.wantsLayer = true
        win.contentView = root
        root.addSubview(c)
        root.layoutSubtreeIfNeeded(); win.displayIfNeeded(); c.layer!.displayIfNeeded()
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width) * k, pixelsHigh: Int(size.height) * k, bitsPerSample: 8,
                                   samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        let cg = NSGraphicsContext(bitmapImageRep: rep)!.cgContext
        cg.scaleBy(x: CGFloat(k), y: CGFloat(k))
        cg.translateBy(x: 0, y: size.height); cg.scaleBy(x: 1, y: -1)
        root.layer!.render(in: cg)
        func alphaAt(_ x: CGFloat, _ y: CGFloat) -> CGFloat { rep.colorAt(x: Int(x * CGFloat(k)), y: Int(y * CGFloat(k)))?.alphaComponent ?? -1 }
        XCTAssertEqual(alphaAt(1, 1), 0, accuracy: 0.02, "Ecke außerhalb des Bogens")
        XCTAssertEqual(alphaAt(1, 40), 1, accuracy: 0.02, "links unterhalb der Ecke bleibt")
        XCTAssertEqual(alphaAt(40, 1), 1, accuracy: 0.02, "oben rechts der Ecke bleibt")
        XCTAssertEqual(alphaAt(80, 50), 1, accuracy: 0.02, "Mitte bleibt")
    }

    /// Die Maske folgt dem Scrollen sofort: Pfad, Lage und Größe werden nicht implizit 0,25 s lang überblendet.
    func testModuleMaskDoesNotAnimate() throws {
        Theme.scale = 1
        let size = CGSize(width: 240, height: 120)
        let c = cell()
        c.frame = CGRect(origin: .zero, size: size)
        let win = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: .borderless, backing: .buffered, defer: false)
        defer { win.close() }
        win.isReleasedWhenClosed = false
        let root = FlippedView(frame: CGRect(origin: .zero, size: size)); root.wantsLayer = true
        win.contentView = root
        root.addSubview(c)
        c.moduleMask = WorkspaceView.moduleMask(for: CGRect(x: -40, y: 0, width: 240, height: 120))
        root.layoutSubtreeIfNeeded(); win.displayIfNeeded(); CATransaction.flush()
        let shape = try XCTUnwrap(c.layer?.mask as? CAShapeLayer)
        c.frame = CGRect(x: 0, y: 0, width: 300, height: 140)
        c.moduleMask = WorkspaceView.moduleMask(for: CGRect(x: -80, y: 0, width: 300, height: 140))
        XCTAssertTrue((c.layer?.mask as? CAShapeLayer) === shape, "dieselbe Masken-Ebene wird weiterverwendet")
        XCTAssertEqual(shape.frame.size, CGSize(width: 300, height: 140))
        XCTAssertTrue(shape.animationKeys()?.isEmpty ?? true, "keine laufende Animation: \(shape.animationKeys() ?? [])")
    }

    /// Die Striche ändern sich nur alle 160 ms: der 80-ms-Takt zeichnet nur bei einem neuen Schritt neu.
    func testMeterRedrawsOnlyWhenStepChanges() {
        let ws = WorkspaceView(frame: NSRect(x: 0, y: 0, width: 600, height: 400), defaultsSuffix: ".test-meter-step")
        defer { ws.close() }
        XCTAssertTrue(ws.meterStepChanged(1), "erster Schritt")
        XCTAssertFalse(ws.meterStepChanged(1), "zweiter Takt im selben Schritt")
        XCTAssertTrue(ws.meterStepChanged(2), "neuer Schritt")
        XCTAssertFalse(ws.meterStepChanged(2))
        XCTAssertTrue(ws.meterStepChanged(nil), "Wechsel zu „Bewegung reduzieren“ zeichnet einmal")
        XCTAssertFalse(ws.meterStepChanged(nil), "bei „Bewegung reduzieren“ nie wieder")
    }

    /// Ein langes Gruppen-Label drückt in Stack-Zeilen nicht in den Titel: es wird mit Auslassung gekürzt.
    func testStackGroupLabelTruncates() {
        let short = WorkspaceView.stackGroupLabel("Web", maxWidth: 200)
        XCTAssertEqual(short.string, "WEB", "kurzes Label bleibt ganz")
        let long = WorkspaceView.stackGroupLabel("Ein sehr langer Gruppenname", maxWidth: 80)
        XCTAssertLessThanOrEqual(long.size().width, 80)
        XCTAssertTrue(long.string.hasSuffix("…"), long.string)
        XCTAssertTrue(long.string.hasPrefix("EIN"), long.string)
        XCTAssertGreaterThan(long.string.count, 1)
    }
}
