import AppKit
import SwiftUI

/// Dialoge als Overlay über dem aktuellen Hauptfenster: Rückfragen, Mitteilungen, Hilfe (F1) und eigene Sheets.
@MainActor
final class SheetPresenter {
    /// Aktuelles Hauptfenster; nach dem Schließen bekommt dessen Arbeitsfläche die Tastatur zurück.
    private let host: () -> MainWindowController?
    /// Eine offene Palette macht dem Dialog Platz.
    private let palette: () -> PaletteWindow?
    private var overlay: OverlayPanel?
    /// Offenes Panel (F1, F3) nicht stillschweigend verdrängen: wer gerade ⏎ drückt, um es zu schließen, soll nicht
    /// aus Versehen einen anderen Dialog bestätigen. Der neue Dialog kommt erst dran, wenn das Panel zu ist.
    private var pending: (() -> Void)?

    /// Panels, die sich mit ihrer eigenen Taste wieder schließen, im Gegensatz zu Rückfragen und Sheets.
    enum Panel { case about, stats }
    private var openPanel: Panel?

    init(host: @escaping () -> MainWindowController?, palette: @escaping () -> PaletteWindow?) {
        self.host = host
        self.palette = palette
    }

    /// Ohne `onCancel` schließt Esc einfach den Dialog.
    func present<V: View>(_ view: V, plainReturn: Bool = true, onCancel: (() -> Void)? = nil, onPrimary: @escaping () -> Void) {
        if openPanel != nil {
            pending = { [weak self] in self?.present(view, plainReturn: plainReturn, onCancel: onCancel, onPrimary: onPrimary) }
            return
        }
        dismiss()
        if let palette = palette(), palette.isVisible { palette.dismiss() }
        let p = OverlayPanel(rootView: view)
        p.onCancel = onCancel ?? { [weak self] in self?.dismiss() }
        p.onPrimary = onPrimary
        p.primaryOnPlainReturn = plainReturn
        overlay = p
        if let window = host()?.window { p.open(over: window) }
    }

    func applyTheme() { overlay?.applyTheme() }

    func dismiss() {
        overlay?.dismiss()
        overlay = nil
        openPanel = nil
        if let c = host() { c.window.makeFirstResponder(c.workspace) }
        if let pending { self.pending = nil; pending() }
    }

    /// F1 und F3: Panel auf, dasselbe Panel noch einmal schließt es wieder.
    func togglePanel(_ panel: Panel) {
        if overlay?.isVisible == true, openPanel == panel { dismiss(); return }
        let close: () -> Void = { [weak self] in self?.dismiss() }
        switch panel {
        case .about: present(AboutView(), onPrimary: close)
        case .stats: present(StatsView(), onPrimary: close)
        }
        openPanel = panel   // erst nach present: das räumt über dismiss den alten Wert ab
    }

    /// Ein offenes Panel hat selbst die Tastatur: seine eigene Taste im Panel-Fenster schließt es wieder.
    func isPanel(_ panel: Panel, _ window: NSWindow?) -> Bool { openPanel == panel && window === overlay }

    /// Dialog offen, aber nicht mehr Key (nach Dropdown, Klick daneben oder App-Wechsel): Esc landet am
    /// Hauptfenster statt am Panel. Ohne das schließt nichts den Dialog, der Blur bleibt liegen und blockt
    /// alle Klicks (Softlock). onCancel räumt den Backdrop mit ab.
    func cancelVisible() -> Bool {
        guard let overlay, overlay.isVisible else { return false }
        overlay.onCancel?()
        return true
    }

    /// Rückfrage im App-Design. Nicht-destruktive bestätigt blankes ⏎, destruktive nur ⌘⏎. Esc bricht immer ab.
    /// `skip` (⌥+Klick) führt direkt aus. `ask` bietet „Nicht mehr fragen“ an; ist die Rückfrage abgeschaltet, läuft die Aktion sofort.
    func confirm(_ message: String, _ info: String, button: String, destructive: Bool = true, skip: Bool = false,
                 infoOnly: Bool = false, ask: Settings.Ask? = nil, then action: @escaping () -> Void) {
        if skip || ask?.enabled == false { action(); return }
        let run = { [weak self] in self?.dismiss(); action() }
        let cancel = { [weak self] in ask?.enabled = true; self?.dismiss() }
        present(ConfirmView(title: message, info: info, button: button, destructive: destructive, infoOnly: infoOnly, ask: ask,
                            onConfirm: run, onCancel: cancel),
                plainReturn: !destructive, onCancel: cancel, onPrimary: run)
    }

    /// Mitteilung ohne echte Alternative: nur „OK“, kein Abbrechen, das dasselbe täte (siehe Kanboard #73).
    func inform(_ title: String, _ detail: String) {
        confirm(title, detail, button: String(localized: "OK", bundle: Bundle.app), destructive: false, infoOnly: true) {}
    }

    /// Fehlermeldung, zusätzlich im Log. `detail` ist die erste Zeile der CLI-Ausgabe, nicht der ganze Prozess-Output.
    func report(_ detail: String, title: String = String(localized: "Claude CLI meldet einen Fehler", bundle: Bundle.app)) {
        AppDelegate.log.error("\(detail, privacy: .private)")
        inform(title, detail)
    }
}
