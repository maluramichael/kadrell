import SwiftUI

/// Seiten der Einstellungen. Unter „Harnesses“ eine Seite je Agent-CLI, Konten gehören zu Claude.
enum SettingsPage: String, CaseIterable {
    case general, appearance, hotkeys, claude, accounts, pi

    var title: String {
        switch self {
        case .general: String(localized: "Allgemein", bundle: Bundle.app)
        case .appearance: String(localized: "Darstellung", bundle: Bundle.app)
        case .hotkeys: String(localized: "Tastenkürzel", bundle: Bundle.app)
        case .claude: AgentKind.claude.displayName
        case .accounts: String(localized: "Konten", bundle: Bundle.app)
        case .pi: AgentKind.pi.displayName
        }
    }

    /// Seite mit den Start-Flags eines Harness.
    init(agent: AgentKind) {
        switch agent {
        case .claude: self = .claude
        case .pi: self = .pi
        }
    }

    /// Unterseite eines Harness, in der Leiste eingerückt.
    var indented: Bool { self == .accounts }
    /// Erste Harness-Seite, darüber steht die Zwischenüberschrift.
    var startsHarnesses: Bool { self == .claude }
}

@Observable
@MainActor
final class SettingsModel {
    /// Wird bei jeder Änderung hochgezählt. Die Bindings lesen ihn mit, damit SwiftUI nach dem Schreiben neu zeichnet.
    private var revision = 0
    /// Offene Seite, beim nächsten Öffnen wieder dieselbe.
    var page = SettingsPage(rawValue: Profile.defaults.string(forKey: "settings.page") ?? "") ?? .general {
        didSet { Profile.defaults.set(page.rawValue, forKey: "settings.page") }
    }
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
    var piBinary = Settings.piBinary {
        didSet { Settings.piBinary = piBinary.trimmingCharacters(in: .whitespacesAndNewlines); changed() }
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

    /// Login-Umgebung und gefundene Binarys. Von der App gesetzt; nil in Tests.
    @ObservationIgnored var cli: ClaudeCLI? { didSet { reloadPiModels() } }
    /// Modelle aus `pi --list-models`, nil = wird geladen.
    private(set) var piModels: [String]?
    /// Pis eigener Default aus `settings.json`, für „Standard (…)“.
    private(set) var piDefaultModel: String?
    /// Pfad, mit dem Pi gestartet würde, nil = nicht gefunden.
    var piResolvedBinary: String? {
        _ = revision
        return cli?.binary(for: .pi).flatMap { FileManager.default.isExecutableFile(atPath: $0) ? $0 : nil }
    }

    func reloadPiModels() {
        guard let cli else { return }
        piDefaultModel = PiCLI.defaultModel(environment: cli.environment)
        guard let binary = piResolvedBinary else { piModels = []; return }
        piModels = nil
        Task { [weak self] in self?.piModels = await PiCLI.listModels(binary: binary, environment: cli.environment) }
    }

    /// Sprache der Dialoge. Liest `revision` mit: nach der Umstellung zeichnet SwiftUI sofort in der neuen Sprache.
    var locale: Locale { _ = revision; return Localization.locale }

    /// Jede Änderung gilt sofort, ohne Speichern: das Binding schreibt direkt in `Settings`.
    func binding<T>(_ key: ReferenceWritableKeyPath<Settings.Type, T>) -> Binding<T> { binding(Settings.self, key) }

    func binding<Root, T>(_ root: Root, _ key: ReferenceWritableKeyPath<Root, T>) -> Binding<T> {
        Binding(get: { _ = self.revision; return root[keyPath: key] },
                set: { root[keyPath: key] = $0; self.changed() })
    }

    /// ⌘↑ / ⌘↓ und Pfeile in der Leiste: vorige oder nächste Seite, am Rand bleibt sie stehen.
    func movePage(_ d: Int) {
        let all = SettingsPage.allCases
        guard let i = all.firstIndex(of: page) else { return }
        page = all[max(0, min(all.count - 1, i + d))]
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
