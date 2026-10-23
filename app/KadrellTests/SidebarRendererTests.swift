import XCTest
@testable import Kadrell

/// Aussehen der Baumzeilen: Renderer direkt in ein Bitmap gezeichnet (Maßstab 1, 1 pt = 1 px), auf dem Gehäuse wie in
/// `SidebarView`. Gruppen-Labels, Auswahl-Umriss, Tastaturring und die Fuge zwischen Gruppen im Kompakt-Design.
@MainActor
final class SidebarRendererTests: XCTestCase {
    private let dark = ColorTheme.named("instrument-dark")

    private func item(name: String = "kadrell", sessions: Int = 2, color: NSColor = NSColor(hex: 0x6ea8fe), favorite: Bool = false,
                      waiting: Int = 0, first: Bool = true, hover: Bool = false, selected: Bool = false) -> SidebarGroupItem {
        let ids = (0..<sessions).map { "s\($0)" }
        let g = Group(id: "g", name: name, color: "#6ea8fe", cwd: "/p", sessionIds: ids, favorite: favorite)
        return SidebarGroupItem(group: g, color: color, dots: ids.map { _ in Theme.idle }, open: true, selected: selected,
                                hover: hover, first: first, waitingCount: waiting)
    }

    /// `status` ist der Rohwert wie von Claude Code („busy“, „waiting“, „idle“); `lit` = nil lässt alle vier Striche leuchten.
    private func session(focused: Bool = false, keyFocus: Bool = false, selected: Bool = false, hover: Bool = false,
                         unread: Bool = false, color: NSColor = NSColor(hex: 0x6ea8fe), status: String? = nil,
                         shellRunning: Bool = false, attached: Bool = true, lit: Int? = nil,
                         message: String? = nil) -> SidebarSessionItem {
        var s = Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "A")
        s.rawStatus = status
        s.hasRunningShell = shellRunning
        return SidebarSessionItem(session: s, color: color, dot: Theme.statusColor(s.status, attached: attached, shellRunning: shellRunning),
                                  meter: SidebarSessionItem.meter(for: s, attached: attached, lit: lit), attached: attached,
                                  selected: selected, focused: focused, keyFocus: keyFocus, hover: hover, message: message,
                                  showAge: false, unread: unread)
    }

    /// Eine Session-Zeile 200 × 24 bei y = 10 auf einer 200 × 44 großen Gehäusefläche.
    private func renderSession(_ s: @autoclosure () -> SidebarSessionItem, theme: ColorTheme,
                               renderer: any SidebarRenderer = ClassicSidebarRenderer()) -> NSBitmapImageRep {
        render(theme: theme, width: 200, height: 44, scale: 1) {
            panelCanvas(200, 44)
            renderer.drawSession(s(), in: CGRect(x: 0, y: 10, width: 200, height: 24))
        }
    }

    private func assertColor(_ rep: NSBitmapImageRep, _ x: Int, _ y: Int, _ expected: NSColor, _ msg: String,
                             file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertLessThan(distance(pixel(rep, x: x, y: y), expected), 0.03, "\(msg) bei \(x),\(y): \(pixel(rep, x: x, y: y).hexString)",
                          file: file, line: line)
    }

    func testGroupLabelIsUppercaseWithCount() {
        let label = ClassicSidebarRenderer.groupLabel(item(name: "kadrell", sessions: 2))
        XCTAssertEqual(label.string, "KADRELL · 2")
        XCTAssertEqual(label.attribute(.kern, at: 0, effectiveRange: nil) as? CGFloat, 0.8)
        withTheme(dark) {
            let l = ClassicSidebarRenderer.groupLabel(item())
            XCTAssertEqual(l.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor, Theme.sub)
            XCTAssertEqual((l.attribute(.font, at: 0, effectiveRange: nil) as? NSFont)?.pointSize, 10.5)
        }
    }

    func testFocusedRowHasOutline() {
        let rep = renderSession(session(focused: true), theme: dark)
        withTheme(dark) {
            assertColor(rep, 6, 22, Theme.fg, "Umriss links")
            assertColor(rep, 100, 11, Theme.fg, "Umriss oben")
            assertColor(rep, 193, 22, Theme.fg, "Umriss rechts")
            assertColor(rep, 100, 22, Theme.bg, "innen Modulfarbe")
            assertColor(rep, 8, 22, Theme.bg, "Umriss nur 1,5 pt breit")
            assertColor(rep, 3, 22, Theme.panel, "links vom Umriss Gehäuse")
            assertColor(rep, 100, 9, Theme.panel, "über der Zeile nichts")
        }
    }

    func testKeyFocusRingOutsideOutline() {
        for t in ColorTheme.all {
            let rep = renderSession(session(focused: true, keyFocus: true), theme: t)
            withTheme(t) {
                XCTAssertGreaterThanOrEqual(pixel(rep, x: 4, y: 22).contrastRatio(with: Theme.panel), 3, "\(t.id): Ring hebt sich vom Gehäuse ab")
                XCTAssertGreaterThanOrEqual(pixel(rep, x: 195, y: 22).contrastRatio(with: Theme.panel), 3, "\(t.id): Ring rechts")
                XCTAssertGreaterThanOrEqual(pixel(rep, x: 100, y: 11).contrastRatio(with: Theme.panel), 3, "\(t.id): Ring oben, 2 pt wie an den Seiten")
                assertColor(rep, 6, 22, Theme.fg, "\(t.id): Umriss bleibt fg")
                assertColor(rep, 100, 13, Theme.fg, "\(t.id): Umriss oben unter Ring und Luft")
                assertColor(rep, 100, 22, Theme.bg, "\(t.id): innen Modulfarbe")
                assertColor(rep, 1, 22, Theme.panel, "\(t.id): links vom Ring Gehäuse")
                assertColor(rep, 100, 9, Theme.panel, "\(t.id): Ring bleibt in der Zeile (oben)")
                assertColor(rep, 100, 34, Theme.panel, "\(t.id): Ring bleibt in der Zeile (unten)")
            }
        }
    }

    /// Kompakt hat 22 pt Zeilenhöhe: mit Ring, Luft und Umriss wird die Marke „neu“ flacher statt über den Umriss zu ragen.
    func testUnreadPillStaysInsideKeyFocusOutline() {
        let renderer = CompactSidebarRenderer()
        let rep = render(theme: dark, width: 200, height: 44, scale: 1) {
            panelCanvas(200, 44)
            renderer.drawSession(session(focused: true, keyFocus: true, unread: true), in: CGRect(x: 0, y: 10, width: 200, height: 22))
        }
        withTheme(dark) {
            // Fläche 13 bis 29, Umriss oben 13 bis 14,5, unten 27,5 bis 29; die Marke endet bei x = 190.
            assertColor(rep, 185, 21, Theme.running, "Marke in der Mitte")
            assertColor(rep, 185, 13, Theme.fg, "Umriss über der Marke durchgehend")
            assertColor(rep, 185, 28, Theme.fg, "Umriss unter der Marke durchgehend")
        }
    }

    /// Nachrichtenzeile mit Tastaturring: die Unterlängen („gjpqy“) halten mindestens 1,5 pt Abstand zur Innenkante des
    /// Umrisses, auch bei 130 %. Gemessen als Unterschied zur selben Zeile ohne Nachricht, vierfach gerastert.
    func testMessageDescendersClearKeyFocusOutline() {
        for renderer in [ClassicSidebarRenderer(), CompactSidebarRenderer()] as [any SidebarRenderer] {
            for ui: CGFloat in [1, 1.3] {
                let k = 4 * ui, h = renderer.sessionRow + renderer.messageLine
                func row(_ m: String?) -> NSBitmapImageRep {
                    render(theme: dark, width: Int(200 * k), height: Int((h * k).rounded(.up)), scale: k) {
                        panelCanvas(200, h)
                        renderer.drawSession(session(focused: true, keyFocus: true, message: m), in: CGRect(x: 0, y: 0, width: 200, height: h))
                    }
                }
                let with = row("gjpqy gjpqy gjpqy"), without = row(nil)
                // Innenkante des Umrisses unten: Fläche endet 3 pt über der Zeilenkante, Umriss 1,5 pt breit.
                let inner = h - 3 - 1.5
                let band = Int(((inner - 1.5) * k).rounded(.down))..<Int((inner * k).rounded(.up))
                let ink = band.filter { y in (Int(42 * k)..<Int(150 * k)).contains { distance(pixel(with, x: $0, y: y), pixel(without, x: $0, y: y)) > 0.05 } }
                XCTAssertTrue(ink.isEmpty, "\(renderer) \(ui): Unterlängen im Band unter der Nachricht bei y = \(ink.map { CGFloat($0) / k })")
            }
        }
    }

    func testFocusWithoutKeyFocusHasNoRing() {
        let rep = renderSession(session(focused: true), theme: dark)
        withTheme(dark) { assertColor(rep, 4, 22, Theme.panel, "ohne Tastatur kein Ring") }
    }

    func testHoverAndSelectionFillShapeWithModuleColour() {
        let groupColor = NSColor(hex: 0xff0000)
        let hover = renderSession(session(hover: true), theme: dark)
        let selected = renderSession(session(selected: true, color: groupColor), theme: dark)
        withTheme(dark) {
            assertColor(hover, 100, 22, Theme.bg, "Hover: Fläche in Modulfarbe")
            assertColor(hover, 7, 22, Theme.bg, "Hover: ohne Umriss")
            assertColor(hover, 3, 22, Theme.panel, "Hover: Fläche eingerückt")
            assertColor(hover, 100, 10, Theme.panel, "Hover: 1 pt Luft oben")
            assertColor(selected, 100, 22, Theme.bg, "Auswahl: Fläche in Modulfarbe")
            assertColor(selected, 1, 22, groupColor, "Auswahl: Balken links in Gruppenfarbe")
            assertColor(selected, 4, 22, Theme.panel, "Auswahl: Balken nur 3 pt")
        }
    }

    func testGroupHeadHasAccentLabelAndNoSeparateCount() {
        let accent = NSColor(hex: 0xff0000)
        for renderer in [ClassicSidebarRenderer(), CompactSidebarRenderer()] as [any SidebarRenderer] {
            let rep = render(theme: dark, width: 240, height: 50, scale: 1) {
                panelCanvas(240, 50)
                renderer.drawGroup(item(color: accent, hover: true), in: CGRect(x: 0, y: 0, width: 240, height: renderer.groupRow))
            }
            let mid = Int(renderer.headRect(CGRect(x: 0, y: 0, width: 240, height: 24)).midY)
            withTheme(dark) {
                assertColor(rep, 29, mid, accent, "\(renderer): Farbakzent bei x = 26 bis 32")
                assertColor(rep, 33, mid, Theme.bg, "\(renderer): Akzent nur 6 pt breit")
            }
            let strayInk = (150..<234).filter { x in (mid - 4...mid + 4).contains { distance(pixel(rep, x: x, y: $0), dark.bg) > 0.02 } }
            XCTAssertTrue(strayInk.isEmpty, "\(renderer): rechts keine Zahl mehr (Hover-Fläche endet bei x = 234), Tinte bei x = \(strayInk)")
            let ink = (37..<44).contains { x in (mid - 4...mid + 4).contains { distance(pixel(rep, x: x, y: $0), dark.bg) > 0.2 } }
            XCTAssertTrue(ink, "\(renderer): Label beginnt bei x = 37")
        }
    }

    func testClassicKeepsPathLineBelowHead() {
        let rep = render(theme: dark, width: 240, height: 38, scale: 1) {
            panelCanvas(240, 38)
            ClassicSidebarRenderer().drawGroup(item(), in: CGRect(x: 0, y: 0, width: 240, height: 38))
        }
        func inked(_ xs: Range<Int>) -> Bool { xs.contains { x in (24..<34).contains { distance(pixel(rep, x: x, y: $0), dark.panel) > 0.1 } } }
        XCTAssertTrue(inked(42..<56), "Pfad „/p“ unter dem Kopf ab x = 42")
        XCTAssertFalse(inked(0..<41), "links vom Pfad nichts")
    }

    func testLongGroupLabelTruncatesBeforeBadges() {
        let g = item(name: String(repeating: "x", count: 80), favorite: true, waiting: 2)
        let renderer = ClassicSidebarRenderer()
        let row = CGRect(x: 0, y: 0, width: 240, height: 38), head = renderer.headRect(row)
        var label = CGRect.zero, badgeRight: CGFloat = 0
        let rep = render(theme: dark, width: 240, height: 38, scale: 1) {
            panelCanvas(240, 38)
            renderer.drawGroup(g, in: row)
            badgeRight = renderer.drawFavorite(g, head: head)
            label = renderer.drawGroupName(g, head: head, right: renderer.drawWaitingBadge(g, head: head, right: badgeRight))
        }
        // Warte-Marke im Bild: erste Spalte rechts vom Labelanfang mit deutlich oranger Tinte (Rot weit über Blau).
        let band = Int(head.midY) - 6...Int(head.midY) + 6
        func orange(_ x: Int) -> Bool { band.contains { let c = pixel(rep, x: x, y: $0); return c.redComponent - c.blueComponent > 0.3 } }
        func inked(_ x: Int) -> Bool { band.contains { distance(pixel(rep, x: x, y: $0), dark.panel) > 0.1 } }
        guard let orangeLeft = (37..<240).first(where: orange) else { return XCTFail("Warte-Marke nicht gezeichnet") }
        // Kantenglättung: der linke Rand der Marke ist blasser, die Marke beginnt am Anfang ihres Tintenlaufs.
        var badgeLeft = orangeLeft
        while inked(badgeLeft - 1) { badgeLeft -= 1 }
        let labelInkRight = (37..<badgeLeft).last(where: inked) ?? 0
        XCTAssertEqual(label.minX, 37)
        XCTAssertLessThan(label.maxX, CGFloat(badgeLeft), "Label endet vor der Warte-Marke")
        XCTAssertLessThan(labelInkRight, badgeLeft - 4, "sichtbare Lücke zwischen Label und Marke")
        XCTAssertGreaterThan(labelInkRight, badgeLeft - 20, "Label nutzt den Platz bis zur Marke (gekürzt, nicht verschwunden)")
        XCTAssertLessThanOrEqual(CGFloat(labelInkRight), label.maxX + 1, "gezeichnete Tinte liegt in der gemeldeten Fläche")
    }

    /// Langer Name, Warte-Marke und Favorit: gekürzt wird nur der Name, „· N“ bleibt vor der Marke sichtbar. Zwei Bilder,
    /// die sich nur in der Zahl der Sessions unterscheiden, müssen sich direkt vor der Marke unterscheiden.
    func testLongGroupLabelKeepsCountVisible() {
        let renderers: [any SidebarRenderer] = [ClassicSidebarRenderer(), CompactSidebarRenderer()]
        for renderer in renderers {
            let row = CGRect(x: 0, y: 0, width: 240, height: renderer.groupRow)
            let head = renderer is ClassicSidebarRenderer ? renderer.headRect(row) : row
            func draw(_ sessions: Int) -> NSBitmapImageRep {
                render(theme: dark, width: 240, height: Int(row.height), scale: 1) {
                    panelCanvas(240, row.height)
                    renderer.drawGroup(item(name: String(repeating: "x", count: 80), sessions: sessions, favorite: true, waiting: 2), in: row)
                }
            }
            let two = draw(2), seven = draw(7)
            let band = Int(head.midY) - 6...Int(head.midY) + 6
            func orange(_ x: Int) -> Bool { band.contains { let c = pixel(two, x: x, y: $0); return c.redComponent - c.blueComponent > 0.3 } }
            guard let badgeLeft = (37..<240).first(where: orange) else { XCTFail("\(renderer): Warte-Marke fehlt"); continue }
            let differs = (37..<badgeLeft).filter { x in band.contains { distance(pixel(two, x: x, y: $0), pixel(seven, x: x, y: $0)) > 0.1 } }
            guard let countLeft = differs.first else { XCTFail("\(renderer): Zahl der Sessions nicht sichtbar"); continue }
            XCTAssertGreaterThan(countLeft, badgeLeft - 24, "\(renderer): Zahl steht direkt vor der Marke")
            // Gekürzt wird der Name: zwischen Labelanfang und Zahl steht Tinte bis kurz vor die Zahl.
            func inked(_ x: Int) -> Bool { band.contains { distance(pixel(two, x: x, y: $0), dark.panel) > 0.1 } }
            XCTAssertTrue(inked(37) || inked(38), "\(renderer): Name beginnt bei x = 37")
            XCTAssertTrue((countLeft - 30..<countLeft).contains(where: inked), "\(renderer): Name reicht bis an die Zahl")
        }
    }

    func testCompactGroupSeparatorIsSeam() {
        let renderer = CompactSidebarRenderer()
        for t in ColorTheme.all {
            let rep = render(theme: t, width: 200, height: 40, scale: 1) {
                panelCanvas(200, 40)
                renderer.drawGroup(item(first: false), in: CGRect(x: 0, y: 10, width: 200, height: renderer.groupRow))
            }
            let firstRep = render(theme: t, width: 200, height: 40, scale: 1) {
                panelCanvas(200, 40)
                renderer.drawGroup(item(first: true), in: CGRect(x: 0, y: 10, width: 200, height: renderer.groupRow))
            }
            withTheme(t) {
                XCTAssertLessThan(luma(rep, x: 100, y: 10), luma(Theme.panel), "\(t.id): Oberkante dunkel")
                XCTAssertGreaterThan(luma(rep, x: 100, y: 11), luma(rep, x: 100, y: 10), "\(t.id): darunter hell")
                assertColor(rep, 100, 12, Theme.panel, "\(t.id): Fuge nur 2 pt")
                assertColor(firstRep, 100, 10, Theme.panel, "\(t.id): erste Gruppe ohne Fuge")
            }
        }
    }

    func testFocusedSessionTitleIsBright() {
        func titleColor(_ s: SidebarSessionItem) -> NSColor? {
            ClassicSidebarRenderer().sessionTitle(s).attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        }
        withTheme(dark) {
            XCTAssertEqual(titleColor(session(focused: true)), Theme.fg, "gezeigte Session: heller Titel")
            XCTAssertEqual(titleColor(session()), Theme.sub, "Ruhezustand bleibt gedämpft")
        }
    }

    // MARK: Strich-Anzeige

    func testWorkingSessionShowsMeterNotDot() {
        let rep = renderSession(session(status: "busy"), theme: dark)
        withTheme(dark) {
            // Striche 2 pt breit bei x = 26...27, 29...30, 32...33, 35...36, Lücken 1 pt bei 28, 31, 34, danach Hintergrund.
            for x in [27, 30, 33, 36] { assertColor(rep, x, 22, Theme.running, "Strich") }
            for x in [28, 31, 34, 37] { assertColor(rep, x, 22, Theme.panel, "Lücke") }
            assertColor(rep, 30, 17, Theme.running, "Strich 10 pt hoch, oben")
            assertColor(rep, 30, 26, Theme.running, "Strich 10 pt hoch, unten")
            assertColor(rep, 30, 16, Theme.panel, "über dem Strich nichts")
            assertColor(rep, 30, 27, Theme.panel, "unter dem Strich nichts")
        }
    }

    func testMeterLitDimsTheRestAndFollowsClockStep() {
        let rep = renderSession(session(status: "busy", lit: 2), theme: dark)
        withTheme(dark) {
            assertColor(rep, 27, 22, Theme.running, "Strich 1 leuchtet")
            assertColor(rep, 30, 22, Theme.running, "Strich 2 leuchtet")
            assertColor(rep, 33, 22, over(Theme.running.withAlphaComponent(0.25), Theme.panel), "Strich 3 gedimmt")
            assertColor(rep, 36, 22, over(Theme.running.withAlphaComponent(0.25), Theme.panel), "Strich 4 gedimmt")
        }
    }

    func testMeterOnlyWhenAttachedAndWorking() {
        func has(_ s: Session, attached: Bool = true) -> Bool { SidebarSessionItem.meter(for: s, attached: attached, lit: 3) != nil }
        var s = Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "A")
        XCTAssertFalse(has(s), "idle: Punkt")
        s.rawStatus = "waiting"
        XCTAssertFalse(has(s), "wartet: Punkt")
        s.rawStatus = "busy"
        XCTAssertTrue(has(s), "arbeitet: Striche")
        XCTAssertFalse(has(s, attached: false), "nicht angehängt: Punkt")
        s.rawStatus = "idle"
        s.hasRunningShell = true
        XCTAssertTrue(has(s), "Shell läuft: Striche")
        XCTAssertEqual(SidebarSessionItem.meter(for: s, attached: true, lit: 3)?.lit, 3)
        // Warten und Fehler gehen vor: die laufende Shell darf sie nicht hinter der Strich-Anzeige verstecken.
        s.rawStatus = "waiting"
        XCTAssertFalse(has(s), "wartet, Shell läuft: Punkt")
        s.rawStatus = "error"
        XCTAssertFalse(has(s), "Fehler, Shell läuft: Punkt")
        s.rawStatus = "busy"
        XCTAssertTrue(has(s), "arbeitet, Shell läuft: Striche")
    }

    func testWaitingSessionKeepsDot() {
        let rep = renderSession(session(status: "waiting"), theme: dark)
        withTheme(dark) {
            assertColor(rep, 31, 22, Theme.waiting, "Punkt in der Mitte von dotRect")
            assertColor(rep, 37, 22, Theme.panel, "rechts vom Punkt, innerhalb meterRect: Hintergrund")
        }
    }

    func testWaitingWithShellKeepsWaitingDot() {
        let rep = renderSession(session(status: "waiting", shellRunning: true), theme: dark)
        withTheme(dark) { assertColor(rep, 31, 22, Theme.waiting, "Punkt in Warte-Farbe, nicht Shell-Farbe") }
    }

    func testShellRunningUsesShellColor() {
        let rep = renderSession(session(status: "idle", shellRunning: true), theme: dark)
        withTheme(dark) {
            assertColor(rep, 27, 22, NSColor(hex: 0xa3a19a), "Strich in Shell-Farbe")
            assertColor(rep, 36, 22, NSColor(hex: 0xa3a19a), "letzter Strich in Shell-Farbe")
        }
    }

    func testStatusRectCoversDotAndMeter() {
        let r = ClassicSidebarRenderer(), row = CGRect(x: 0, y: 10, width: 200, height: 24)
        XCTAssertEqual(r.meterRect(row), CGRect(x: 26, y: r.headRect(row).midY - 5, width: 12, height: 10))
        XCTAssertTrue(r.statusRect(row).contains(r.dotRect(row)))
        XCTAssertTrue(r.statusRect(row).contains(r.meterRect(row)))
    }
}
