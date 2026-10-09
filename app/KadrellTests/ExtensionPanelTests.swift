import XCTest
import AppKit
@testable import Kadrell

/// Rechte Sidebar: Tabs je Extension, Tastatur, Ein- und Ausblenden pro Fenster, Fokus beim Verschwinden.
/// Mit `KADRELL_RENDER_OUT=<pfad>.png` legt `testRenderSmoke` zusätzlich ein Bild für die Sichtprüfung ab.
@MainActor
final class ExtensionPanelTests: XCTestCase {
    private static let index = 11
    /// Wie `MainWindowController.suffix(index)`, hier ohne MainActor, damit `tearDown` es lesen kann.
    private let suffix = ".\(ExtensionPanelTests.index + 1)"
    /// Die Leisten-Geometrie ist für 100% ausgelegt; feste Skalierung, damit die UI-Größe des laufenden Profils die Tests nicht kippt.
    private var savedScale: CGFloat = 1

    override func setUp() {
        super.setUp()
        savedScale = Theme.scale
        Theme.scale = 1
    }

    override func tearDown() {
        Theme.scale = savedScale
        for k in ["sidebar.width", "rightSidebar.width", "rightSidebar.visible", "workspace.selected", "workspace.mode", "workspace.auto",
                  "NSSplitView Subview Frames KadrellSplit"] {
            Profile.defaults.removeObject(forKey: k + suffix)
            UserDefaults.standard.removeObject(forKey: k + suffix)
        }
        super.tearDown()
    }

    private func key(_ code: UInt16, _ mods: NSEvent.ModifierFlags = []) -> NSEvent {
        NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: 0, windowNumber: 0, context: nil,
                         characters: "", charactersIgnoringModifiers: "", isARepeat: false, keyCode: code)!
    }

    private func item(_ n: Int, color: ThemeColor? = nil, detail: String? = nil) -> PanelNode {
        .item(text: "PROJ-\(n) Eintrag \(n)", detail: detail, color: color,
              actions: [PanelAction(id: "open:\(n)", label: "Öffnen"), PanelAction(id: "claude:\(n)", label: "Session starten")])
    }

    private var twoTabs: [(name: String, tree: PanelTree)] {
        [("a", PanelTree(title: "Jira", nodes: [item(1), item(2), item(3)])),
         ("b", PanelTree(title: "ddev", nodes: [.text("läuft", .ok)]))]
    }

    private func controller() -> MainWindowController {
        let wc = MainWindowController(index: Self.index, cascade: nil)
        wc.window.setContentSize(NSSize(width: 1200, height: 700))
        wc.split.layoutSubtreeIfNeeded()
        return wc
    }

    func testHotkeyDefaultsStayUnique() {
        XCTAssertEqual(HotkeyAction.focusRightSidebar.defaultKey, Hotkey(.command, "3"))
        XCTAssertEqual(HotkeyAction.toggleRightSidebar.defaultKey, Hotkey([.command, .option], "b"))
        let keys = HotkeyAction.allCases.map(\.defaultKey)
        XCTAssertEqual(Set(keys).count, keys.count)
        // Eigene Titel, nicht der Sammelzweig von „Fokus-Kachel schließen“.
        XCTAssertNotEqual(HotkeyAction.focusRightSidebar.title, HotkeyAction.closeFocused.title)
        XCTAssertNotEqual(HotkeyAction.toggleRightSidebar.title, HotkeyAction.closeFocused.title)
    }

    func testPanelKeyboardNavigation() {
        let v = ExtensionPanelView(frame: NSRect(x: 0, y: 0, width: 300, height: 400))
        var got: [String] = []
        var left = false
        v.onAction = { name, id in got.append(name + "|" + id) }
        v.onLeave = { left = true }
        v.panels = twoTabs
        XCTAssertEqual(v.selectedTab, "a", "ohne Auswahl der erste Tab")

        v.keyDown(with: key(KeyCode.down))
        v.keyDown(with: key(KeyCode.down))
        v.keyDown(with: key(KeyCode.returnKey))
        XCTAssertEqual(got, ["a|open:2"], "⏎ löst die erste Aktion des gewählten Eintrags aus")

        v.keyDown(with: key(KeyCode.right))
        XCTAssertEqual(v.selectedTab, "b")
        v.keyDown(with: key(KeyCode.right))
        XCTAssertEqual(v.selectedTab, "b", "am letzten Tab bleibt es stehen")
        v.keyDown(with: key(KeyCode.left))
        XCTAssertEqual(v.selectedTab, "a")

        v.keyDown(with: key(KeyCode.escape))
        XCTAssertTrue(left, "Esc gibt die Tastatur ab")
    }

    func testAreaHiddenWithoutPanels() {
        let wc = controller()
        XCTAssertNil(wc.rightScroll.superview, "ohne Panels kein Bereich, kein leerer Rand")

        let treeWidth = wc.sidebarScroll.frame.width
        wc.updateExtensionPanels(twoTabs)
        XCTAssertTrue(wc.split.arrangedSubviews.last === wc.rightScroll, "dritter Bereich ganz rechts")
        wc.split.layoutSubtreeIfNeeded()
        XCTAssertEqual(wc.rightScroll.frame.width, 260, accuracy: 1, "Startbreite")
        XCTAssertEqual(wc.sidebarScroll.frame.width, treeWidth, accuracy: 1, "der Baum behält seine Breite")

        // Breite pro Fenster gespeichert: nach dem Ziehen kommt der Bereich so breit zurück.
        wc.split.setPosition(wc.split.bounds.width - 301, ofDividerAt: 1)
        wc.toggleRightSidebar()
        wc.toggleRightSidebar()
        wc.split.layoutSubtreeIfNeeded()
        XCTAssertEqual(wc.rightScroll.frame.width, 300, accuracy: 1)
        wc.toggleSidebar()
        wc.split.layoutSubtreeIfNeeded()
        XCTAssertEqual(wc.rightScroll.frame.width, 300, accuracy: 2, "Baum aus: die rechte Sidebar bleibt gleich breit")
        wc.toggleSidebar()
        wc.split.layoutSubtreeIfNeeded()
        XCTAssertEqual(wc.rightScroll.frame.width, 300, accuracy: 1, "Baum ein/aus lässt die rechte Sidebar stehen")
        XCTAssertEqual(wc.sidebarScroll.frame.width, treeWidth, accuracy: 1)

        wc.toggleRightSidebar()
        XCTAssertTrue(wc.isRightSidebarHidden)
        XCTAssertNil(wc.rightScroll.superview)
        XCTAssertTrue(MainWindowController(index: Self.index, cascade: nil).isRightSidebarHidden, "Sichtbarkeit pro Fenster gespeichert")
        wc.toggleRightSidebar()
        XCTAssertTrue(wc.split.arrangedSubviews.last === wc.rightScroll)

        wc.updateExtensionPanels([])
        XCTAssertFalse(wc.split.arrangedSubviews.contains(wc.rightScroll))
        XCTAssertFalse(wc.isRightSidebarHidden, "ausgeblendet mangels Panels, nicht vom Nutzer")
    }

    /// Mit rechter Sidebar sichert der Split drei Bereiche, beim Start hat er zwei und verwirft sein Autosave.
    /// Die Baumbreite muss trotzdem zurückkommen.
    func testTreeWidthSurvivesRestartWithRightSidebar() {
        let wc = controller()
        wc.restoreSidebarWidth()
        wc.split.setPosition(300, ofDividerAt: 0)
        wc.updateExtensionPanels(twoTabs)
        wc.split.layoutSubtreeIfNeeded()
        XCTAssertEqual(wc.sidebarScroll.frame.width, 300, accuracy: 1)

        let again = controller()
        again.restoreSidebarWidth()
        again.split.layoutSubtreeIfNeeded()
        XCTAssertEqual(again.sidebarScroll.frame.width, 300, accuracy: 1)
    }

    /// ⌘⇧T, wenn schon Panels da sind: sie kommen vor `restoreSidebarWidth`. Die Baumbreite muss trotzdem gespeichert
    /// werden, sonst fehlt sie nach dem nächsten Start.
    func testTreeWidthStoredWhenPanelsComeFirst() {
        let wc = controller()
        wc.updateExtensionPanels(twoTabs)
        wc.restoreSidebarWidth()
        wc.split.layoutSubtreeIfNeeded()
        let width = wc.sidebarScroll.frame.width
        XCTAssertEqual(Profile.defaults.object(forKey: "sidebar.width" + suffix) as? Double ?? -1, Double(width), accuracy: 1)

        let again = controller()
        again.updateExtensionPanels(twoTabs)
        again.restoreSidebarWidth()
        again.split.layoutSubtreeIfNeeded()
        XCTAssertEqual(again.sidebarScroll.frame.width, width, accuracy: 1)
    }

    /// B4: ein panel.set bei offenem Kontextmenü darf die Aktion nicht an einen anderen Tab oder Eintrag schicken.
    func testContextMenuKeepsTabAndAction() throws {
        let v = ExtensionPanelView(frame: NSRect(x: 0, y: 0, width: 300, height: 400))
        let w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 300, height: 400), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.contentView = v
        defer { w.close() }
        var got: [String] = []
        v.onAction = { name, id in got.append(name + "|" + id) }
        v.panels = twoTabs
        let r = v.rect(0).scaled(Theme.scale)
        let p = v.convert(CGPoint(x: r.midX, y: r.midY), to: nil)
        let click = try XCTUnwrap(NSEvent.mouseEvent(with: .rightMouseDown, location: p, modifierFlags: [], timestamp: 0, windowNumber: w.windowNumber,
                                                     context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        let menu = try XCTUnwrap(v.menu(for: click))
        v.panels = [twoTabs[1]]
        menu.performActionForItem(at: 1)
        XCTAssertEqual(got, ["a|claude:1"])
    }

    func testFocusFallsBackWhenTabVanishes() {
        let wc = controller()
        wc.updateExtensionPanels(twoTabs)
        XCTAssertTrue(wc.window.makeFirstResponder(wc.extensionPanel))

        // Die Extension des gewählten Tabs stirbt, eine andere bleibt: die Tastatur geht trotzdem an die Arbeitsfläche.
        wc.updateExtensionPanels([twoTabs[1]])
        XCTAssertTrue(wc.window.firstResponder === wc.workspace)
        XCTAssertEqual(wc.extensionPanel.selectedTab, "b")

        XCTAssertTrue(wc.window.makeFirstResponder(wc.extensionPanel))
        wc.updateExtensionPanels([])
        XCTAssertTrue(wc.window.firstResponder === wc.workspace)
    }

    // MARK: Statusleiste

    private typealias StatusItem = (name: String, text: String, color: ThemeColor?, action: String?)

    private func barWindow(_ bar: StatusBarView, width: CGFloat) -> NSWindow {
        let w = NSWindow(contentRect: NSRect(x: 100, y: 100, width: width, height: 30), styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        bar.frame = NSRect(x: 0, y: 0, width: width, height: 30)
        w.contentView = bar
        return w
    }

    private func barElements(_ bar: StatusBarView) -> [A11yElement] { (bar.accessibilityChildren() ?? []).compactMap { $0 as? A11yElement } }

    private func click(_ bar: StatusBarView, _ w: NSWindow, on e: A11yElement) {
        let r = e.accessibilityFrame()
        let p = w.convertPoint(fromScreen: CGPoint(x: r.midX, y: r.midY))
        bar.mouseDown(with: NSEvent.mouseEvent(with: .leftMouseDown, location: p, modifierFlags: [], timestamp: 0, windowNumber: w.windowNumber,
                                               context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!)
    }

    func testStatusItemClickSendsAction() throws {
        let bar = StatusBarView(frame: .zero)
        let w = barWindow(bar, width: 1200)
        defer { w.close() }
        var got: [String] = []
        bar.onExtensionAction = { got += [$0, $1] }
        bar.extensionItems = [("ddev", "ddev 3 up", .ok, "ddev:list"), ("info", "nur Text", nil, nil)]
        bar.display()
        let labels = barElements(bar).compactMap { $0.accessibilityLabel() }
        XCTAssertTrue(labels.contains("ddev"))
        XCTAssertFalse(labels.contains("info"), "ohne Aktion kein Knopf")
        click(bar, w, on: try XCTUnwrap(barElements(bar).first { $0.accessibilityLabel() == "ddev" }))
        XCTAssertEqual(got, ["ddev", "ddev:list"])
    }

    func testNarrowBarDropsLeftmostItemsAndKeepsModules() throws {
        let items: [StatusItem] = (1...3).map { ("e\($0)", "Eintrag Nummer \($0)", .ok, "a:\($0)") }
        func labels(width: CGFloat) -> (ext: [String], rest: [String], frames: [NSRect]) {
            let bar = StatusBarView(frame: .zero)
            let w = barWindow(bar, width: width)
            defer { w.close() }
            bar.waitingCount = 2
            bar.extensionItems = items
            bar.display()
            let all = barElements(bar)
            let names = all.compactMap { $0.accessibilityLabel() }
            return (names.filter { $0.hasPrefix("e") && $0.count == 2 }, names.filter { !($0.hasPrefix("e") && $0.count == 2) }, all.map { $0.accessibilityFrame() })
        }
        let wide = labels(width: 1400)
        XCTAssertEqual(wide.ext.sorted(), ["e1", "e2", "e3"])
        let narrow = labels(width: 1200)
        XCTAssertEqual(narrow.rest, wide.rest, "vorhandene Module bleiben vollständig")
        XCTAssertEqual(narrow.ext.sorted(), ["e2", "e3"])
        for (i, a) in narrow.frames.enumerated() { for b in narrow.frames[(i + 1)...] { XCTAssertFalse(a.intersects(b), "\(a) überlappt \(b)") } }
    }

    /// Über viele Breiten (um 1000 px), damit der Rest nach dem letzten passenden Eintrag mal klein ausfällt.
    func testExtensionItemsLeaveRoomForBreadcrumb() {
        let natural = NSAttributedString(string: "kadrell › Eine ziemlich lange Session", attributes: Theme.attrs(11.5, Theme.fg, bold: true)).size().width
        for width in stride(from: 900 as CGFloat, through: 1100, by: 9) {
            let bar = StatusBarView(frame: .zero)
            let w = barWindow(bar, width: width)
            defer { w.close() }
            bar.crumb = (group: "kadrell", session: "Eine ziemlich lange Session")
            bar.extensionItems = (1...6).map { ("e\($0)", "Ein langer Eintrag Nummer \($0)", .ok, "a:\($0)") }
            bar.display()
            XCTAssertGreaterThanOrEqual(bar.crumbWidth, min(natural, StatusBarView.minCrumbWidth) - 1, "Breite \(width)")
            let frames = barElements(bar).map { $0.accessibilityFrame() }
            for (i, a) in frames.enumerated() { for b in frames[(i + 1)...] { XCTAssertFalse(a.intersects(b), "Breite \(width)") } }
        }
    }

    /// Der Manager meldet bis zu 50-mal pro Sekunde; `needsDisplay` ist ohne sichtbares Fenster nicht lesbar, daher der Vergleich selbst.
    func testUnchangedItemsDoNotRedraw() {
        let items: [StatusItem] = [("ddev", "ddev 3 up", .ok, nil)]
        XCTAssertTrue(StatusBarView.sameItems(items, [("ddev", "ddev 3 up", .ok, nil)]))
        XCTAssertFalse(StatusBarView.sameItems(items, [("ddev", "ddev 4 up", .ok, nil)]))
        XCTAssertFalse(StatusBarView.sameItems(items, [("ddev", "ddev 3 up", .warn, nil)]))
        XCTAssertFalse(StatusBarView.sameItems(items, []))
    }

    /// Mit `KADRELL_RENDER_BAR_OUT=<pfad>.png` entsteht ein Bild der Leiste mit zwei Einträgen.
    func testRenderStatusBar() throws {
        let bar = StatusBarView(frame: .zero)
        let w = barWindow(bar, width: 1200)
        defer { w.close() }
        bar.crumb = (group: "kadrell", session: "Statusleiste")
        bar.counts = StatusCounts(running: 2, waiting: 1, idle: 3, error: 0, detached: 1)
        bar.extensionItems = [("ddev", "ddev 3 up", .ok, "ddev:list"), ("jira", "2 Tickets", .warn, nil)]
        bar.display()
        let rep = try XCTUnwrap(bar.bitmapImageRepForCachingDisplay(in: bar.bounds))
        bar.cacheDisplay(in: bar.bounds, to: rep)
        guard let out = ProcessInfo.processInfo.environment["KADRELL_RENDER_BAR_OUT"] else { return }
        try XCTUnwrap(rep.representation(using: .png, properties: [:])).write(to: URL(fileURLWithPath: out))
    }

    func testWarnIsTheYellowDotColorEverywhere() {
        XCTAssertEqual(ThemeColor.warn.nsColor, Theme.waiting)
        XCTAssertEqual(ThemeColor.err.nsColor, Theme.error)
    }

    func testRenderSmoke() throws {
        let colors: [ThemeColor] = [.accent, .muted, .ok, .warn, .err]
        let nodes: [PanelNode] = [
            .section(title: "In Arbeit", collapsed: false, children: colors.enumerated().map { item($0 + 1, color: $1, detail: $1.rawValue) }),
            .section(title: "Zugeklappt", collapsed: true, children: [item(9)]),
        ] + colors.map { .text("Text in \($0.rawValue)", $0) } + [.text("Aktualisiert 14:02", nil), .button(label: "Neu laden", action: "refresh")]
        let v = ExtensionPanelView(frame: NSRect(x: 0, y: 0, width: 320, height: 520))
        v.panels = [("jira", PanelTree(title: "Jira", nodes: nodes)), ("ddev", PanelTree(title: "", nodes: []))]
        let root = Backdrop(frame: v.frame)
        root.addSubview(v)
        let w = NSWindow(contentRect: v.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        w.contentView = root
        defer { w.close() }
        w.makeFirstResponder(v)
        v.keyDown(with: key(KeyCode.down))
        v.keyDown(with: key(KeyCode.down))
        XCTAssertGreaterThan(v.frame.height, 300, "alle Zeilen haben Platz")

        let rep = try XCTUnwrap(root.bitmapImageRepForCachingDisplay(in: root.bounds))
        root.cacheDisplay(in: root.bounds, to: rep)
        guard let out = ProcessInfo.processInfo.environment["KADRELL_RENDER_OUT"] else { return }
        try XCTUnwrap(rep.representation(using: .png, properties: [:])).write(to: URL(fileURLWithPath: out))
    }
}

/// Fenstergrund hinter dem Panel, das selbst halbtransparent malt.
private final class Backdrop: NSView {
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) { Theme.bg.setFill(); dirtyRect.fill() }
}
