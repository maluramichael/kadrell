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

    override func tearDown() {
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
