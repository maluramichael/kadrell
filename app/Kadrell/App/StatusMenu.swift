import Foundation

/// Reine Aufbereitung fürs Menüleisten-Menü (`AppDelegate.buildStatusMenu`), ohne AppKit: testbar ohne Fenster.
enum StatusMenu {
    /// Wartende zuerst, danach ungesehen fertige (nicht wartende), beide in Baumreihenfolge, keine Dopplungen.
    static func rows(order: [String], waiting: Set<String>, unseen: Set<String>) -> (waiting: [String], done: [String]) {
        (order.filter { waiting.contains($0) }, order.filter { unseen.contains($0) && !waiting.contains($0) })
    }

    /// „3 warten · 1 neu“, leer ohne beides.
    static func title(waiting: Int, done: Int) -> String {
        waiting == 0 && done == 0 ? "" : String(localized: "  \(waiting) warten · \(done) neu")
    }
}
