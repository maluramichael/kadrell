import AppKit

/// Untermenü „Verschieben nach“: die Gruppen kommen erst beim Aufklappen hinein (`menuNeedsUpdate`), damit auch das
/// einmal gebaute Hauptmenü nie eine veraltete Liste zeigt. `sessionIds` nil heißt: die fokussierte Session.
final class MoveToGroupMenu: NSMenu {
    let sessionIds: [String]?

    init(title: String, sessionIds: [String]?) {
        self.sessionIds = sessionIds
        super.init(title: title)
        autoenablesItems = false
    }

    required init(coder: NSCoder) { fatalError("nicht aus einem Nib") }
}

/// Ziel eines Eintrags in „Verschieben nach“.
struct MoveRequest {
    let sessionIds: [String]
    let groupId: String
}

/// Kontextmenü einer Gruppe (Rechtsklick auf die Gruppenzeile, ⇧F10 auf der zugeklappten Gruppe, Untermenü im
/// Kontextmenü jeder Session) und das Verschieben von Sessions zwischen Gruppen. Alles läuft über dieselben Wege
/// wie die schwebende Toolbar am Gruppenkopf und `kadrell move`.
extension AppDelegate: NSMenuDelegate {
    /// Baum, Kontextmenü und `kadrell move`: die Sessions wechseln die Gruppe, behalten Ordner und Prozess.
    func moveSessions(_ ids: [String], toGroup gid: String) {
        guard store.move(sessions: ids, to: gid) else { return }
        reloadViews()
    }

    /// Eintrag „Verschieben nach ▸“ für die Session `id` (nil: die fokussierte, Hauptmenü).
    func moveMenuItem(for ids: [String]?, title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        let menu = MoveToGroupMenu(title: title, sessionIds: ids)
        menu.delegate = self
        item.submenu = menu
        return item
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        guard let menu = menu as? MoveToGroupMenu else { return }
        menu.removeAllItems()
        let ids = menu.sessionIds ?? workspace.focused.map { [$0] } ?? []
        guard !ids.isEmpty else {
            menu.addItem(disabled: String(localized: "Keine Session fokussiert", bundle: Bundle.app))
            return
        }
        for item in moveTargets(for: ids) { menu.addItem(item) }
    }

    /// Alle Gruppen in Baumreihenfolge. Wo die Sessions schon stehen, ist der Eintrag abgehakt und aus. Gleichnamige
    /// Gruppen tragen ihren Ordner dahinter, sonst wären sie nicht zu unterscheiden.
    private func moveTargets(for ids: [String]) -> [NSMenuItem] {
        let names = Dictionary(grouping: store.groups, by: \.name)
        return store.groups.map { g in
            let label = (names[g.name]?.count ?? 0) > 1 ? "\(g.name) · \(g.host.map { "ssh \($0)" } ?? Theme.shortPath(g.cwd))" : g.name
            let item = NSMenuItem(title: label, action: #selector(menuMoveToGroup(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = MoveRequest(sessionIds: ids, groupId: g.id)
            item.image = Self.groupDot(Theme.group(g.color))
            let here = ids.allSatisfy { g.sessionIds.contains($0) }
            item.state = here ? .on : .off
            item.isEnabled = !here
            return item
        }
    }

    @objc func menuMoveToGroup(_ sender: NSMenuItem) {
        guard let r = sender.representedObject as? MoveRequest else { return }
        moveSessions(r.sessionIds, toGroup: r.groupId)
    }

    /// Farbpunkt der Gruppe vor ihrem Namen, wie im Baum.
    static func groupDot(_ color: NSColor) -> NSImage {
        NSImage(size: NSSize(width: 10, height: 10), flipped: false) { r in
            color.setFill()
            NSBezierPath(ovalIn: r.insetBy(dx: 1, dy: 1)).fill()
            return true
        }
    }

    // MARK: Kontextmenü der Gruppe

    func groupMenu(for gid: String) -> NSMenu? {
        guard let g = store.group(id: gid) else { return nil }
        let menu = NSMenu(title: g.name)
        menu.autoenablesItems = false
        for item in groupMenuItems(for: g) { menu.addItem(item) }
        return menu
    }

    /// Dieselben Aktionen wie die Toolbar am Gruppenkopf, dazu Ordner-Aktionen wie bei einer Session. Remote-Gruppen
    /// haben keinen lokalen Ordner: dort fehlen Terminal, Pfad, Finder und Editor.
    func groupMenuItems(for g: Group) -> [NSMenuItem] {
        func item(_ title: String, _ action: Selector, enabled: Bool = true) -> NSMenuItem {
            let i = NSMenuItem(title: title, action: action, keyEquivalent: "")
            i.target = self
            i.representedObject = g.id
            i.isEnabled = enabled
            return i
        }
        let local = g.host == nil, hasSessions = g.sessionIds.contains { workspace.session($0) != nil }
        let favorite = item(String(localized: "Favorit", bundle: Bundle.app), #selector(menuGroupFavorite(_:)))
        favorite.state = g.isFavorite ? .on : .off
        var items = [
            item(local ? String(localized: "Neue Session in dieser Gruppe", bundle: Bundle.app) : String(localized: "Remote-Session wählen …", bundle: Bundle.app),
                 #selector(menuGroupNewSession(_:))),
        ]
        if local { items.append(item(String(localized: "Neues Terminal in dieser Gruppe", bundle: Bundle.app), #selector(menuGroupNewTerminal(_:)))) }
        items += [
            item(String(localized: "Alle Sessions zeigen", bundle: Bundle.app), #selector(menuGroupShowAll(_:)), enabled: hasSessions),
            .separator(),
            item(String(localized: "Bearbeiten …", bundle: Bundle.app), #selector(menuGroupEdit(_:))),
            favorite,
        ]
        if local {
            items += [
                .separator(),
                item(String(localized: "Pfad kopieren", bundle: Bundle.app), #selector(menuGroupCopyPath(_:))),
                item(String(localized: "Im Finder zeigen", bundle: Bundle.app), #selector(menuGroupShowInFinder(_:))),
                item(String(localized: "Im Editor öffnen", bundle: Bundle.app), #selector(menuGroupOpenEditor(_:)),
                     enabled: !Settings.editorCommand.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty),
            ]
        }
        items += [.separator(), item(String(localized: "Gruppe schließen …", bundle: Bundle.app), #selector(menuGroupClose(_:)))]
        return items
    }

    private func contextGroup(_ sender: NSMenuItem) -> Group? {
        (sender.representedObject as? String).flatMap { store.group(id: $0) }
    }

    @objc private func menuGroupNewSession(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        openNewSession(groupId: g.id)
    }

    @objc private func menuGroupNewTerminal(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        openNewTerminal(groupId: g.id)
    }

    @objc private func menuGroupShowAll(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        workspace.select(g.sessionIds, add: false)
    }

    @objc private func menuGroupEdit(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        openEditGroup(g.id)
    }

    @objc private func menuGroupFavorite(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        store.toggleFavorite(id: g.id)
        reloadViews()
    }

    @objc private func menuGroupCopyPath(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        NSPasteboard.general.copy(g.cwd)
    }

    @objc private func menuGroupShowInFinder(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: g.cwd)
    }

    @objc private func menuGroupOpenEditor(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        openEditor(in: g.cwd)
    }

    @objc private func menuGroupClose(_ sender: NSMenuItem) {
        guard let g = contextGroup(sender) else { return }
        closeGroup(g.id)
    }

    // MARK: Tasten aus dem Baum an die Session

    /// Hat der Baum die Tastatur (⌘1, ↑↓), soll eine Taste, die Claude selbst verlangt (⌃⏎), trotzdem dort ankommen:
    /// die Fokus-Session bekommt die Tastatur und die Taste.
    func passKeyToFocusedSession(_ event: NSEvent) {
        guard let f = workspace.focused else { NSSound.beep(); return }
        workspace.setFocus(f)
        guard let t = attach.terminal(for: f), t.window?.firstResponder === t else { NSSound.beep(); return }
        t.keyDown(with: event)
    }
}

extension NSMenu {
    /// Grauer Hinweis statt leerem Untermenü.
    func addItem(disabled title: String) {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        addItem(item)
    }
}
