import AppKit

/// Kontextmenü einer Session: dieselben Einträge hängen an Baumzeile, Kachel-Header und Stack-Zeile
/// (Kontextmenü, `representedObject` trägt die Session-Id) sowie am Hauptmenü „Session“ (Kürzel, wirkt
/// auf die fokussierte Session, `representedObject` bleibt nil). Ein Menü, eine Stelle für Enabled-Logik.
extension AppDelegate {
    func sessionMenu(for id: String) -> NSMenu? {
        guard workspace.session(id) != nil else { return nil }
        let menu = NSMenu()
        for item in sessionMenuItems(for: id, shortcuts: false) { menu.addItem(item) }
        return menu
    }

    func sessionMenuItems(for id: String?, shortcuts: Bool) -> [NSMenuItem] {
        func item(_ title: String, _ action: Selector, key: String = "") -> NSMenuItem {
            let i = NSMenuItem(title: title, action: action, keyEquivalent: shortcuts ? key : "")
            i.target = self
            i.representedObject = id
            return i
        }
        return [
            item("Fortsetzen", #selector(menuContextResume(_:)), key: "r"),
            item("Stoppen", #selector(menuContextStop(_:)), key: "."),
            item("Entfernen …", #selector(menuContextClose(_:))),
            .separator(),
            item("Nur diese zeigen", #selector(menuContextIsolate(_:))),
            item("Aus Ansicht nehmen", #selector(menuContextRemoveFromView(_:))),
            item("Neue Session hier", #selector(menuContextNewHere(_:))),
            .separator(),
            item("Pfad kopieren", #selector(menuContextCopyPath(_:))),
            item("Im Finder zeigen", #selector(menuContextShowInFinder(_:))),
        ]
    }

    /// `representedObject` (Kontextmenü) geht vor, sonst die fokussierte Session (Hauptmenü). Auch von `validateMenuItem` genutzt.
    fileprivate func contextSession(_ sender: NSMenuItem) -> Session? {
        ((sender.representedObject as? String) ?? workspace.focused).flatMap { workspace.session($0) }
    }

    @objc private func menuContextResume(_ sender: NSMenuItem) {
        guard let s = contextSession(sender) else { return }
        attach.attachNow(s)
        workspace.select([s.id], add: false)
    }

    @objc private func menuContextStop(_ sender: NSMenuItem) {
        guard let s = contextSession(sender) else { return }
        stopSession(s)
    }

    @objc private func menuContextClose(_ sender: NSMenuItem) {
        guard let s = contextSession(sender) else { return }
        closeSession(s.id)
    }

    @objc private func menuContextIsolate(_ sender: NSMenuItem) {
        guard let s = contextSession(sender) else { return }
        workspace.select([s.id], add: false)
    }

    /// Aus der Auswahl nehmen, ohne Claude zu beenden: `select(add: true)` toggelt eine bereits gezeigte Session weg.
    @objc private func menuContextRemoveFromView(_ sender: NSMenuItem) {
        guard let s = contextSession(sender) else { return }
        workspace.select([s.id], add: true)
    }

    @objc private func menuContextNewHere(_ sender: NSMenuItem) {
        guard let s = contextSession(sender), !s.cwd.isEmpty else { return }
        startSession(group: store.group(forSession: s.id), cwd: s.cwd)
    }

    @objc private func menuContextCopyPath(_ sender: NSMenuItem) {
        guard let s = contextSession(sender), !s.cwd.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(s.cwd, forType: .string)
    }

    @objc private func menuContextShowInFinder(_ sender: NSMenuItem) {
        guard let s = contextSession(sender), !s.cwd.isEmpty else { return }
        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: s.cwd)
    }
}

/// Enabled-Status live vor jedem Anzeigen, Kontextmenü wie Hauptmenü: Fortsetzen nur ohne laufenden Prozess,
/// Stoppen nur mit, Aus Ansicht nehmen nur für gerade gezeigte Sessions. Andere Menüpunkte (nil-Target
/// landet über die Responder-Kette ebenfalls hier) bleiben unangetastet.
extension AppDelegate: NSMenuItemValidation {
    func validateMenuItem(_ menuItem: NSMenuItem) -> Bool {
        switch menuItem.action {
        case #selector(menuContextResume(_:)): return contextSession(menuItem).map { !attach.isAttached($0.id) } ?? false
        case #selector(menuContextStop(_:)): return contextSession(menuItem).map { attach.isAttached($0.id) } ?? false
        case #selector(menuContextRemoveFromView(_:)): return contextSession(menuItem).map { workspace.selected.contains($0.id) } ?? false
        case #selector(menuContextClose(_:)), #selector(menuContextIsolate(_:)), #selector(menuContextNewHere(_:)),
             #selector(menuContextCopyPath(_:)), #selector(menuContextShowInFinder(_:)):
            return contextSession(menuItem) != nil
        default: return true
        }
    }
}
