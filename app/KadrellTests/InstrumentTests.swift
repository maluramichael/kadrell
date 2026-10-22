import XCTest
import Metal
import QuartzCore
@testable import Kadrell

/// Bauteile des Instrument-Looks: Fuge, Taste, Display. Gezeichnet wird in ein gespiegeltes Bitmap (1 pt = 1 px,
/// y wächst nach unten wie in den echten Views), und zwar über `Theme.scaled`, damit der Maßstab wie in der App wirkt.
@MainActor
final class InstrumentTests: XCTestCase {
    private func render(width: Int, height: Int, _ draw: () -> Void) -> NSBitmapImageRep {
        render(theme: Theme.current, width: width, height: height, scale: 1, draw)
    }

    /// Zeilen, in denen mindestens ein Pixel merklich von `color` abweicht.
    private func changedRows(_ rep: NSBitmapImageRep, against color: NSColor) -> [Int] {
        let ref = luma(color)
        return (0..<rep.pixelsHigh).filter { y in (0..<rep.pixelsWide).contains { abs(luma(rep, x: $0, y: y) - ref) > 0.01 } }
    }

    /// Trenner eines Splits wie in der App: unskaliert, über dem Gehäuse des Baums.
    private func renderDivider(_ split: ThinSplitView, rect: CGRect, theme: ColorTheme) -> NSBitmapImageRep {
        render(theme: theme, width: Int(split.bounds.width), height: Int(split.bounds.height), scale: 1) {
            panelCanvas(split.bounds.width, split.bounds.height)
            split.drawDivider(in: rect)
        }
    }

    /// Eine View 1:1 in ein Bitmap (1 pt = 1 px), Zeile 0 ist oben.
    private func render(_ view: NSView) -> NSBitmapImageRep {
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(view.bounds.width), pixelsHigh: Int(view.bounds.height),
                                   bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                                   bytesPerRow: 0, bitsPerPixel: 0)!
        rep.size = view.bounds.size
        view.cacheDisplay(in: view.bounds, to: rep)
        return rep
    }

    private func alpha(_ rep: NSBitmapImageRep, x: Int, y: Int) -> CGFloat { rep.colorAt(x: x, y: y)!.alphaComponent }

    func testSeamDrawsDarkThenLight() {
        for t in ColorTheme.all {
            let rep = render(theme: t, width: 20, height: 20, scale: 1) {
                panelCanvas(20, 20)
                Instrument.hSeam(x: 0, y: 10, width: 20)
            }
            let panel = luma(rep, x: 5, y: 0)
            XCTAssertLessThan(luma(rep, x: 5, y: 10), panel, t.id)
            XCTAssertGreaterThan(luma(rep, x: 5, y: 11), panel, t.id)
            XCTAssertEqual(luma(rep, x: 5, y: 9), panel, accuracy: 0.005, t.id)
            XCTAssertEqual(luma(rep, x: 5, y: 12), panel, accuracy: 0.005, t.id)
        }
    }

    func testVerticalSeamDrawsDarkThenLight() {
        for t in ColorTheme.all {
            let rep = render(theme: t, width: 20, height: 20, scale: 1) {
                panelCanvas(20, 20)
                Instrument.vSeam(x: 10, y: 0, height: 20)
            }
            let panel = luma(rep, x: 0, y: 5)
            XCTAssertLessThan(luma(rep, x: 10, y: 5), panel, t.id)
            XCTAssertGreaterThan(luma(rep, x: 11, y: 5), panel, t.id)
        }
    }

    func testSeamStaysOnePointAtScale() {
        let t = ColorTheme.named("instrument-dark")
        for scale in [0.9, 1.0, 1.1, 1.3] as [CGFloat] {
            let rep = render(theme: t, width: 20, height: 20, scale: scale) {
                panelCanvas(20 / scale, 20 / scale)
                Instrument.hSeam(x: 0, y: 5, width: 15)
            }
            let rows = changedRows(rep, against: t.panel)
            XCTAssertEqual(rows.count, 2, "Maßstab \(scale): genau zwei Zeilen")
            XCTAssertEqual(rows.last.map { $0 - rows[0] }, 1, "Maßstab \(scale): dunkle und helle Zeile grenzen aneinander")
            if let first = rows.first {
                XCTAssertLessThan(luma(rep, x: 3, y: first), luma(t.panel), "Maßstab \(scale): oben dunkel")
                XCTAssertGreaterThan(luma(rep, x: 3, y: first + 1), luma(t.panel), "Maßstab \(scale): unten hell")
            }
        }
    }

    func testSeamVisibleInEveryTheme() {
        // Die dunkle Zeile hebt sich vom Grund ab, die helle von der dunklen; die helle Zeile ist nie dunkler als der Grund.
        for t in ColorTheme.all {
            withTheme(t) {
                let dark = over(Instrument.seamDark, t.panel), light = over(Instrument.seamLight, t.panel)
                XCTAssertGreaterThanOrEqual(dark.contrastRatio(with: t.panel), 1.05, t.id)
                XCTAssertGreaterThanOrEqual(light.contrastRatio(with: dark), 1.15, t.id)
                XCTAssertGreaterThanOrEqual(luma(light), luma(t.panel), t.id)
            }
        }
    }

    func testSeamColorsFollowDarkAndLight() {
        for t in ColorTheme.all {
            withTheme(t) {
                XCTAssertEqual(Instrument.seamDark.alphaComponent, t.dark ? 0.6 : 0.13, accuracy: 0.001, t.id)
                XCTAssertEqual(Instrument.seamLight.alphaComponent, t.dark ? 0.05 : 0.65, accuracy: 0.001, t.id)
            }
        }
    }

    func testPressedKeySinks() {
        let r = CGRect(x: 0, y: 0, width: 40, height: 20)
        _ = render(theme: .named("instrument-dark"), width: 40, height: 20, scale: 1) {
            let up = Instrument.key(r, pressed: false, fill: nil)
            let down = Instrument.key(r, pressed: true, fill: nil)
            XCTAssertEqual(down.minY, up.minY + 1)
            XCTAssertEqual(up.height, r.height - 2, "Kappe ohne die 2 pt Dicke")
            XCTAssertEqual(down.height, up.height, "gleiche Kappe, nur tiefer")
        }
    }

    func testKeyUsesFillAndEdge() {
        let t = ColorTheme.named("instrument-dark")
        let fill = NSColor(hex: 0xff0000)
        var edge = NSColor.clear
        let rep = render(theme: t, width: 40, height: 20, scale: 1) {
            panelCanvas(40, 20)
            _ = Instrument.key(CGRect(x: 0, y: 0, width: 40, height: 20), pressed: false, fill: fill)
            edge = Instrument.keyEdge
        }
        XCTAssertEqual(luma(rep, x: 20, y: 8), luma(fill), accuracy: 0.02, "Kappe trägt die Füllung")
        XCTAssertEqual(luma(rep, x: 20, y: 19), luma(edge), accuracy: 0.02, "Dicke unten in keyEdge")
    }

    func testScreenIsBlackInEveryTheme() {
        for t in ColorTheme.all {
            withTheme(t) {
                XCTAssertGreaterThanOrEqual(Instrument.screenInk.contrastRatio(with: Instrument.screenFace), 4.5, t.id)
                XCTAssertGreaterThanOrEqual(Instrument.screenMuted.contrastRatio(with: Instrument.screenFace), 4.5, t.id)
                XCTAssertEqual(Instrument.screenFace.hexString, t.dark ? "#050505" : "#0d0d0c", t.id)
                XCTAssertEqual(Instrument.screenInk.hexString, "#efe9dc", t.id)
                XCTAssertEqual(Instrument.screenMuted.hexString, "#8f8b81", t.id)
            }
        }
    }

    func testScreenDrawsFaceWithTopHighlight() {
        let t = ColorTheme.named("instrument-dark")
        let face = luma(NSColor(hex: 0x050505))
        let rep = render(theme: t, width: 40, height: 20, scale: 1) {
            panelCanvas(40, 20)
            Instrument.screen(CGRect(x: 0, y: 0, width: 40, height: 20))
        }
        XCTAssertEqual(luma(rep, x: 20, y: 10), face, accuracy: 0.01)
        XCTAssertGreaterThan(luma(rep, x: 20, y: 0), luma(rep, x: 20, y: 10), "oben liegt die helle Haarlinie")
        XCTAssertGreaterThan(luma(rep, x: 0, y: 0), face + 0.02, "Ecke ist abgerundet, dort scheint der Grund durch")
    }

    func testSplitDividerIsSeam() {
        // senkrechter Split, Trenner bei x = 50: Spalte 50 dunkler als panel, 51 heller oder gleich, Breite 2
        let split = ThinSplitView(frame: CGRect(x: 0, y: 0, width: 100, height: 40)); split.isVertical = true
        XCTAssertEqual(split.dividerThickness, 2)
        let rep = renderDivider(split, rect: CGRect(x: 50, y: 0, width: 2, height: 40), theme: .named("instrument-dark"))
        withTheme(.named("instrument-dark")) { XCTAssertLessThan(luma(rep, x: 50, y: 20), luma(Theme.panel)) }
        XCTAssertGreaterThanOrEqual(luma(rep, x: 51, y: 20), luma(rep, x: 50, y: 20) * 1.15)
    }

    func testHorizontalSplitDividerIsDarkOnTop() {
        // linkes Dock: waagerechter Trenner, oben dunkel, unten hell (der Split ist gespiegelt)
        let split = ThinSplitView(frame: CGRect(x: 0, y: 0, width: 40, height: 100)); split.isVertical = false
        let rep = renderDivider(split, rect: CGRect(x: 0, y: 50, width: 40, height: 2), theme: .named("instrument-dark"))
        withTheme(.named("instrument-dark")) { XCTAssertLessThan(luma(rep, x: 20, y: 50), luma(Theme.panel)) }
        XCTAssertGreaterThanOrEqual(luma(rep, x: 20, y: 51), luma(rep, x: 20, y: 50) * 1.15)
    }

    func testCornerIgnoresClicksAndCoversOnlyTheCorner() {
        let oldOpacity = Settings.tileOpacity
        defer { Settings.tileOpacity = oldOpacity }
        Settings.tileOpacity = 1
        let c = ModuleCornerView(frame: CGRect(x: 0, y: 0, width: 16, height: 16))
        XCTAssertNil(c.hitTest(CGPoint(x: 1, y: 1)))
        // Pixel ganz in der Ecke = Gehäusefarbe, Pixel nahe der Gegenecke = durchsichtig
        let rep = render(c)
        XCTAssertEqual(alpha(rep, x: 0, y: 0), 1, accuracy: 0.05)
        XCTAssertEqual(alpha(rep, x: 15, y: 15), 0, accuracy: 0.05)
        // Nur die eigene Fläche: Kreis und Ring reichen rechnerisch weiter, gezeichnet wird außerhalb nichts.
        let wide = render(theme: Theme.current, width: 40, height: 40, scale: 1) { c.draw(c.bounds) }
        for (x, y) in [(24, 24), (30, 16), (16, 30), (20, 5)] { XCTAssertEqual(alpha(wide, x: x, y: y), 0, accuracy: 0.01, "\(x),\(y)") }
    }

    /// Die Ecke setzt die geraden Fugen fort: Wo der Bogen in die linke Kante (Trenner) bzw. die obere Kante
    /// (Leiste) übergeht, liegen dunkle und helle Linie genau dort und genau so wie bei der geraden Fuge.
    func testCornerContinuesStraightSeams() {
        let oldOpacity = Settings.tileOpacity
        defer { Settings.tileOpacity = oldOpacity }
        Settings.tileOpacity = 1
        let side = ModuleCornerView.radius + 2
        let c = ModuleCornerView(frame: CGRect(x: 0, y: 0, width: side, height: side))
        // Vierfach gerastert, damit die Kantenglättung des Bogens die Messpunkte nicht berührt: Spalte 1 liegt mitten
        // in der dunklen, Spalte 5 mitten in der hellen Linie, die letzte Zeile direkt am Übergang in die Gerade.
        let k = 4, n = Int(side) * k, last = n - 1
        for id in ["instrument-dark", "instrument-light"] {
            let t = ColorTheme.named(id)
            let split = ThinSplitView(frame: CGRect(x: 0, y: 0, width: 100, height: 40)); split.isVertical = true
            let seam = renderDivider(split, rect: CGRect(x: 50, y: 0, width: 2, height: 40), theme: t)
            let dark = luma(seam, x: 50, y: 20), light = luma(seam, x: 51, y: 20)
            // Untergrund wie im Fenster: oben die Fuge der Leiste, links die des Trenners, die Ecke liegt darüber.
            let rep = render(theme: t, width: n, height: n, scale: 1) {
                let zoom = NSAffineTransform(); zoom.scale(by: CGFloat(k)); zoom.concat()
                panelCanvas(side, side)
                Instrument.hSeam(x: 0, y: 0, width: side)
                Instrument.vSeam(x: 0, y: 2, height: side - 2)
                c.draw(c.bounds)
            }
            XCTAssertEqual(luma(rep, x: 1, y: last), dark, accuracy: 0.01, "\(id): links außen dunkel wie der Trenner")
            XCTAssertEqual(luma(rep, x: 5, y: last), light, accuracy: 0.01, "\(id): links innen hell wie der Trenner")
            XCTAssertEqual(luma(rep, x: last, y: 1), dark, accuracy: 0.01, "\(id): oben außen dunkel wie die Leiste")
            XCTAssertEqual(luma(rep, x: last, y: 5), light, accuracy: 0.01, "\(id): oben innen hell wie die Leiste")
            withTheme(t) { XCTAssertEqual(luma(rep, x: 0, y: 0), luma(Theme.panel), accuracy: 0.01, "\(id): äußere Ecke ist Gehäuse") }
        }
    }

    /// Die Ecke hängt im Fenster über Leiste und Split und folgt der Arbeitsfläche: 2 pt nach links und oben versetzt,
    /// damit sie die Fugen von Trenner und Leiste fortsetzt, auch nach Ziehen und mit ausgeblendetem Baum.
    func testCornerFollowsWorkspaceOrigin() {
        let wc = MainWindowController(index: 23, cascade: nil)
        wc.window.setContentSize(NSSize(width: 1000, height: 700))
        wc.split.layoutSubtreeIfNeeded()
        let root = wc.window.contentView!
        XCTAssertTrue(root.subviews.last === wc.corner, "Ecke liegt zuoberst")
        let side = ModuleCornerView.scaledRadius + 2
        func expected() -> CGPoint {
            let o = wc.workspace.convert(CGPoint.zero, to: root)
            return CGPoint(x: o.x - 2, y: o.y - 2)
        }

        wc.split.setPosition(300, ofDividerAt: 0)
        XCTAssertEqual(wc.corner.frame.origin, expected())
        XCTAssertEqual(wc.corner.frame.origin, CGPoint(x: 300, y: Theme.barHeight - 2), "Trenner bei 300 bis 302, Leiste darüber")
        XCTAssertEqual(wc.corner.frame.width, side, accuracy: 0.01)
        XCTAssertEqual(wc.corner.frame.height, side, accuracy: 0.01)

        // Den Split legt erst der Zeichenzyklus des Fensters neu aus, im Test erzwungen.
        func relayout() { wc.split.needsLayout = true; wc.split.layoutSubtreeIfNeeded() }
        wc.toggleSidebar()
        relayout()
        XCTAssertEqual(wc.corner.frame.origin, CGPoint(x: -2, y: Theme.barHeight - 2), "ohne Baum ragt die Ecke links aus dem Fenster")
        wc.toggleSidebar()
        relayout()
        XCTAssertEqual(wc.corner.frame.origin, expected())
        XCTAssertGreaterThan(wc.corner.frame.minX, 100, "Baum wieder da, Ecke wieder rechts davon")
    }

    func testScaledRadiusRoundsToWholePoints() {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        XCTAssertEqual(ModuleCornerView.scaledRadius, 14)
        Theme.scale = 1.3
        XCTAssertEqual(ModuleCornerView.scaledRadius, 18)
    }

    /// Nur die Kachel genau in der Ecke der Arbeitsfläche rundet sich, konzentrisch zum Bogen der Modulecke.
    func testModuleCornerRadiusOnlyForTopLeftTile() {
        let saved = Theme.scale; defer { Theme.scale = saved }
        Theme.scale = 1
        let r = ModuleCornerView.scaledRadius
        XCTAssertEqual(WorkspaceView.moduleCornerRadius(for: CGRect(x: 0, y: 0, width: 300, height: 200), gap: 0), r)
        XCTAssertEqual(WorkspaceView.moduleCornerRadius(for: CGRect(x: 6, y: 6, width: 300, height: 200), gap: 6), r - 6)
        XCTAssertEqual(WorkspaceView.moduleCornerRadius(for: CGRect(x: 6, y: 6, width: 300, height: 200), gap: 0), 0, "nicht in der Ecke")
        XCTAssertEqual(WorkspaceView.moduleCornerRadius(for: CGRect(x: 306, y: 0, width: 300, height: 200), gap: 0), 0, "rechts daneben")
        XCTAssertEqual(WorkspaceView.moduleCornerRadius(for: CGRect(x: 30, y: 30, width: 300, height: 200), gap: 30), 0, "Abstand größer als der Bogen")
    }

    /// Im Layout bekommt nur die erste Kachel (oben links) den Radius, auch nach Änderung des Abstands.
    func testWorkspaceRoundsOnlyTopLeftCell() {
        let oldGap = Settings.tileGap, oldScale = Theme.scale
        let suffix = ".test-corner"
        let ws = WorkspaceView(frame: NSRect(x: 0, y: 0, width: 600, height: 400), defaultsSuffix: suffix)
        defer {
            Settings.tileGap = oldGap; Theme.scale = oldScale
            ws.close()
            for k in ["workspace.selected", "workspace.mode", "workspace.auto"] { Profile.defaults.removeObject(forKey: k + suffix) }
        }
        Theme.scale = 1
        Settings.tileGap = 0
        let sessions = ["a", "b"].map { Session(id: $0, cwd: "/p", startedAt: 0, sessionId: $0, name: $0) }
        let group = Group(id: "g", name: "G", color: "#89b4fa", cwd: "/p", sessionIds: ["a", "b"], favorite: false)
        ws.reload(groups: [group], sessions: sessions)
        ws.select(["a", "b"], add: false, takeKeyboard: false)
        func radii() -> [CGFloat] {
            ws.subviews.compactMap { $0 as? CellView }.filter { !$0.isHidden }.sorted { ($0.frame.minX, $0.frame.minY) < ($1.frame.minX, $1.frame.minY) }
                .map(\.moduleCornerRadius)
        }
        XCTAssertEqual(radii(), [ModuleCornerView.scaledRadius, 0])
        Settings.tileGap = 6
        ws.relayout()
        XCTAssertEqual(radii(), [ModuleCornerView.scaledRadius - 6, 0])
    }

    /// Kachel `cell` im Fenster anzeigen und die Layer-Ebenen vierfach gerastert abholen: Rahmen, Ecke und Inhalt wie
    /// auf dem Bildschirm, Zeile 0 oben. Außerhalb der Kachel durchsichtig.
    private func renderCellLayer(_ cell: CellView, size: CGSize, k: Int = 4) -> NSBitmapImageRep {
        let win = NSWindow(contentRect: CGRect(origin: .zero, size: size), styleMask: .borderless, backing: .buffered, defer: false)
        defer { win.close() }
        win.isReleasedWhenClosed = false
        let root = FlippedView(frame: CGRect(origin: .zero, size: size)); root.wantsLayer = true
        win.contentView = root
        cell.frame = CGRect(origin: .zero, size: size)
        root.addSubview(cell)
        root.layoutSubtreeIfNeeded()
        win.displayIfNeeded()
        cell.layer!.displayIfNeeded()
        let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width) * k, pixelsHigh: Int(size.height) * k, bitsPerSample: 8,
                                   samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
        let cg = NSGraphicsContext(bitmapImageRep: rep)!.cgContext
        cg.scaleBy(x: CGFloat(k), y: CGFloat(k))
        // `render(in:)` übergeht die gespiegelte Geometrie der Arbeitsfläche (`isGeometryFlipped`), Inhalt, Rahmen und
        // Ecken kämen gemeinsam auf dem Kopf an. Zurückgespiegelt steht alles wie auf dem Bildschirm (Kopfzeile oben,
        // vom Test geprüft).
        cg.translateBy(x: 0, y: size.height); cg.scaleBy(x: 1, y: -1)
        root.layer!.render(in: cg)
        return rep
    }

    /// Fokussierte Kachel oben links bei Abstand 0: Der Rahmen (2 pt in `Theme.fg`) folgt dem Bogen der Modulecke, außerhalb
    /// des Bogens zeichnet die Kachel nichts, sonst schnitte die Ecke den Rahmen ab und bei Deckkraft unter 100 % schiene
    /// die Kachel durchs Gehäuse. Die anderen drei Ecken bleiben eckig.
    func testTopLeftTileFrameFollowsModuleCorner() {
        let oldScale = Theme.scale, oldTheme = Theme.current
        defer { Theme.scale = oldScale; Theme.current = oldTheme }
        Theme.current = .named("instrument-dark")
        let accent = NSColor(hex: 0x6ea8fe), size = CGSize(width: 160, height: 100), k = 4
        for scale: CGFloat in [1, 1.3] {
            Theme.scale = scale
            let r = ModuleCornerView.scaledRadius
            let cell = CellView(session: Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "A"))
            cell.state.focused = true
            cell.state.groupColor = accent
            cell.moduleCornerRadius = WorkspaceView.moduleCornerRadius(for: CGRect(origin: .zero, size: size), gap: 0)
            let rep = renderCellLayer(cell, size: size, k: k)
            func px(_ x: CGFloat, _ y: CGFloat) -> (Int, Int) { (Int(x * CGFloat(k)), Int(y * CGFloat(k))) }
            func at(_ x: CGFloat, _ y: CGFloat) -> NSColor { let p = px(x, y); return pixel(rep, x: p.0, y: p.1) }
            func alphaAt(_ x: CGFloat, _ y: CGFloat) -> CGFloat { let p = px(x, y); return alpha(rep, x: p.0, y: p.1) }
            // Ausrichtung: oben liegt die Kopfzeile (neutral in surface), nicht der Körper.
            XCTAssertLessThan(distance(at(80, 8), Theme.surface), 0.03, "\(scale): Kopfzeile oben, \(at(80, 8).hexString)")
            // Mitte der Rundung bei (r, r); unter 45° nach oben links mitten im 2-pt-Rahmen (Abstand r - 1): Rahmenfarbe.
            let mid = r - (r - 1) / 2.squareRoot()
            XCTAssertLessThan(distance(at(mid, mid), Theme.fg), 0.05, "\(scale): Rahmen auf dem Bogen, \(at(mid, mid).hexString)")
            XCTAssertEqual(alphaAt(mid, mid), 1, accuracy: 0.02, "\(scale): Rahmen deckend")
            // Außerhalb des Bogens (Abstand r + 1) und in der Ecke selbst: nichts.
            let out = r - (r + 1) / 2.squareRoot()
            for (x, y) in [(out, out), (0.1, 0.1), (1, 1), (0.1, r / 2), (r / 2, 0.1)] {
                XCTAssertEqual(alphaAt(x, y), 0, accuracy: 0.02, "\(scale): außerhalb des Bogens bei \(x),\(y)")
            }
            // Die geraden Kanten tragen den Rahmen bis an den Bogen.
            XCTAssertLessThan(distance(at(r + 2, 0.5), Theme.fg), 0.05, "\(scale): oben gerade")
            XCTAssertLessThan(distance(at(0.5, r + 2), Theme.fg), 0.05, "\(scale): links gerade")
            // Nur oben links rund. `render(in:)` übergeht `maskedCorners` und rundet hier alle vier Ecken, auf dem
            // Bildschirm gilt die Maske (Lage der Ecke prüft `testRoundedCornerIsVisuallyTopLeft`).
            XCTAssertEqual(cell.layer?.maskedCorners, [.layerMinXMinYCorner])
            XCTAssertEqual(cell.layer?.cornerRadius, r)
        }
    }

    /// `layerMinXMinYCorner` ist in der echten Arbeitsfläche oben links: der Fensterinhalt spiegelt seine Unterebenen
    /// (`isGeometryFlipped`), lokal minY der Kachelebene liegt also im Fenster oben. Geprüft über die Umrechnung bis zur
    /// obersten Ebene des Fensters, die wie das Fenster nach oben zählt.
    func testRoundedCornerIsVisuallyTopLeft() {
        let wc = MainWindowController(index: 25, cascade: nil)
        defer { wc.window.close() }
        wc.window.setContentSize(NSSize(width: 1000, height: 700))
        wc.split.layoutSubtreeIfNeeded()
        let cell = CellView(session: Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "A"))
        cell.frame = CGRect(x: 0, y: 0, width: 200, height: 120)
        wc.workspace.addSubview(cell)
        defer { cell.removeFromSuperview() }
        wc.window.displayIfNeeded()
        guard let layer = cell.layer, let top = wc.window.contentView?.superview?.layer else { return XCTFail("ohne Ebenen") }
        XCTAssertTrue(layer.superlayer === wc.workspace.layer, "Kachelebene hängt direkt an der Arbeitsfläche")
        // AppKit spiegelt nur dort, wo sich die Richtung ändert (hier der Fensterinhalt). Ungerade viele gespiegelte
        // Vorfahren: lokal minY ist oben, genau wie unter der einen gespiegelten Ebene in `testMaskedCornerRendersTopLeftOnly`.
        let flips = sequence(first: layer, next: { $0.superlayer }).dropFirst().filter(\.isGeometryFlipped).count
        XCTAssertEqual(flips % 2, 1, "ungerade viele gespiegelte Vorfahren")
        let minY = layer.convert(CGPoint(x: 0, y: 0), to: top), maxY = layer.convert(CGPoint(x: 0, y: layer.bounds.height), to: top)
        XCTAssertGreaterThan(minY.y, maxY.y, "lokal minY liegt im Fenster oben")
        // Gegenprobe über AppKit: die obere linke Ecke der Kachel im Fenster (Fenster zählt nach oben).
        let viewTopLeft = cell.convert(CGPoint.zero, to: nil)
        XCTAssertEqual(minY.y - maxY.y, 120, accuracy: 0.01)
        XCTAssertEqual(viewTopLeft.y - cell.convert(CGPoint(x: 0, y: 120), to: nil).y, 120, accuracy: 0.01, "Kachel ist gespiegelt, y = 0 oben")
    }

    /// Die Ecken-Maske der Kachel wie auf dem Bildschirm: `render(in:)` übergeht `maskedCorners`, deshalb hier über
    /// `CARenderer` (Metal), der wie der Fensterserver zeichnet. Rand, Radius und Maske kommen von der echten Kachel,
    /// die Ebene hängt wie in der App unter einer gespiegelten Ebene; eine Marke bei y = 0 zeigt, wo oben ist.
    func testMaskedCornerRendersTopLeftOnly() throws {
        let device = try XCTUnwrap(MTLCreateSystemDefaultDevice())
        let w = 160, h = 100
        let desc = MTLTextureDescriptor.texture2DDescriptor(pixelFormat: .bgra8Unorm, width: w, height: h, mipmapped: false)
        desc.usage = [.renderTarget, .shaderRead]
        desc.storageMode = .managed
        let texture = try XCTUnwrap(device.makeTexture(descriptor: desc))
        let cell = CellView(session: Session(id: "a", cwd: "/p", startedAt: 0, sessionId: "a", name: "A"))
        cell.moduleCornerRadius = 30
        let real = try XCTUnwrap(cell.layer)
        // Ohne implizite Animationen: sonst stünden Rand und Flächen beim Zeichnen noch auf ihren Startwerten.
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let root = CALayer(); root.frame = CGRect(x: 0, y: 0, width: w, height: h); root.isGeometryFlipped = true
        let tile = CALayer(); tile.frame = root.bounds
        tile.masksToBounds = real.masksToBounds; tile.cornerRadius = real.cornerRadius; tile.maskedCorners = real.maskedCorners
        tile.borderWidth = 2; tile.borderColor = NSColor.green.cgColor; tile.backgroundColor = NSColor.blue.cgColor
        let mark = CALayer(); mark.frame = CGRect(x: 60, y: 0, width: 40, height: 6); mark.backgroundColor = NSColor.red.cgColor
        root.addSublayer(tile); root.addSublayer(mark)
        CATransaction.commit()
        // Renderer und Abholen auf derselben Warteschlange: sonst liest die Kopie das Bild, bevor der Renderer fertig ist.
        let queue = try XCTUnwrap(device.makeCommandQueue())
        let renderer = CARenderer(mtlTexture: texture, options: [kCARendererMetalCommandQueue: queue])
        renderer.layer = root
        renderer.bounds = root.bounds
        let zeros = [UInt8](repeating: 0, count: w * h * 4)
        var bytes = zeros
        // BGRA; die Zeilenrichtung der Textur legt die Marke fest.
        func bgra(_ x: Int, _ row: Int) -> [UInt8] { let i = (row * w + x) * 4; return Array(bytes[i..<i + 4]) }
        let red: [UInt8] = [0, 0, 255, 255]
        // Der erste Durchgang kann leer bleiben, solange der Renderer die Ebenen noch nicht übernommen hat. Eine neue
        // Textur ist nicht genullt: „irgendwas ungleich 0“ hieße dann Speichermüll statt fertigem Bild. Deshalb vor
        // jedem Durchgang leeren und warten, bis die Marke sichtbar ist.
        for _ in 0..<10 where bgra(80, 2) != red && bgra(80, h - 3) != red {
            texture.replace(region: MTLRegionMake2D(0, 0, w, h), mipmapLevel: 0, withBytes: zeros, bytesPerRow: w * 4)
            CATransaction.flush()
            renderer.beginFrame(atTime: CACurrentMediaTime(), timeStamp: nil)
            renderer.addUpdate(renderer.bounds)
            renderer.render()
            renderer.endFrame()
            let buffer = try XCTUnwrap(queue.makeCommandBuffer()), blit = try XCTUnwrap(buffer.makeBlitCommandEncoder())
            blit.synchronize(resource: texture); blit.endEncoding(); buffer.commit(); buffer.waitUntilCompleted()
            texture.getBytes(&bytes, bytesPerRow: w * 4, from: MTLRegionMake2D(0, 0, w, h), mipmapLevel: 0)
        }
        let topRow = bgra(80, 2) == red ? 0 : h - 1
        XCTAssertEqual(bgra(80, topRow == 0 ? 2 : h - 3), red, "Marke oben")
        let bottomRow = h - 1 - topRow
        XCTAssertEqual(bgra(1, topRow)[3], 0, "oben links rund: außerhalb des Bogens nichts")
        for (x, row, name) in [(w - 1, topRow, "oben rechts"), (0, bottomRow, "unten links"), (w - 1, bottomRow, "unten rechts")] {
            XCTAssertEqual(bgra(x, row), [0, 255, 0, 255], "\(name) eckig, Rand bis in die Ecke")
        }
    }

    /// Ändert sich eine Einstellung ohne neues Schema (etwa die Kachel-Deckkraft), zeichnen beide Trenner neu.
    /// Ein unsichtbares Fenster nimmt keine Neuzeichnen-Wünsche an, deshalb steht es (durchsichtig) vorne. `needsDisplay`
    /// der View meldet bei Ebenen nichts, geprüft wird an der Ebene, vorher dort geleert.
    func testApplyAppearanceRedrawsSeamDividers() {
        let wc = MainWindowController(index: 24, cascade: nil)
        defer { wc.window.close() }
        wc.window.setContentSize(NSSize(width: 1000, height: 700))
        wc.window.alphaValue = 0
        wc.window.orderFront(nil)
        let views: [(String, NSView)] = [("Trenner Baum/Arbeitsfläche", wc.split), ("Trenner Baum/Dock", wc.leftColumn), ("Ecke", wc.corner)]
        for themeChanged in [false, true] {
            wc.applyAppearance(themeChanged: themeChanged)
            wc.window.displayIfNeeded()
            for (_, v) in views { v.layer?.displayIfNeeded() }
            for (name, v) in views { XCTAssertEqual(v.layer?.needsDisplay(), false, "\(name): nach dem Zeichnen sauber") }
            wc.applyAppearance(themeChanged: themeChanged)
            for (name, v) in views { XCTAssertEqual(v.layer?.needsDisplay(), true, "Theme neu: \(themeChanged), \(name)") }
        }
    }

    func testMeterLightsInSteps() {
        let saved = Feedback.reduceMotion; defer { Feedback.reduceMotion = saved }
        Feedback.reduceMotion = false
        XCTAssertEqual(Feedback.meterLit(now: 0.00), 1)
        XCTAssertEqual(Feedback.meterLit(now: 0.17), 2)
        XCTAssertEqual(Feedback.meterLit(now: 0.49), 4)
        XCTAssertEqual(Feedback.meterLit(now: 0.65), 1)
    }

    func testMeterStillWithReduceMotion() {
        let saved = Feedback.reduceMotion; defer { Feedback.reduceMotion = saved }
        Feedback.reduceMotion = true
        XCTAssertNil(Feedback.meterLit(now: 0.17))
    }

    func testMeterDrawsLitAndDimBars() {
        // 12×10, lit = 2: Mitte von Strich 1 und 2 deckend, von Strich 3 und 4 ca. 25 %
        let rep = render(width: 12, height: 10) { Instrument.meter(CGRect(x: 0, y: 0, width: 12, height: 10), lit: 2, color: .white) }
        XCTAssertEqual(alpha(rep, x: 1, y: 5), 1, accuracy: 0.05)
        XCTAssertEqual(alpha(rep, x: 10, y: 5), 0.25, accuracy: 0.08)
    }

    func testMeterWithoutLitDrawsAllBarsSolid() {
        let rep = render(width: 12, height: 10) { Instrument.meter(CGRect(x: 0, y: 0, width: 12, height: 10), lit: nil, color: .white) }
        for x in [1, 4, 7, 10] { XCTAssertEqual(alpha(rep, x: x, y: 5), 1, accuracy: 0.05, "x \(x)") }
        XCTAssertEqual(alpha(rep, x: 2, y: 5), 0, accuracy: 0.05, "Lücke zwischen Strich 1 und 2")
    }

    /// Längen der Folgen deckender Pixel (Striche) und der Lücken dazwischen in einer Bitmap-Zeile.
    private func runs(_ rep: NSBitmapImageRep, y: Int) -> (bars: [Int], gaps: [Int]) {
        var bars: [Int] = [], gaps: [Int] = [], len = 0, solid = false
        for x in 0..<rep.pixelsWide {
            let on = alpha(rep, x: x, y: y) > 0.5
            if on == solid { len += 1; continue }
            if len > 0 { if solid { bars.append(len) } else if !bars.isEmpty { gaps.append(len) } }
            solid = on; len = 1
        }
        if solid && len > 0 { bars.append(len) }
        return (bars, gaps)
    }

    func testMeterGapsEvenAtEveryScale() {
        for s: CGFloat in [0.9, 1, 1.1, 1.2, 1.3] {
            let w = Int((14 * s).rounded(.up)), h = Int((12 * s).rounded(.up))
            let rep = render(theme: Theme.current, width: w, height: h, scale: s) {
                Instrument.meter(CGRect(x: 1, y: 1, width: 12, height: 10), lit: nil, color: .white)
            }
            // Zeile in der Mitte abtasten: Folgen deckender Pixel (Striche) und Lücken dazwischen
            let (bars, gaps) = runs(rep, y: Int(6 * s))
            XCTAssertEqual(bars.count, 4, "\(s)")
            XCTAssertEqual(Set(bars).count, 1, "\(s): Striche gleich breit \(bars)")
            XCTAssertEqual(Set(gaps).count, 1, "\(s): Lücken gleich breit \(gaps)")
            XCTAssertGreaterThanOrEqual(gaps.first ?? 0, 1)
        }
    }

    func testRimDarkOutsideLightInsideWithoutBottom() {
        let rep = render(theme: .named("instrument-dark"), width: 40, height: 20, scale: 1) {
            panelCanvas(40, 20)
            Instrument.rim(CGRect(x: 0, y: 0, width: 40, height: 20), top: true)
        }
        let panel = luma(rep, x: 20, y: 10)
        XCTAssertLessThan(luma(rep, x: 0, y: 10), panel, "links außen dunkel")
        XCTAssertGreaterThan(luma(rep, x: 1, y: 10), luma(rep, x: 0, y: 10), "links innen heller")
        XCTAssertLessThan(luma(rep, x: 39, y: 10), panel, "rechts außen dunkel")
        XCTAssertGreaterThan(luma(rep, x: 38, y: 10), luma(rep, x: 39, y: 10), "rechts innen heller")
        XCTAssertLessThan(luma(rep, x: 20, y: 0), panel, "oben außen dunkel")
        XCTAssertGreaterThan(luma(rep, x: 20, y: 1), luma(rep, x: 20, y: 0), "oben innen heller")
        XCTAssertEqual(luma(rep, x: 20, y: 19), panel, accuracy: 0.005, "keine Unterkante")
        XCTAssertEqual(luma(rep, x: 20, y: 2), panel, accuracy: 0.005, "Fläche bleibt unberührt")
    }

    func testRimWithoutTopLeavesTopOpenAndRoundsCorner() {
        let flat = render(theme: .named("instrument-dark"), width: 40, height: 20, scale: 1) {
            panelCanvas(40, 20)
            Instrument.rim(CGRect(x: 0, y: 0, width: 40, height: 20), top: false)
        }
        let panel = luma(flat, x: 20, y: 10)
        XCTAssertEqual(luma(flat, x: 20, y: 0), panel, accuracy: 0.005, "ohne top kein Rand oben")
        XCTAssertLessThan(luma(flat, x: 0, y: 0), panel, "Seitenrand läuft bis oben")
        let round = render(theme: .named("instrument-dark"), width: 40, height: 20, scale: 1) {
            panelCanvas(40, 20)
            Instrument.rim(CGRect(x: 0, y: 0, width: 40, height: 20), top: true, radius: 10)
        }
        XCTAssertEqual(luma(round, x: 0, y: 0), panel, accuracy: 0.005, "Ecke oben links ist rund, Pixel (0, 0) bleibt frei")
        XCTAssertLessThan(luma(round, x: 20, y: 0), panel, "oben weiter rechts liegt der Rand")
    }

    func testTopLeftRoundedRoundsOnlyThatCorner() {
        let rep = render(theme: .named("instrument-dark"), width: 20, height: 20, scale: 1) {
            Instrument.topLeftRounded(CGRect(x: 0, y: 0, width: 20, height: 20), radius: 8).fill()
        }
        XCTAssertLessThan(alpha(rep, x: 0, y: 0), 0.5, "Ecke oben links rund")
        for (x, y) in [(19, 0), (0, 19), (19, 19)] { XCTAssertGreaterThan(alpha(rep, x: x, y: y), 0.9, "(\(x), \(y)) bleibt eckig") }
        let square = render(theme: .named("instrument-dark"), width: 20, height: 20, scale: 1) {
            Instrument.topLeftRounded(CGRect(x: 0, y: 0, width: 20, height: 20), radius: 0).fill()
        }
        XCTAssertGreaterThan(alpha(square, x: 0, y: 0), 0.9, "Radius 0 = Rechteck")
    }

    func testGroupLabelAndAccent() {
        let l = Instrument.groupLabel("kadrell")
        XCTAssertEqual(l.string, "KADRELL")
        XCTAssertEqual(l.attribute(.kern, at: 0, effectiveRange: nil) as? CGFloat, 0.8)
        let rep = render(theme: Theme.current, width: 20, height: 20, scale: 1) {
            Instrument.groupAccent(center: CGPoint(x: 10, y: 10), color: .red)
        }
        XCTAssertEqual(pixel(rep, x: 10, y: 10).redComponent, 1, accuracy: 0.02)
        XCTAssertGreaterThan(alpha(rep, x: 10, y: 10), 0.9)
        XCTAssertLessThan(alpha(rep, x: 6, y: 10), 0.1, "6 pt breit: x = 6 liegt außerhalb")
        XCTAssertGreaterThan(alpha(rep, x: 7, y: 10), 0.9)
        XCTAssertLessThan(alpha(rep, x: 10, y: 6), 0.1)
    }

    func testDividerLeavesCornerAreaToModuleCorner() {
        let split = ThinSplitView(frame: CGRect(x: 0, y: 0, width: 100, height: 40)); split.isVertical = true
        split.cornerRect = CGRect(x: 50, y: -2, width: 16, height: 16)
        let rep = render(theme: .named("instrument-dark"), width: 100, height: 40, scale: 1) {
            split.drawDivider(in: CGRect(x: 50, y: 0, width: 2, height: 40))
        }
        XCTAssertEqual(alpha(rep, x: 50, y: 5), 0, accuracy: 0.01, "unter der Ecke malt der Trenner nichts")
        XCTAssertGreaterThan(alpha(rep, x: 50, y: 20), 0.5, "darunter Grund und Fuge")
    }

    func testCornerLeftStripTranslucentLikeDivider() {
        let old = Settings.tileOpacity; defer { Settings.tileOpacity = old }
        Settings.tileOpacity = 0.5
        let rep = render(ModuleCornerView(frame: CGRect(x: 0, y: 0, width: 16, height: 16)))
        XCTAssertEqual(alpha(rep, x: 0, y: 8), 0.5, accuracy: 0.05, "links wie der Trenner")
        XCTAssertEqual(alpha(rep, x: 8, y: 0), 1, accuracy: 0.05, "oben wie die deckende Leiste")
    }

    func testClosedWindowNoLongerMovesCorner() {
        let wc = MainWindowController(index: 26, cascade: nil)
        defer { wc.window.close() }
        wc.window.setContentSize(NSSize(width: 1000, height: 700)); wc.split.layoutSubtreeIfNeeded()
        wc.split.setPosition(300, ofDividerAt: 0)
        XCTAssertEqual(wc.corner.frame.minX, 300, "Beobachter aktiv")
        wc.close()
        wc.split.setPosition(400, ofDividerAt: 0)
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        XCTAssertEqual(wc.corner.frame.minX, 300, "nach close() zieht niemand mehr die Ecke nach")
    }

    // MARK: Dialog-Bauteile (Stufe 4)

    func testRaisedPanelShadowBodyRim() {
        let t = ColorTheme.named("instrument-dark")
        let rep = render(theme: t, width: 60, height: 40, scale: 1) { Instrument.raisedPanel(CGRect(x: 0, y: 0, width: 60, height: 37)) }
        XCTAssertLessThan(distance(pixel(rep, x: 30, y: 20), t.raised), 0.02, "Fläche")
        XCTAssertLessThan(luma(rep, x: 30, y: 0), luma(t.raised), "dunkle Fugenlinie außen")
        XCTAssertGreaterThan(luma(rep, x: 30, y: 1), luma(rep, x: 30, y: 0), "helle Linie innen")
        XCTAssertGreaterThan(rep.colorAt(x: 30, y: 38)!.alphaComponent, 0.3, "Schatten unter der Fläche")
        XCTAssertLessThan(rep.colorAt(x: 0, y: 0)!.alphaComponent, 0.05, "Ecke rund")
    }

    func testRaisedRimStaysHairlineAtScale() {
        // 1,3: Bitmap 78 × 52, Körper logisch 60 × 37. Zeile 0 dunkel, Zeile 1 hell, Zeile 2 schon Fläche.
        let t = ColorTheme.named("instrument-dark")
        let rep = render(theme: t, width: 78, height: 52, scale: 1.3) { Instrument.raisedPanel(CGRect(x: 0, y: 0, width: 60, height: 37)) }
        XCTAssertLessThan(luma(rep, x: 39, y: 0), luma(rep, x: 39, y: 1))
        XCTAssertLessThan(distance(pixel(rep, x: 39, y: 2), t.raised), 0.02)
    }

    func testRimVisibleOnRaisedInEveryTheme() {
        for t in ColorTheme.all {
            let rep = render(theme: t, width: 40, height: 40, scale: 1) { Instrument.raisedPanel(CGRect(x: 0, y: 0, width: 40, height: 37)) }
            XCTAssertGreaterThan(abs(luma(rep, x: 20, y: 0) - luma(t.raised)), 0.02, t.id)
        }
    }

    func testFocusedWellShowsAccentRing() {
        let t = ColorTheme.named("instrument-light")
        let rep = render(theme: t, width: 60, height: 20, scale: 1) { Instrument.well(CGRect(x: 0, y: 0, width: 60, height: 20), focused: true) }
        XCTAssertLessThan(distance(pixel(rep, x: 30, y: 1), t.accent), 0.05)
        XCTAssertLessThan(distance(pixel(rep, x: 30, y: 10), t.well), 0.02)
    }

    func testRowHighlightMatchesTree() {
        let t = ColorTheme.named("instrument-dark")
        let rep = render(theme: t, width: 200, height: 24, scale: 1) {
            panelCanvas(200, 24); Instrument.rowHighlight(CGRect(x: 0, y: 0, width: 200, height: 24), outline: true, ring: true)
        }
        XCTAssertLessThan(distance(pixel(rep, x: 4, y: 12), t.accent), 0.1, "Ring außen")
        XCTAssertLessThan(distance(pixel(rep, x: 100, y: 12), t.bg), 0.02, "Fläche innen")
    }

    func testKeyRoleLabelsReadable() {
        for t in ColorTheme.all { withTheme(t) {
            XCTAssertEqual(KeyRole.primary.fill, Theme.accent); XCTAssertNil(KeyRole.neutral.fill); XCTAssertEqual(KeyRole.destructive.fill, Theme.error)
            for role in KeyRole.allCases {
                XCTAssertGreaterThanOrEqual(role.label.contrastRatio(with: role.fill ?? Instrument.keyCap), 4.5, "\(t.id) \(role)")
            }
        } }
    }

    func testSurfaceViewIgnoresClicks() {
        XCTAssertNil(InstrumentSurfaceView(part: .raised, frame: NSRect(x: 0, y: 0, width: 50, height: 50)).hitTest(NSPoint(x: 10, y: 10)))
    }
}
