import XCTest
@testable import Kadrell

/// Sessions zwischen Gruppen verschieben: Datenhaltung, Ziehen im Baum, Kontextmenü der Gruppe, Tasten im Baum.
@MainActor
final class GroupMoveTests: XCTestCase {
    nonisolated(unsafe) var url: URL!

    override func setUp() {
        url = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)/groups.json")
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: url.deletingLastPathComponent())
    }

    private func session(_ id: String, cwd: String) -> Session { Session(id: id, cwd: cwd, startedAt: 0, sessionId: id, name: id) }

    // MARK: GroupStore

    /// Verschieben hängt hinten an, die Session behält ihren Ordner, und der nächste Abgleich holt sie nicht zurück.
    func testMoveAppendsKeepsCwdAndSurvivesAssign() {
        let store = GroupStore(url: url)
        let sessions = [session("a1", cwd: "/p/a"), session("a2", cwd: "/p/a"), session("b1", cwd: "/p/b")]
        store.assign(sessions)
        let (a, b) = (store.groups[0].id, store.groups[1].id)
        XCTAssertTrue(store.move(sessions: ["a1"], to: b))
        XCTAssertEqual(store.group(id: a)?.sessionIds, ["a2"])
        XCTAssertEqual(store.group(id: b)?.sessionIds, ["b1", "a1"])
        XCTAssertFalse(store.assign(sessions), "der Abgleich ordnet nach cwd nur Sessions ohne Gruppe zu")
        XCTAssertEqual(GroupStore(url: url).groups, store.groups)
        XCTAssertFalse(store.move(sessions: ["a1"], to: b), "schon dort: nichts zu tun")
        XCTAssertFalse(store.move(sessions: ["a1"], to: "gibt-es-nicht"))
    }

    /// Die letzte Session raus: eine Gruppe ohne Herz geht sofort weg, mit Herz bleibt sie stehen.
    func testMovingLastSessionRemovesEmptyGroupUnlessFavorite() {
        let store = GroupStore(url: url)
        store.assign([session("a1", cwd: "/p/a"), session("b1", cwd: "/p/b"), session("c1", cwd: "/p/c")])
        let (a, b, c) = (store.groups[0].id, store.groups[1].id, store.groups[2].id)
        store.toggleFavorite(id: c)
        store.move(sessions: ["a1"], to: b)
        XCTAssertNil(store.group(id: a))
        store.move(sessions: ["c1", "a1", "c1"], to: b)
        XCTAssertEqual(store.group(id: b)?.sessionIds, ["b1", "a1", "c1"], "doppelte Ids zählen einmal")
        XCTAssertEqual(store.group(id: c)?.sessionIds, [], "Favorit bleibt leer stehen")
    }

    // MARK: Ziehen im Baum

    private struct Tree {
        let view: SidebarView
        let window: NSWindow
    }

    /// Zwei Gruppen: g (a, b) und h (c).
    private func tree(sort: SidebarSort = .off) -> Tree {
        let savedScale = Theme.scale
        addTeardownBlock { @MainActor in Theme.scale = savedScale }
        Theme.scale = 1
        let a = session("a", cwd: "/p"), b = session("b", cwd: "/p"), c = session("c", cwd: "/q")
        let g = Group(id: "g", name: "P", color: "#89b4fa", cwd: "/p", sessionIds: ["a", "b"])
        let h = Group(id: "h", name: "Q", color: "#f38ba8", cwd: "/q", sessionIds: ["c"])
        let w = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 240, height: 400), styleMask: .borderless, backing: .buffered, defer: false)
        w.isReleasedWhenClosed = false
        let v = SidebarView(frame: NSRect(x: 0, y: 0, width: 240, height: 400))
        w.contentView = v
        v.sort = sort
        v.reload(groups: [g, h], sessions: [a, b, c])
        return Tree(view: v, window: w)
    }

    private func mouse(_ t: Tree, _ type: NSEvent.EventType, row: Int) {
        let p = t.view.convert(CGPoint(x: 60, y: t.view.rowRect(row).midY), to: nil)
        let e = NSEvent.mouseEvent(with: type, location: p, modifierFlags: [], timestamp: 0, windowNumber: t.window.windowNumber,
                                   context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
        switch type {
        case .leftMouseDown: t.view.mouseDown(with: e)
        case .leftMouseDragged: t.view.mouseDragged(with: e)
        default: t.view.mouseUp(with: e)
        }
    }

    /// Zeilen: 0 Gruppe P, 1 a, 2 b, 3 Gruppe Q, 4 c.
    private func drag(_ t: Tree, from: Int, to: Int) {
        mouse(t, .leftMouseDown, row: from)
        mouse(t, .leftMouseDragged, row: to)
        mouse(t, .leftMouseUp, row: to)
    }

    func testDragSessionOntoOtherGroupMovesIt() {
        let t = tree()
        defer { t.window.close() }
        var moved: ([String], String)?, reordered: (String, String)?
        t.view.onMoveToGroup = { moved = ($0, $1) }
        t.view.onMoveSession = { reordered = ($0, $1) }
        drag(t, from: 1, to: 3)   // a auf den Kopf von Q
        XCTAssertEqual(moved?.0, ["a"]); XCTAssertEqual(moved?.1, "h")
        moved = nil
        drag(t, from: 2, to: 4)   // b auf eine Session von Q: landet ebenso in Q
        XCTAssertEqual(moved?.0, ["b"]); XCTAssertEqual(moved?.1, "h")
        XCTAssertNil(reordered)
        moved = nil
        drag(t, from: 1, to: 2)   // in der eigenen Gruppe: umsortieren wie bisher
        XCTAssertNil(moved)
        XCTAssertEqual(reordered?.0, "a"); XCTAssertEqual(reordered?.1, "b")
    }

    /// Sortiert gibt es keine Handreihenfolge, in eine andere Gruppe ziehen geht trotzdem.
    func testDragWhileSortedOnlyMovesAcrossGroups() {
        let t = tree(sort: .alpha)
        defer { t.window.close() }
        var moved: ([String], String)?, reordered: (String, String)?
        t.view.onMoveToGroup = { moved = ($0, $1) }
        t.view.onMoveSession = { reordered = ($0, $1) }
        drag(t, from: 1, to: 2)
        XCTAssertNil(reordered)
        drag(t, from: 1, to: 3)
        XCTAssertEqual(moved?.1, "h")
    }

    /// Während des Ziehens über einer fremden Gruppe ist sie als Ganzes markiert (Akzentrahmen), keine Einfügelinie.
    func testDropTargetGroupIsHighlighted() throws {
        let t = tree()
        defer { t.window.close() }
        func pixel(_ row: Int) throws -> NSColor {
            let rep = try XCTUnwrap(t.view.bitmapImageRepForCachingDisplay(in: t.view.bounds))
            t.view.cacheDisplay(in: t.view.bounds, to: rep)
            let k = CGFloat(rep.pixelsWide) / t.view.bounds.width
            // Linker Rand der Zeile, wo der Ring liegt (3 pt eingerückt, 2 pt breit).
            return try XCTUnwrap(rep.colorAt(x: Int(4 * k), y: Int(t.view.rowRect(row).midY * k))?.usingColorSpace(.sRGB))
        }
        let before = try pixel(4)
        mouse(t, .leftMouseDown, row: 1)
        mouse(t, .leftMouseDragged, row: 4)
        let during = try pixel(4)
        mouse(t, .leftMouseUp, row: 4)
        XCTAssertNotEqual(before, during, "Zielgruppe ist beim Ziehen sichtbar markiert")
    }

    // MARK: Kontextmenü und Tasten

    func testRightClickOnGroupRowAsksForGroupMenu() {
        let t = tree()
        defer { t.window.close() }
        var asked: String?
        t.view.onGroupContextMenu = { asked = $0; return NSMenu() }
        let p = t.view.convert(CGPoint(x: 60, y: t.view.rowRect(3).midY), to: nil)
        let e = NSEvent.mouseEvent(with: .rightMouseDown, location: p, modifierFlags: [], timestamp: 0, windowNumber: t.window.windowNumber,
                                   context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
        XCTAssertNotNil(t.view.menu(for: e))
        XCTAssertEqual(asked, "h")
    }

    private func key(_ code: UInt16, _ mods: NSEvent.ModifierFlags = []) -> NSEvent {
        NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: mods, timestamp: 0, windowNumber: 0, context: nil,
                         characters: "", charactersIgnoringModifiers: "", isARepeat: false, keyCode: code)!
    }

    /// ⌃⏎ öffnet kein Menü mehr, es geht an die Session. ⇧F10 auf einer zugeklappten Gruppe fragt nach deren Menü.
    func testControlReturnPassesThroughAndShiftF10OnCollapsedGroup() {
        let t = tree()
        defer { t.window.close() }
        t.view.focused = "c"
        var passed: NSEvent?, sessionMenu = 0, groupMenu: String?
        t.view.onPassKey = { passed = $0 }
        t.view.onContextMenu = { _ in sessionMenu += 1; return nil }
        t.view.onGroupContextMenu = { groupMenu = $0; return nil }
        t.view.keyDown(with: key(KeyCode.returnKey, .control))
        XCTAssertEqual(passed?.keyCode, KeyCode.returnKey)
        XCTAssertEqual(sessionMenu, 0, "⌃⏎ ist keine Menütaste mehr")
        t.view.keyDown(with: key(KeyCode.f10, .shift))
        XCTAssertEqual(sessionMenu, 1)
        XCTAssertNil(groupMenu)
        t.view.keyDown(with: key(KeyCode.left))   // Gruppe Q zuklappen, die Fokus-Session ist nicht mehr zu sehen
        t.view.keyDown(with: key(KeyCode.f10, .shift))
        XCTAssertEqual(sessionMenu, 1)
        XCTAssertEqual(groupMenu, "h")
    }
}

/// Gruppe bearbeiten: Ordner änderbar, mit denselben Kürzeln wie bei Neue Session.
@MainActor
final class EditGroupFolderTests: XCTestCase {
    nonisolated(unsafe) var root: URL!

    override func setUp() {
        root = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-edit-\(UUID().uuidString)")
        for d in ["alpha", "beta", "betamax"] {
            try? FileManager.default.createDirectory(at: root.appendingPathComponent(d), withIntermediateDirectories: true)
        }
    }

    override func tearDown() { try? FileManager.default.removeItem(at: root) }

    private func path(_ d: String) -> String { root.appendingPathComponent(d).path }

    func testNewFolderIsSavedAndDefaultNameFollows() {
        let g = Group(id: "g", name: "alpha", color: "#89b4fa", cwd: path("alpha"), sessionIds: ["s"])
        let model = EditGroupModel(group: g)
        var saved: Group?
        model.onSave = { saved = $0 }
        model.folder = path("betamax")
        XCTAssertTrue(model.folderMoved)
        XCTAssertEqual(model.name, "betamax", "der automatische Name zieht mit")
        model.save()
        XCTAssertEqual(saved?.cwd, path("betamax"))
        XCTAssertEqual(saved?.sessionIds, ["s"], "Sessions bleiben in der Gruppe")
        XCTAssertEqual(saved?.id, "g")
    }

    func testOwnNameStaysAndAbbreviationResolves() {
        let g = Group(id: "g", name: "Mein Projekt", color: "#89b4fa", cwd: path("alpha"), sessionIds: [])
        let model = EditGroupModel(group: g)
        model.folder = root.path + "/betam"
        XCTAssertEqual(model.resolvedFolder, path("betamax"), "eindeutiges Kürzel")
        XCTAssertEqual(model.name, "Mein Projekt")
        model.folder = root.path + "/b"
        XCTAssertNil(model.resolvedFolder, "mehrdeutig: beta und betamax")
        model.completeFolder()
        XCTAssertNotNil(model.resolvedFolder, "Tab schreibt den ersten Treffer aus")
    }

    func testMissingFolderBlocksSaveAndOtherGroupIsHinted() {
        let other = Group(id: "o", name: "Beta", color: "#f38ba8", cwd: path("beta"), sessionIds: [])
        let g = Group(id: "g", name: "alpha", color: "#89b4fa", cwd: path("alpha"), sessionIds: [])
        let model = EditGroupModel(group: g, others: [other])
        var saved: Group?
        model.onSave = { saved = $0 }
        model.folder = path("gibt-es-nicht")
        XCTAssertTrue(model.folderHint.error)
        model.save()
        XCTAssertNil(saved, "ohne gültigen Ordner kein Speichern")
        model.folder = path("beta")
        XCTAssertEqual(model.sharedWith?.id, "o")
        XCTAssertFalse(model.folderHint.error)
        model.save()
        XCTAssertEqual(saved?.cwd, path("beta"), "zwei Gruppen im selben Ordner sind erlaubt, nur ein Hinweis")
    }

    /// Remote-Gruppen hängen am Host, ihr Ordner ist Platzhalter und bleibt.
    func testRemoteGroupKeepsPlaceholderFolder() {
        let g = Group(id: "r", name: "host", color: "#89b4fa", cwd: NSHomeDirectory(), sessionIds: [], host: "host")
        let model = EditGroupModel(group: g)
        var saved: Group?
        model.onSave = { saved = $0 }
        model.name = "Server"
        model.save()
        XCTAssertEqual(saved?.cwd, NSHomeDirectory())
        XCTAssertEqual(saved?.name, "Server")
        XCTAssertEqual(saved?.host, "host")
    }
}
