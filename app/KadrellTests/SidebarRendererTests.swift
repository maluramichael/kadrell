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

    private func session(focused: Bool = false, keyFocus: Bool = false, selected: Bool = false, hover: Bool = false,
                         unread: Bool = false, color: NSColor = NSColor(hex: 0x6ea8fe)) -> SidebarSessionItem {
        SidebarSessionItem(session: Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "A"), color: color, dot: Theme.idle,
                           attached: true, selected: selected, focused: focused, keyFocus: keyFocus, hover: hover, message: nil,
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
                assertColor(rep, 200, mid, Theme.bg, "\(renderer): Hover in Modulfarbe, rechts keine Zahl mehr")
            }
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
}
