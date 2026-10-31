import XCTest
@testable import Kadrell

/// Statusleiste unten: Prüfung der Einträge, Platzverteilung, Ein- und Ausblenden im Fenster, Klick und Aussehen.
/// Mit `KADRELL_RENDER_FOOTER_OUT=<pfad>.png` legt `testRenderFooter` Bilder für die Sichtprüfung ab.
@MainActor
final class FooterBarTests: XCTestCase {
    private static let index = 31

    override func tearDown() async throws {
        MainWindowController.footerHideDelay = 1.5
        for k in ["workspace.selected", "workspace.mode", "workspace.auto", "rightSidebar.visible", "sidebar.width"] {
            Profile.defaults.removeObject(forKey: k + MainWindowController.suffix(Self.index))
        }
        try await super.tearDown()
    }

    private func json(_ s: String) -> JSONValue { try! JSONDecoder().decode(JSONValue.self, from: Data(s.utf8)) }

    // MARK: Prüfung

    func testValidationKeepsGoodItemsAndExplainsTheRest() {
        let r = PanelValidation.footer(json("""
        [{"id":"cpu","align":"right","label":"CPU","bars":[0.5,2,-1,"x"],"color":"ok","action":"open","title":"Aktivität"},
         {"id":"disk","gauge":1.7,"text":"82%","align":"mitte","color":"lila"},
         {"text":"ohne id"},
         {"id":"leer"},
         {"id":"cpu","text":"doppelt"}]
        """))
        XCTAssertEqual(r.items.map(\.id), ["cpu", "disk"])
        XCTAssertEqual(r.items[0].align, .right)
        XCTAssertEqual(r.items[0].bars, [0.5, 1, 0], "auf 0...1 begrenzt, Nicht-Zahlen fallen weg")
        XCTAssertEqual(r.items[0].color, .ok)
        XCTAssertEqual(r.items[0].action, "open")
        XCTAssertEqual(r.items[0].spoken, "Aktivität")
        XCTAssertEqual(r.items[1].gauge, 1)
        XCTAssertEqual(r.items[1].align, .left, "unbekannte Ausrichtung heißt links")
        XCTAssertNil(r.items[1].color)
        XCTAssertEqual(r.items[1].spoken, "82%", "ohne title nennt VoiceOver Label und Text")
        XCTAssertEqual(r.warnings.count, 4, "\(r.warnings)")
    }

    func testValidationCapsCountsAndLengths() {
        let many = (0..<30).map { #"{"id":"e\#($0)","text":"\#(String(repeating: "x", count: 90))"}"# }.joined(separator: ",")
        let r = PanelValidation.footer(json("[\(many)]"))
        XCTAssertEqual(r.items.count, PanelValidation.maxFooterItems)
        XCTAssertEqual(r.items[0].text?.count, PanelValidation.maxStatus)
        XCTAssertEqual(r.warnings.count, 1)
        let bars = (0..<200).map { _ in "0.5" }.joined(separator: ",")
        XCTAssertEqual(PanelValidation.footer(json(#"[{"id":"b","bars":[\#(bars)]}]"#)).items[0].bars.count, PanelValidation.maxBars)
        XCTAssertTrue(PanelValidation.footer(nil).items.isEmpty)
        XCTAssertTrue(PanelValidation.footer(json("{}")).items.isEmpty, "leere Lua-Tabelle kommt auch als Objekt")
    }

    func testMessageDecodes() {
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"footer","items":[{"id":"a","text":"x"}]}"#.utf8)),
                       .footer(json(#"[{"id":"a","text":"x"}]"#)))
        XCTAssertEqual(ExtensionMessage.decode(Data(#"{"t":"footer","items":null}"#.utf8)), .footer(nil))
    }

    // MARK: Platz

    func testFitPrefersRightAndDropsFromTheMiddle() {
        XCTAssertEqual(FooterBarView.fit(left: [100, 100], right: [100], width: 500).left, 2)
        let tight = FooterBarView.fit(left: [100, 100, 100], right: [100, 100], width: 450)
        XCTAssertEqual(tight.right, 2, "rechts zählt vor links")
        XCTAssertEqual(tight.left, 2, "links fallen die hinteren weg, zwischen den Seiten bleiben 16 pt")
        let narrow = FooterBarView.fit(left: [100], right: [100, 100, 100], width: 250)
        XCTAssertEqual(narrow.right, 2, "rechts fallen die vorderen weg")
        XCTAssertEqual(narrow.left, 0)
    }

    // MARK: Fenster

    private func controller() -> MainWindowController {
        let wc = MainWindowController(index: Self.index, cascade: nil)
        wc.window.setContentSize(NSSize(width: 1200, height: 700))
        return wc
    }

    private let entries: [FooterBarView.Entry] = [
        ("system-info", FooterItem(id: "cpu", align: .right, label: "CPU", bars: [0.1, 0.6, 0.9, 0.3], action: "open")),
        ("system-info", FooterItem(id: "disk", align: .right, label: "SSD", text: "82%", gauge: 0.82)),
    ]

    func testFooterTakesNoSpaceUntilItemsArrive() async {
        MainWindowController.footerHideDelay = 0
        let wc = controller()
        defer { wc.close(); wc.window.close() }
        let root = wc.window.contentView!.bounds
        XCTAssertTrue(wc.footer.isHidden)
        XCTAssertEqual(wc.split.frame.maxY, root.height, "ohne Einträge reicht der Split bis unten")

        wc.updateFooter(entries)
        XCTAssertFalse(wc.footer.isHidden)
        XCTAssertEqual(wc.footer.frame.height, FooterBarView.height)
        XCTAssertEqual(wc.footer.frame.maxY, root.height)
        XCTAssertEqual(wc.split.frame.maxY, root.height - FooterBarView.height, "der Split rückt über die Leiste")
        XCTAssertEqual(wc.wallpaper.frame, wc.split.frame)

        wc.updateFooter([])
        let hidden = await until(2) { wc.footer.isHidden }
        XCTAssertTrue(hidden)
        XCTAssertEqual(wc.split.frame.maxY, root.height)
    }

    /// Kommt nach einem Neuladen der Extension gleich wieder etwas, bleibt die Leiste stehen: kein Größenwechsel der Terminals.
    func testShortGapKeepsFooter() async {
        MainWindowController.footerHideDelay = 0.3
        let wc = controller()
        defer { wc.close(); wc.window.close() }
        wc.updateFooter(entries)
        wc.updateFooter([])
        wc.updateFooter(entries)
        try? await Task.sleep(for: .milliseconds(500))
        XCTAssertFalse(wc.footer.isHidden)
    }

    func testFooterFollowsUIScale() {
        let saved = Theme.scale
        defer { Theme.scale = saved }
        let wc = controller()
        defer { wc.close(); wc.window.close() }
        wc.updateFooter(entries)
        Theme.scale = 1.3
        wc.applyAppearance(themeChanged: false)
        XCTAssertEqual(wc.footer.frame.height, (22 * 1.3).rounded())
        XCTAssertEqual(wc.split.frame.maxY, wc.window.contentView!.bounds.height - wc.footer.frame.height)
    }

    private func until(_ seconds: TimeInterval, _ condition: () -> Bool) async -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while !condition(), Date() < deadline { try? await Task.sleep(for: .milliseconds(20)) }
        return condition()
    }

    // MARK: Klick und VoiceOver

    private func footerWindow(_ f: FooterBarView, width: CGFloat = 900) -> NSWindow {
        let w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: width, height: 22), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        f.frame = NSRect(x: 0, y: 0, width: width, height: 22)
        w.contentView = f
        return w
    }

    func testClickSendsActionAndVoiceOverSeesEntries() throws {
        let saved = Theme.scale
        defer { Theme.scale = saved }
        Theme.scale = 1
        let f = FooterBarView(frame: .zero)
        let w = footerWindow(f)
        defer { w.close() }
        var got: [String] = []
        f.onAction = { got += [$0, $1] }
        f.entries = entries + [("other", FooterItem(id: "note", label: "Notiz", text: "hallo"))]
        f.display()
        XCTAssertEqual(f.drawn.map(\.entry.item.id), ["note", "disk", "cpu"], "links von links, rechts von rechts")
        let elements = (f.accessibilityChildren() ?? []).compactMap { $0 as? A11yElement }
        XCTAssertEqual(elements.filter { $0.accessibilityRole() == .button }.map { $0.accessibilityLabel() }, ["CPU"])
        XCTAssertEqual(Set(elements.filter { $0.accessibilityRole() == .staticText }.compactMap { $0.accessibilityLabel() }), ["Notiz hallo", "SSD 82%"])

        let cpu = try XCTUnwrap(f.drawn.first { $0.entry.item.id == "cpu" }).rect
        let p = f.convert(CGPoint(x: cpu.midX, y: cpu.midY), to: nil)
        f.mouseDown(with: NSEvent.mouseEvent(with: .leftMouseDown, location: p, modifierFlags: [], timestamp: 0, windowNumber: w.windowNumber,
                                             context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!)
        XCTAssertEqual(got, ["system-info", "open"])
        let disk = try XCTUnwrap(f.drawn.first { $0.entry.item.id == "disk" }).rect
        f.mouseDown(with: NSEvent.mouseEvent(with: .leftMouseDown, location: f.convert(CGPoint(x: disk.midX, y: disk.midY), to: nil), modifierFlags: [],
                                             timestamp: 0, windowNumber: w.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!)
        XCTAssertEqual(got.count, 2, "ohne Aktion passiert nichts")
    }

    func testUnchangedEntriesCompareEqual() {
        XCTAssertTrue(FooterBarView.same(entries, entries))
        var other = entries
        other[0].item.bars = [0.2]
        XCTAssertFalse(FooterBarView.same(entries, other))
        XCTAssertFalse(FooterBarView.same(entries, []))
    }

    // MARK: Aussehen

    /// Balken füllen von unten in der Farbe des Eintrags, über einer gedämpften Spur; die Leiste trägt oben die Fuge.
    func testBarsFillFromBottomInItemColor() throws {
        let item = FooterItem(id: "cpu", color: .ok, bars: [1, 0])
        let theme = ColorTheme.named("instrument-dark")
        let w = 200, h = 22
        let rep = render(theme: theme, width: w, height: h, scale: 1) {
            let f = FooterBarView(frame: NSRect(x: 0, y: 0, width: w, height: h))
            f.entries = [("x", item)]
            f.draw(f.bounds)
        }
        withTheme(theme) {
            XCTAssertLessThan(distance(pixel(rep, x: 11, y: 12), Theme.idle), 0.02, "voller Balken in der Farbe")
            XCTAssertGreaterThan(distance(pixel(rep, x: 15, y: 12), Theme.idle), 0.2, "leerer Balken nur als Spur")
            XCTAssertLessThan(luma(rep, x: 100, y: 0), luma(Theme.panel), "oben die dunkle Hälfte der Fuge")
        }
    }

    func testRenderFooter() throws {
        let out = ProcessInfo.processInfo.environment["KADRELL_RENDER_FOOTER_OUT"]
        let saved = (Theme.current, Theme.scale)
        defer { (Theme.current, Theme.scale) = saved }
        let sample: [FooterBarView.Entry] = [
            ("notes", FooterItem(id: "branch", label: "git", text: "main", color: .accent)),
            ("system-info", FooterItem(id: "disk", align: .right, label: "SSD", text: "82% · 120 GB", color: .warn, gauge: 0.82, action: "open")),
            ("system-info", FooterItem(id: "cpu", align: .right, label: "CPU",
                                       bars: [0.05, 0.12, 0.4, 0.95, 0.3, 0.2, 0.08, 0.6, 0.15, 0.02, 0.7, 0.33], action: "open")),
            ("system-info", FooterItem(id: "cpu-total", align: .right, label: "CPU", text: "23%")),
        ]
        for (theme, scale) in [("instrument-dark", 1.0), ("instrument-light", 1.0), ("instrument-dark", 1.3)] {
            Theme.current = ColorTheme.named(theme)
            Theme.scale = scale
            let f = FooterBarView(frame: .zero)
            let w = footerWindow(f, width: 900 * scale)
            f.frame.size.height = FooterBarView.height
            f.entries = sample
            f.display()
            let rep = try XCTUnwrap(f.bitmapImageRepForCachingDisplay(in: f.bounds))
            f.cacheDisplay(in: f.bounds, to: rep)
            w.close()
            guard let out else { continue }
            let path = out.replacingOccurrences(of: ".png", with: "-\(theme)-\(Int(scale * 100)).png")
            try XCTUnwrap(rep.representation(using: .png, properties: [:])).write(to: URL(fileURLWithPath: path))
        }
    }
}
