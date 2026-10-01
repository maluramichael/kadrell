import AppKit

/// Reine Aufbereitung fürs Menüleisten-Menü (`StatusItemController`), ohne AppKit: testbar ohne Fenster.
enum StatusMenu {
    /// Wartende zuerst, danach ungesehen fertige (nicht wartende), beide in Baumreihenfolge, keine Dopplungen.
    static func rows(order: [String], waiting: Set<String>, unseen: Set<String>) -> (waiting: [String], done: [String]) {
        (order.filter { waiting.contains($0) }, order.filter { unseen.contains($0) && !waiting.contains($0) })
    }

    /// „3 warten · 1 neu“, leer ohne beides.
    static func title(waiting: Int, done: Int) -> String {
        waiting == 0 && done == 0 ? "" : String(localized: "  \(waiting) warten · \(done) neu", bundle: Bundle.app)
    }
}

/// Menüleisten-Icon mit Kurzstatus. Sein Menü listet wartende und ungesehen fertige Sessions, Klick fokussiert
/// sie. Bleibt sichtbar, solange Kadrell läuft, auch wenn das Fenster versteckt ist.
@MainActor
final class StatusItemController: NSObject, NSMenuDelegate {
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    private let store: GroupStore
    var onFocus: (String) -> Void = { _ in }
    var onNextWaiting: () -> Void = {}
    var onOpen: () -> Void = {}
    /// Stand vom letzten `update`, das Menü entsteht daraus erst beim Öffnen (`menuNeedsUpdate`).
    private var rows: (waiting: [String], done: [String]) = ([], [])
    private var sessions: [String: Session] = [:]
    let menu = NSMenu()

    init(store: GroupStore) {
        self.store = store
        super.init()
        item.button?.image = NSImage(systemSymbolName: "terminal", accessibilityDescription: "Kadrell")
        item.button?.image?.isTemplate = true
        menu.delegate = self
        item.menu = menu
    }

    /// "3 warten · 1 neu" im Menüleisten-Icon (ungesehen fertige zählen als „neu“ mit). Läuft bei jedem Abgleich,
    /// deshalb nur Titel und Daten merken, das Menü baut erst `menuNeedsUpdate`.
    func update(_ sessions: [Session], unseen: Set<String>) {
        self.sessions = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        let waiting = Set(sessions.filter { $0.status == .waiting }.map(\.id))
        rows = StatusMenu.rows(order: store.groups.flatMap(\.sessionIds), waiting: waiting, unseen: unseen)
        item.button?.title = StatusMenu.title(waiting: rows.waiting.count, done: rows.done.count)
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        func add(_ title: String, _ action: Selector) -> NSMenuItem {
            let item = menu.addItem(withTitle: title, action: action, keyEquivalent: "")
            item.target = self
            return item
        }
        func addRow(_ key: String, waiting: Bool) {
            guard let s = sessions[key] else { return }
            let group = store.group(forSession: key)?.name ?? ""
            let item = add("\(group) › \(s.title) · \(s.elapsed())", #selector(select(_:)))
            item.representedObject = key
            item.image = NSImage(systemSymbolName: waiting ? "clock" : "checkmark.circle", accessibilityDescription: nil)
        }
        if rows.waiting.isEmpty, rows.done.isEmpty {
            menu.addItem(withTitle: String(localized: "Keine wartenden Sessions", bundle: Bundle.app), action: nil, keyEquivalent: "").isEnabled = false
        } else {
            for key in rows.waiting { addRow(key, waiting: true) }
            for key in rows.done { addRow(key, waiting: false) }
            menu.addItem(.separator())
            add(String(localized: "Nächste wartende Session", bundle: Bundle.app), #selector(nextWaiting)).isEnabled = !rows.waiting.isEmpty
        }
        menu.addItem(.separator())
        _ = add(String(localized: "Kadrell öffnen", bundle: Bundle.app), #selector(open))
    }

    @objc private func select(_ sender: NSMenuItem) {
        guard let key = sender.representedObject as? String else { return }
        onFocus(key)
    }
    @objc private func nextWaiting() { onNextWaiting() }
    @objc private func open() { onOpen() }
}
