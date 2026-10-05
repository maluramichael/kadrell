import AppKit
import SwiftUI

/// Dünne Hülle um den Manager für den F4-Dialog. Der Manager meldet bis zu 50-mal pro Sekunde (jede Logzeile),
/// SwiftUI soll den Dialog trotzdem nicht so oft neu bauen: Änderungen gehen höchstens alle 0,2 s raus.
@MainActor
final class ExtensionsModel: ObservableObject {
    static let coalesce: TimeInterval = 0.2

    let manager: ExtensionManager
    @Published private(set) var selected = 0
    @Published var showLog = false
    /// Marktplatz: Suchbegriff, gefundene GitHub-Repos, laufende Suche, gerade installierende und letzter Fehler.
    @Published var query = ""
    @Published private(set) var results: [RemoteExtension] = []
    @Published private(set) var searching = false
    @Published private(set) var installing: Set<String> = []
    @Published private(set) var storeError: String?
    /// Gespeicherte Werte der gewählten Extension ohne Geheimnisse. Von denen steht nur hier, ob sie gesetzt sind.
    @Published private(set) var stored: [String: JSONValue] = [:]
    @Published private(set) var secretsSet: Set<String> = []
    /// Fenster des offenen Dialogs, vom `SheetPresenter` gesetzt; nil heißt zu. Formularwerte zählen nur, solange es da ist:
    /// was nach dem Schließen noch an Fokuswechseln nachläuft, verwirft seinen Entwurf.
    weak var window: NSWindow?
    private var lastPublish = Date.distantPast
    private var pending = false

    init(manager: ExtensionManager) {
        self.manager = manager
        let previous = manager.onChange
        manager.onChange = { [weak self] in
            previous()
            self?.changed()
        }
    }

    var items: [FoundExtension] { manager.found }
    var current: FoundExtension? { items.indices.contains(selected) ? items[selected] : nil }

    func canToggle(_ ext: FoundExtension) -> Bool { ext.problem == nil }
    func isEnabled(_ name: String) -> Bool { Settings.enabledExtensions.contains(name) }

    func select(_ index: Int) {
        guard !items.isEmpty else { return }
        let i = min(max(index, 0), items.count - 1)
        guard i != selected else { return }
        selected = i
        Task { await load() }
    }

    /// Klick auf eine Zeile beendet ein offenes Textfeld (Entwurf verworfen), sonst behielte es die Tastatur
    /// und ↑↓, Leertaste, R und L gingen ins Feld.
    func click(_ index: Int) {
        window?.makeFirstResponder(nil)
        select(index)
    }

    /// Fokus ging in ein anderes Textfeld (Tab, Klick): dann speichert das verlassene Feld wie mit ⏎.
    var editingAnotherField: Bool { window?.firstResponder is NSText }

    func toggle(_ ext: FoundExtension) {
        guard canToggle(ext) else { return NSSound.beep() }
        manager.setEnabled(ext.name, !isEnabled(ext.name))
        publish()
    }

    func reload(_ ext: FoundExtension) {
        guard canToggle(ext), isEnabled(ext.name) else { return NSSound.beep() }
        manager.reload(ext.name)
    }

    func openFolder(_ ext: FoundExtension) { NSWorkspace.shared.open(ext.dir) }

    /// Ein Repo gilt als installiert, wenn eine Extension so heißt wie das Repo. Heißt der Ordner nach dem Manifest
    /// anders als das Repo, bietet der Marktplatz eben noch einmal „Installieren" an, das schadet nicht.
    func isInstalled(_ r: RemoteExtension) -> Bool { items.contains { $0.name == r.repo } }

    func searchStore() async {
        let q = query
        searching = true
        storeError = nil
        defer { searching = false }
        do {
            let found = try await GitHubExtensions.search(q)
            if query == q { results = found }
        } catch {
            storeError = String(localized: "Suche fehlgeschlagen: \(error.localizedDescription)", bundle: Bundle.app)
        }
    }

    func install(_ r: RemoteExtension) async {
        installing.insert(r.id)
        storeError = nil
        defer { installing.remove(r.id) }
        do {
            try await GitHubExtensions.install(r, into: manager.catalogDir)
            manager.refresh()
        } catch {
            storeError = String(localized: "Installieren fehlgeschlagen: \(error.localizedDescription)", bundle: Bundle.app)
        }
    }

    /// Legt den Ordner bei Bedarf an: wer noch keine Extension hat, soll trotzdem wissen, wohin damit.
    func openCatalog() {
        try? FileManager.default.createDirectory(at: manager.catalogDir, withIntermediateDirectories: true)
        NSWorkspace.shared.open(manager.catalogDir)
    }

    /// ↑↓ wählen, Leertaste an/aus, R neu laden, L Log. Esc schließt das Panel selbst.
    @discardableResult
    func handleKey(_ key: KeyEquivalent) -> Bool {
        switch key {
        case .upArrow: select(selected - 1)
        case .downArrow: select(selected + 1)
        case .space: if let ext = current { toggle(ext) }
        case "r", "R": if let ext = current { reload(ext) }
        case "l", "L": showLog.toggle()
        default: return false
        }
        return true
    }

    /// Taste ohne Modifier als `KeyEquivalent`; Pfeile über ihren Code, weil ihre Zeichen Funktionstasten-Codes sind.
    static func key(for event: NSEvent) -> KeyEquivalent? {
        guard event.modifierFlags.intersection(Hotkey.modMask).isEmpty else { return nil }
        switch event.keyCode {
        case KeyCode.up: return .upArrow
        case KeyCode.down: return .downArrow
        default: return event.charactersIgnoringModifiers?.first.map { KeyEquivalent($0) }
        }
    }

    /// Taste für das offene Panel, nil wenn sie nicht dem Dialog gehört: mit Modifier oder solange ein Textfeld
    /// oder Schalter die Tastatur hat, dort gehören Leertaste und Buchstaben dem Feld.
    static func panelKey(_ event: NSEvent, in window: NSWindow?) -> KeyEquivalent? {
        let responder = window?.firstResponder
        guard !(responder is NSText || responder is NSControl) else { return nil }
        return key(for: event)
    }

    func load() async {
        guard let ext = current else { return }
        let values = await ExtensionSettings.values(for: ext)
        guard current?.name == ext.name else { return }
        let secrets = Set((ext.manifest?.settings ?? []).filter { $0.type == "secret" }.map(\.key))
        stored = values.filter { !secrets.contains($0.key) }
        secretsSet = secrets.filter { values[$0] != nil }
    }

    /// Aus dem Formular, nur bei offenem Dialog. Schließen (Esc, F4, ⌘⏎) heißt abbrechen, nicht speichern.
    func commit(_ key: String, _ value: JSONValue, in ext: FoundExtension) {
        guard window != nil else { return }
        Task { await save(key, value, in: ext) }
    }

    /// Eine eingeschaltete Extension startet danach neu, sonst sähe sie den neuen Wert in `kadrell.config` nicht.
    func save(_ key: String, _ value: JSONValue, in ext: FoundExtension) async {
        await ExtensionSettings.set(value, for: ext, key: key)
        if canToggle(ext), isEnabled(ext.name) { manager.reload(ext.name) }
        await load()
    }

    /// Sofort, wenn die letzte Meldung lang genug her ist, sonst einmal gesammelt am Ende der Wartezeit.
    private func changed() {
        guard !pending else { return }
        let wait = Self.coalesce - Date().timeIntervalSince(lastPublish)
        guard wait > 0 else { return publish() }
        pending = true
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            self?.pending = false
            self?.publish()
        }
    }

    private func publish() {
        lastPublish = Date()
        // Verschwundene Extension am Ende der Liste: Auswahl rückt nach.
        if selected >= items.count, !items.isEmpty { select(items.count - 1) }
        objectWillChange.send()
    }
}
