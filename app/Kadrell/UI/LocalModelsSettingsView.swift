import SwiftUI

// Abschnitte „Ollama“, „llama.cpp“ und „Modelle für Funktionen“ der Einstellungen (⌘,). Alles gilt sofort; ⏎ schließt
// wie überall die Einstellungen, deshalb prüfen Adressfelder und Suche kurz nach dem Tippen von selbst und Laden und
// Löschen sind Tasten (Tab, Leertaste).

/// Ollama: an/aus, Adresse, installierte Modelle, Laden mit Fortschritt, Suche auf ollama.com.
struct OllamaSettings: SettingsBlock {
    @State private var local = LocalModels.shared
    @State private var flags = LocalFlags()
    @State private var url = LocalModelSettings.ollamaURL
    @State private var pullName = ""
    @State private var query = ""
    @State private var debounce = Debounce()
    @State private var searchDebounce = Debounce()

    var body: some View {
        SettingsSection(String(localized: "Lokale Modelle: Ollama", bundle: Bundle.app),
                        note: String(localized: "gezeigt wird nur, was installiert ist", bundle: Bundle.app)) {
            setting(String(localized: "Ollama nutzen", bundle: Bundle.app)) {
                onOff(flags.binding({ LocalModelSettings.ollamaEnabled }, { LocalModelSettings.ollamaEnabled = $0; refresh(after: 0) }))
            }
            if flags.read(LocalModelSettings.ollamaEnabled) {
                setting(String(localized: "Adresse des Ollama-Dienstes", bundle: Bundle.app)) {
                    LocalTextField(placeholder: OllamaClient.defaultURL, text: $url)
                        .onChange(of: url) { _, v in LocalModelSettings.ollamaURL = v; refresh(after: 0.7) }
                }
                LocalStatusRow(provider: .ollama, hint: String(localized: "Ollama starten (App oder ollama serve), Kadrell bringt keinen eigenen Dienst mit", bundle: Bundle.app))
                LocalModelRows(provider: .ollama)
                pullRow
                LocalPullRows()
                setting(String(localized: "Auf ollama.com suchen (Webseite, keine offizielle Schnittstelle)", bundle: Bundle.app)) {
                    LocalTextField(placeholder: String(localized: "z. B. qwen3", bundle: Bundle.app), text: $query)
                        .onChange(of: query) { _, v in searchDebounce.run(after: 0.5) { await local.search(v) } }
                }
                searchRows
            }
        }
        .task { await local.refresh(.ollama) }
    }

    private func refresh(after delay: Double) {
        debounce.run(after: delay) { await local.refresh(.ollama) }
    }

    private var pullRow: some View {
        setting(String(localized: "Modell laden (Name aus der Bibliothek)", bundle: Bundle.app)) {
            HStack(spacing: 6) {
                LocalTextField(placeholder: "qwen3:0.6b", text: $pullName)
                smallKey(String(localized: "Laden", bundle: Bundle.app)) {
                    local.pull(pullName)
                    pullName = ""
                }
                .fixedSize()
                .disabled(pullName.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    private var searchRows: some View {
        ForEach(local.searchHits.prefix(8)) { hit in
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(hit.name).foregroundStyle(Theme.fgColor)
                    if !hit.summary.isEmpty { Text(hit.summary).font(Theme.ui(10.5)).foregroundStyle(Theme.mutedColor).lineLimit(1) }
                }
                Spacer(minLength: 12)
                // Größe wählen setzt nur den Namen ins Feld „Modell laden“: geladen wird erst mit „Laden“, nach Blick auf die Größe.
                ForEach(hit.tags.prefix(6), id: \.self) { tag in
                    smallKey(tag) { pullName = "\(hit.name):\(tag)" }
                        .help(String(localized: "\(hit.name):\(tag) ins Feld „Modell laden“", bundle: Bundle.app))
                }
                if hit.tags.isEmpty { smallKey(String(localized: "wählen", bundle: Bundle.app)) { pullName = hit.name } }
            }
            .font(Theme.ui(12)).padding(.horizontal, 10).padding(.vertical, 5)
        }
    }
}

/// llama.cpp: an/aus, `llama-server`, Modellordner oder Adresse eines laufenden Servers, installierte Modelle.
/// Laden bietet Kadrell hier nicht an (siehe `LlamaClient`), die Zeile sagt, wie Modelle dazukommen.
struct LlamaCppSettings: SettingsBlock {
    @State private var local = LocalModels.shared
    @State private var flags = LocalFlags()
    @State private var binary = LocalModelSettings.llamaBinary
    @State private var dir = LocalModelSettings.llamaModelsDir
    @State private var url = LocalModelSettings.llamaURL
    @State private var debounce = Debounce()

    var body: some View {
        SettingsSection(String(localized: "Lokale Modelle: llama.cpp", bundle: Bundle.app),
                        note: String(localized: "ohne Adresse startet Kadrell llama-server selbst, er endet mit Kadrell", bundle: Bundle.app)) {
            setting(String(localized: "llama.cpp nutzen", bundle: Bundle.app)) {
                onOff(flags.binding({ LocalModelSettings.llamaEnabled }, { LocalModelSettings.llamaEnabled = $0; refresh(after: 0) }))
            }
            if flags.read(LocalModelSettings.llamaEnabled) {
                setting(String(localized: "Pfad zu llama-server (leer = automatisch)", bundle: Bundle.app)) {
                    LocalTextField(placeholder: detected, text: $binary)
                        .onChange(of: binary) { _, v in LocalModelSettings.llamaBinary = v; refresh(after: 0.7) }
                }
                setting(String(localized: "Modellordner mit GGUF-Dateien (optional)", bundle: Bundle.app)) {
                    LocalTextField(placeholder: String(localized: "nur der Cache von llama.cpp", bundle: Bundle.app), text: $dir)
                        .onChange(of: dir) { _, v in LocalModelSettings.llamaModelsDir = v; refresh(after: 0.7) }
                }
                setting(String(localized: "Adresse eines laufenden llama-server (optional)", bundle: Bundle.app)) {
                    LocalTextField(placeholder: String(localized: "leer = Kadrell startet ihn", bundle: Bundle.app), text: $url)
                        .onChange(of: url) { _, v in LocalModelSettings.llamaURL = v; refresh(after: 0.7) }
                }
                LocalStatusRow(provider: .llamacpp, hint: String(localized: "llama.cpp installieren (brew install llama.cpp) oder den Pfad setzen", bundle: Bundle.app))
                LocalModelRows(provider: .llamacpp)
                Text(String(localized: "Neue Modelle: einmal im Terminal mit llama-server -hf <Repo> laden (landet im Cache) oder GGUF-Dateien in den Modellordner legen", bundle: Bundle.app))
                    .font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 10).padding(.vertical, 6)
            }
        }
        .task { await local.refresh(.llamacpp) }
    }

    private var detected: String {
        let path = LlamaServerProcess.autodetect(environment: local.environment)
        return path.isEmpty ? String(localized: "nicht gefunden", bundle: Bundle.app) : Theme.shortPath(path)
    }

    private func refresh(after delay: Double) {
        debounce.run(after: delay) { await local.refresh(.llamacpp) }
    }
}

/// Modelle für Funktionen: je Funktion an/aus und Modell aus den gerade installierten.
struct LocalFeatureSettings: SettingsBlock {
    @State private var local = LocalModels.shared
    @State private var flags = LocalFlags()

    var body: some View {
        SettingsSection(String(localized: "Modelle für Funktionen", bundle: Bundle.app),
                        note: String(localized: "je Funktion an/aus und Modell aus den vorhandenen", bundle: Bundle.app)) {
            ForEach(LocalFeature.allCases, id: \.self) { f in
                setting(f.title) {
                    onOff(flags.binding({ LocalModelSettings.featureEnabled(f) }, { LocalModelSettings.setFeatureEnabled(f, $0) }))
                }
                .help(f.summary)
                setting(String(localized: "Modell", bundle: Bundle.app)) {
                    menu(modelChoice(f), modelItems(f))
                }
                if f == .quickStart {
                    setting(String(localized: "Projektverweis aus dem ersten Prompt entfernen, wenn sicher", bundle: Bundle.app)) {
                        onOff(flags.binding({ LocalModelSettings.quickStartStripReference }, { LocalModelSettings.quickStartStripReference = $0 }))
                    }
                }
            }
        }
    }

    /// Auswahl eines Modells schaltet die Funktion gleich mit an; „kein Modell“ lässt den Schalter, wie er ist.
    private func modelChoice(_ f: LocalFeature) -> Binding<String> {
        flags.binding({ LocalModelSettings.featureModel(f)?.string ?? "" }, { v in
            let ref = LocalModelRef(string: v)
            LocalModelSettings.setFeatureModel(f, ref)
            if ref != nil { LocalModelSettings.setFeatureEnabled(f, true) }
        })
    }

    /// Nur vorhandene Modelle; ein gewähltes, das gerade fehlt (gelöscht, Provider aus), bleibt sichtbar markiert.
    private func modelItems(_ f: LocalFeature) -> [(String, String)] {
        var items = [("", String(localized: "kein Modell (nur Namenssuche)", bundle: Bundle.app))]
        items += local.available.map { ($0.ref.string, $0.ref.display) }
        if let current = LocalModelSettings.featureModel(f), !items.contains(where: { $0.0 == current.string }) {
            items.append((current.string, String(localized: "\(current.display) (nicht vorhanden)", bundle: Bundle.app)))
        }
        return items
    }
}

// MARK: Gemeinsame Zeilen

/// Status eines Providers mit Taste „Neu laden“; ein Fehler zeigt beim Überfahren, was zu tun ist.
private struct LocalStatusRow: SettingsBlock {
    let provider: LocalProviderKind
    let hint: String
    @State private var local = LocalModels.shared

    var body: some View {
        setting(String(localized: "Status", bundle: Bundle.app)) {
            HStack(spacing: 8) {
                switch local.status(provider) {
                case .off: Text(String(localized: "aus", bundle: Bundle.app)).foregroundStyle(Theme.mutedColor)
                case .checking: ProgressView().controlSize(.small); Text(String(localized: "prüft …", bundle: Bundle.app)).foregroundStyle(Theme.mutedColor)
                case .ready(let v): Text(v).foregroundStyle(Theme.accentTextColor).lineLimit(1).truncationMode(.middle).help(v)
                case .failed(let e): Text(e).foregroundStyle(Theme.errorTextColor).lineLimit(1).truncationMode(.tail).help(e + "\n" + hint)
                }
                Spacer(minLength: 4)
                smallKey(String(localized: "Neu laden", bundle: Bundle.app)) { Task { await local.refresh(provider) } }.fixedSize()
            }
        }
    }
}

/// Installierte Modelle eines Providers, löschbare mit zweistufiger Taste: der zweite Druck löscht.
private struct LocalModelRows: View {
    let provider: LocalProviderKind
    @State private var local = LocalModels.shared
    @State private var armed: String?

    var body: some View {
        let models = local.models(provider)
        if case .ready = local.status(provider), models.isEmpty {
            row { Text(String(localized: "Keine Modelle installiert", bundle: Bundle.app)).foregroundStyle(Theme.mutedColor) }
        }
        ForEach(models) { m in
            row {
                // Der Name hat Vorrang, die Angaben daneben kürzen zuerst (Größe steht vorn, sie zählt beim Aufräumen).
                Text(m.name).foregroundStyle(Theme.fgColor).lineLimit(1).truncationMode(.middle).layoutPriority(1)
                Spacer(minLength: 12)
                Text([m.sizeText, m.detail].filter { !$0.isEmpty }.joined(separator: " · ")).foregroundStyle(Theme.mutedColor).lineLimit(1)
                if m.canRemove, !m.downloading { deleteKey(m) }
            }
        }
        if let e = local.actionError {
            row { Text(e).font(Theme.ui(11)).foregroundStyle(Theme.errorTextColor) }
        }
    }

    private func row<C: View>(@ViewBuilder _ content: () -> C) -> some View {
        HStack(spacing: 10) { content() }.font(Theme.ui(12)).padding(.horizontal, 10).padding(.vertical, 5)
    }

    private func deleteKey(_ m: LocalModel) -> some View {
        let isArmed = armed == m.id
        return Button {
            if isArmed { armed = nil; Task { await local.delete(m) } } else { armed = m.id }
        } label: {
            Text(isArmed ? String(localized: "Wirklich löschen", bundle: Bundle.app) : String(localized: "Löschen", bundle: Bundle.app))
        }
        .buttonStyle(InstrumentKeyStyle(isArmed ? .destructive : .neutral, compact: true)).kbdFocusRing().fixedSize()
    }
}

/// Laufende und gescheiterte Downloads mit Fortschritt, Abbrechen bzw. OK.
private struct LocalPullRows: View {
    @State private var local = LocalModels.shared

    var body: some View {
        ForEach(local.pulls) { pull in
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 10) {
                    Text(pull.ref.name).foregroundStyle(Theme.fgColor).lineLimit(1)
                    Spacer(minLength: 12)
                    Text(pull.error ?? Self.progressText(pull.progress))
                        .foregroundStyle(pull.error != nil ? Theme.errorTextColor : Theme.mutedColor).lineLimit(1).truncationMode(.middle)
                    Button { local.cancelPull(pull.ref) } label: {
                        Text(pull.error != nil ? String(localized: "OK", bundle: Bundle.app) : String(localized: "Abbrechen", bundle: Bundle.app))
                    }
                    .buttonStyle(InstrumentKeyStyle(.neutral, compact: true)).kbdFocusRing()
                }
                if pull.error == nil {
                    if let f = pull.progress.fraction { ProgressView(value: f).tint(Theme.accentColor) } else { ProgressView().progressViewStyle(.linear).tint(Theme.accentColor) }
                }
            }
            .font(Theme.ui(12)).padding(.horizontal, 10).padding(.vertical, 5)
        }
    }

    static func progressText(_ p: PullProgress) -> String {
        guard let f = p.fraction else { return p.status }
        let done = ByteCountFormatter.string(fromByteCount: p.completed, countStyle: .file)
        let total = ByteCountFormatter.string(fromByteCount: p.total, countStyle: .file)
        return "\(Int(f * 100)) % · \(done) / \(total)"
    }
}

// MARK: Hilfen

/// Textfeld der rechten Spalte im Stil der Einstellungen, mit eigenem Fokusring.
private struct LocalTextField: View {
    let placeholder: String
    @Binding var text: String
    @FocusState private var focused: Bool

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain).foregroundStyle(Theme.fgColor)
            .focused($focused).instrumentWell(focused: focused)
    }
}

/// `LocalModelSettings` ist nicht beobachtbar: jede Änderung über diese Bindings zählt hoch und zeichnet neu.
@Observable
@MainActor
private final class LocalFlags {
    private var tick = 0

    func read<T>(_ value: @autoclosure () -> T) -> T { _ = tick; return value() }

    func binding<T>(_ get: @escaping () -> T, _ set: @escaping (T) -> Void) -> Binding<T> {
        Binding(get: { self.read(get()) }, set: { set($0); self.tick += 1 })
    }
}

/// Kurz nach dem letzten Tastendruck handeln, nicht bei jedem Zeichen.
@MainActor
private final class Debounce {
    private var task: Task<Void, Never>?

    func run(after seconds: Double, _ work: @escaping @MainActor () async -> Void) {
        task?.cancel()
        task = Task {
            try? await Task.sleep(for: .seconds(seconds))
            guard !Task.isCancelled else { return }
            await work()
        }
    }
}
