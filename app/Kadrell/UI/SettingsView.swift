import SwiftUI

@Observable
@MainActor
final class SettingsModel {
    /// Wird bei jeder Änderung hochgezählt. Die Bindings lesen ihn mit, damit SwiftUI nach dem Schreiben neu zeichnet.
    private var revision = 0
    /// Textfelder halten ungetrimmte Entwürfe, sonst schluckt das Trimmen beim Tippen jedes Leerzeichen.
    var startFolder = Settings.startFolder {
        didSet {
            let p = startFolder.trimmingCharacters(in: .whitespacesAndNewlines)
            if !p.isEmpty { Settings.startFolder = p }
            changed()
        }
    }
    var editorCommand = Settings.editorCommand {
        didSet { Settings.editorCommand = editorCommand.trimmingCharacters(in: .whitespacesAndNewlines); changed() }
    }
    var hotkeys: [HotkeyAction: Hotkey] {
        get { _ = revision; return Hotkeys.current }
        set { Hotkeys.current = newValue; changed() }
    }
    @ObservationIgnored lazy var fonts: [(name: String, display: String)] = {
        let list = Settings.monospaceFonts, current = Settings.terminalFontName
        return list.contains { $0.name == current } ? list : [(current, current)] + list
    }()
    /// Aktion, deren Kürzel gerade aufgenommen wird.
    var recording: HotkeyAction?
    /// Nach jeder Änderung: die App zieht Menü, Aussehen und Sessions nach.
    @ObservationIgnored var onApply: (() -> Void)?
    @ObservationIgnored var onClose: (() -> Void)?
    @ObservationIgnored private var monitor: Any?

    /// Account-Verwaltung (Global-Swap). Von der App gesetzt; nil im reinen SwiftUI-Preview.
    /// Stored statt computed, damit SwiftUI die Liste nach dem Hinzufügen/Wechseln sicher neu zeichnet.
    @ObservationIgnored var accounts: AccountService? { didSet { reloadAccounts(); loadCurrentEmail() } }
    private(set) var accountList: [Account] = []
    private(set) var activeAccountId: String?
    /// E-Mail des gerade angemeldeten Accounts, damit am Knopf steht, wer hinzugefügt würde. Asynchron nachgeladen.
    private(set) var currentEmail: String?
    private func reloadAccounts() { accountList = accounts?.accounts ?? []; activeAccountId = accounts?.activeId }
    private func loadCurrentEmail() {
        Task { [weak self] in
            guard let token = await UsageService.token() else { return }
            self?.currentEmail = await AccountService.profileEmail(token: token)
        }
    }
    func addAccount() { Task { [weak self] in _ = await self?.accounts?.addCurrent(); self?.reloadAccounts() } }
    func switchAccount(_ id: String) { Task { [weak self] in _ = await self?.accounts?.switchTo(id); self?.reloadAccounts() } }
    func removeAccount(_ id: String) { Task { [weak self] in await self?.accounts?.remove(id); self?.reloadAccounts() } }

    /// Sprache der Dialoge. Liest `revision` mit: nach der Umstellung zeichnet SwiftUI sofort in der neuen Sprache.
    var locale: Locale { _ = revision; return Localization.locale }

    /// Jede Änderung gilt sofort, ohne Speichern: das Binding schreibt direkt in `Settings`.
    func binding<T>(_ key: ReferenceWritableKeyPath<Settings.Type, T>) -> Binding<T> { binding(Settings.self, key) }

    func binding<Root, T>(_ root: Root, _ key: ReferenceWritableKeyPath<Root, T>) -> Binding<T> {
        Binding(get: { _ = self.revision; return root[keyPath: key] },
                set: { root[keyPath: key] = $0; self.changed() })
    }

    /// Erst anwenden, dann hochzählen: SwiftUI zeichnet beim Hochzählen neu und liest dabei Theme und UI-Größe, die `onApply` setzt.
    private func changed() {
        Task { @MainActor [weak self] in
            self?.onApply?()
            self?.revision += 1
        }
    }

    /// Nächster Tastendruck wird das Kürzel. Esc bricht ab, ⌫ entfernt es. Läuft vor OverlayPanel und Terminal.
    func record(_ a: HotkeyAction) {
        stopRecording()
        recording = a
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, let a = self.recording else { return event }
            let plain = event.modifierFlags.intersection(Hotkey.modMask).isEmpty
            if plain, event.keyCode == KeyCode.escape { self.stopRecording(); return nil }
            if plain, event.keyCode == KeyCode.delete { self.hotkeys[a] = nil; self.stopRecording(); return nil }
            guard let k = Hotkey(event: event), k.isUsable else { NSSound.beep(); return nil }
            for (other, v) in self.hotkeys where v == k { self.hotkeys[other] = nil }   // doppelt vergeben geht nicht
            self.hotkeys[a] = k
            self.stopRecording()
            return nil
        }
    }

    func stopRecording() {
        if let m = monitor { NSEvent.removeMonitor(m) }
        monitor = nil
        recording = nil
    }
}

/// ⌘, Einstellungen: füllt das Fenster bis auf einen schmalen Rand, Abschnitte in einer oder zwei Spalten.
/// Änderungen gelten sofort, ⏎ und Esc schließen.
struct SettingsView: View {
    @Bindable var model: SettingsModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("EINSTELLUNGEN").capsLabel()
                Spacer()
                Text("Kadrell v\(Settings.version)").font(Theme.ui(11)).foregroundStyle(Theme.mutedColor)
            }
            .padding(.horizontal, 16).padding(.top, 12)
            SeamLine().padding(.top, 12)
            SettingsColumns { sections }
            Color.clear.frame(height: 12)
            DialogFoot(hint: String(localized: "Änderungen gelten sofort · Esc schließt", bundle: Bundle.app), button: String(localized: "Fertig", bundle: Bundle.app)) { model.stopRecording(); model.onClose?() }
        }
        // Im Overlay so breit wie das Panel; allein (Tests, Vorschau) in der früheren festen Breite.
        .frame(idealWidth: 900 * Theme.scale, maxWidth: .infinity, alignment: .leading)
        .environment(\.locale, model.locale)
        .onDisappear { model.stopRecording() }
    }

    /// Alle Abschnitte in Lese- und Tab-Reihenfolge. Ein neuer Abschnitt ist eine eigene `SettingsBlock`-View und
    /// bekommt hier eine Zeile; die Spalten verteilen sich von selbst neu.
    @ViewBuilder private var sections: some View {
        SessionsSettings(model: model)
        ClaudeSettings(model: model)
        AccountsSettings(model: model)
        AppearanceSettings(model: model)
        CustomizeSettings(model: model)
        TreeSettings(model: model)
        UpdatesSettings(model: model)
        AutoModeSettings(model: model)
        AsksSettings(model: model)
        OllamaSettings()
        LlamaCppSettings()
        LocalFeatureSettings()
        HotkeySettings(model: model, part: .navigation)
        HotkeySettings(model: model, part: .tiles)
    }
}

