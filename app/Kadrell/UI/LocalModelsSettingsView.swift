import SwiftUI

/// Abschnitt „Lokale Modelle“ und „Modelle für Funktionen“ in den Einstellungen (⌘,). Eigene View, damit die
/// Einstellungen sie nur an einer Stelle einhängen. Alles gilt sofort; ⏎ schließt wie überall die Einstellungen,
/// deshalb prüfen Adressfelder und Suche kurz nach dem Tippen von selbst und Laden/Löschen sind Tasten (Tab, Leertaste).
struct LocalModelsSettingsSection: View {
    @State private var local = LocalModels.shared
    /// Schreibzähler: `LocalModelSettings` ist nicht beobachtbar, jede Änderung zählt hoch und zeichnet neu.
    @State private var tick = 0
    @State private var ollamaURL = LocalModelSettings.ollamaURL
    @State private var llamaBinary = LocalModelSettings.llamaBinary
    @State private var llamaDir = LocalModelSettings.llamaModelsDir
    @State private var llamaURL = LocalModelSettings.llamaURL
    @State private var pullName: [LocalProviderKind: String] = [:]
    @State private var query = ""
    /// Modell, dessen Löschen-Taste scharf ist: der zweite Druck löscht.
    @State private var armed: String?
    @State private var debounce: Task<Void, Never>?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            SettingsKit.heading(String(localized: "Lokale Modelle", bundle: Bundle.app),
                                note: String(localized: "Ollama und llama.cpp · gezeigt wird nur, was installiert ist", bundle: Bundle.app))
            ollamaTable
            SettingsKit.heading(String(localized: "llama.cpp", bundle: Bundle.app),
                                note: String(localized: "Kadrell startet llama-server selbst, wenn keine Adresse gesetzt ist; er endet mit Kadrell", bundle: Bundle.app))
            llamaTable
            SettingsKit.heading(String(localized: "Modelle für Funktionen", bundle: Bundle.app),
                                note: String(localized: "je Funktion an/aus und Modell aus den vorhandenen", bundle: Bundle.app))
            featureTable
        }
        .task { await local.refreshAll() }
    }

    // MARK: Ollama

    private var ollamaTable: some View {
        SettingsKit.table {
            SettingsKit.row(String(localized: "Ollama nutzen", bundle: Bundle.app)) {
                SettingsKit.onOff(flag({ LocalModelSettings.ollamaEnabled }, { LocalModelSettings.ollamaEnabled = $0; refreshSoon(.ollama, delay: 0) }))
            }
            if LocalModelSettings.ollamaEnabled {
                SettingsKit.row(String(localized: "Adresse des Ollama-Dienstes", bundle: Bundle.app)) {
                    SettingsTextField(placeholder: OllamaClient.defaultURL, text: $ollamaURL)
                        .onChange(of: ollamaURL) { _, v in LocalModelSettings.ollamaURL = v; refreshSoon(.ollama) }
                }
                statusRow(.ollama, hint: String(localized: "Ollama starten (App oder ollama serve), Kadrell bringt keinen eigenen Dienst mit", bundle: Bundle.app))
                modelRows(.ollama)
                pullRow(.ollama, placeholder: "qwen3:0.6b")
                pullRows(.ollama)
                SettingsKit.row(String(localized: "Auf ollama.com suchen (Webseite, keine offizielle Schnittstelle)", bundle: Bundle.app)) {
                    SettingsTextField(placeholder: String(localized: "z. B. qwen3", bundle: Bundle.app), text: $query)
                        .onChange(of: query) { _, v in searchSoon(v) }
                }
                searchRows
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
                // Größe wählen setzt nur den Namen ins Feld darüber: geladen wird erst mit „Laden“, nach Blick auf die Größe.
                ForEach(hit.tags.prefix(6), id: \.self) { tag in
                    SettingsKit.key(tag) { pullName[.ollama] = "\(hit.name):\(tag)" }
                        .help(String(localized: "\(hit.name):\(tag) ins Feld „Modell laden“", bundle: Bundle.app))
                }
                if hit.tags.isEmpty { SettingsKit.key(String(localized: "wählen", bundle: Bundle.app)) { pullName[.ollama] = hit.name } }
            }
            .font(Theme.ui(12)).padding(.horizontal, 10).padding(.vertical, 5)
        }
    }

    // MARK: llama.cpp

    private var llamaTable: some View {
        SettingsKit.table {
            SettingsKit.row(String(localized: "llama.cpp nutzen", bundle: Bundle.app)) {
                SettingsKit.onOff(flag({ LocalModelSettings.llamaEnabled }, { LocalModelSettings.llamaEnabled = $0; refreshSoon(.llamacpp, delay: 0) }))
            }
            if LocalModelSettings.llamaEnabled {
                SettingsKit.row(String(localized: "Pfad zu llama-server (leer = automatisch)", bundle: Bundle.app)) {
                    SettingsTextField(placeholder: LlamaServerProcess.autodetect(environment: local.environment).isEmpty
                                      ? String(localized: "nicht gefunden", bundle: Bundle.app) : Theme.shortPath(LlamaServerProcess.autodetect(environment: local.environment)),
                                      text: $llamaBinary)
                        .onChange(of: llamaBinary) { _, v in LocalModelSettings.llamaBinary = v; refreshSoon(.llamacpp) }
                }
                SettingsKit.row(String(localized: "Modellordner mit GGUF-Dateien (optional)", bundle: Bundle.app)) {
                    SettingsTextField(placeholder: String(localized: "nur der Cache von llama.cpp", bundle: Bundle.app), text: $llamaDir)
                        .onChange(of: llamaDir) { _, v in LocalModelSettings.llamaModelsDir = v; refreshSoon(.llamacpp) }
                }
                SettingsKit.row(String(localized: "Adresse eines laufenden llama-server (optional)", bundle: Bundle.app)) {
                    SettingsTextField(placeholder: String(localized: "leer = Kadrell startet ihn", bundle: Bundle.app), text: $llamaURL)
                        .onChange(of: llamaURL) { _, v in LocalModelSettings.llamaURL = v; refreshSoon(.llamacpp) }
                }
                statusRow(.llamacpp, hint: String(localized: "llama.cpp installieren (brew install llama.cpp) oder den Pfad setzen", bundle: Bundle.app))
                modelRows(.llamacpp)
                pullRow(.llamacpp, placeholder: "ggml-org/gemma-3-1b-it-GGUF:Q4_K_M")
                pullRows(.llamacpp)
            }
        }
    }

    // MARK: Gemeinsame Zeilen

    private func statusRow(_ p: LocalProviderKind, hint: String) -> some View {
        let s = local.status(p)
        return SettingsKit.row(String(localized: "Status", bundle: Bundle.app)) {
            HStack(spacing: 8) {
                switch s {
                case .off: Text(String(localized: "aus", bundle: Bundle.app)).foregroundStyle(Theme.mutedColor)
                case .checking: ProgressView().controlSize(.small); Text(String(localized: "prüft …", bundle: Bundle.app)).foregroundStyle(Theme.mutedColor)
                case .ready(let v): Text(v).foregroundStyle(Theme.accentTextColor).lineLimit(1).truncationMode(.middle)
                case .failed(let e): Text(e).foregroundStyle(Theme.errorTextColor).lineLimit(1).truncationMode(.tail).help(e + "\n" + hint)
                }
                Spacer(minLength: 4)
                SettingsKit.key(String(localized: "Neu laden", bundle: Bundle.app)) { Task { await local.refresh(p) } }
            }
        }
    }

    @ViewBuilder
    private func modelRows(_ p: LocalProviderKind) -> some View {
        let models = local.models(p)
        if case .ready = local.status(p), models.isEmpty {
            SettingsKit.row(String(localized: "Keine Modelle installiert", bundle: Bundle.app)) { EmptyView() }
        }
        ForEach(models) { m in
            HStack(spacing: 10) {
                Text(m.name).foregroundStyle(Theme.fgColor).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 12)
                Text([m.detail, m.sizeText].filter { !$0.isEmpty }.joined(separator: " · ")).foregroundStyle(Theme.mutedColor).lineLimit(1)
                if m.canRemove, !m.downloading { deleteKey(m) }
            }
            .font(Theme.ui(12)).padding(.horizontal, 10).padding(.vertical, 5)
        }
        if let e = local.actionError {
            Text(e).font(Theme.ui(11)).foregroundStyle(Theme.errorTextColor).padding(.horizontal, 10).padding(.vertical, 5)
        }
    }

    private func deleteKey(_ m: LocalModel) -> some View {
        let isArmed = armed == m.id
        return Button {
            if isArmed { armed = nil; Task { await local.delete(m) } } else { armed = m.id }
        } label: {
            Text(isArmed ? String(localized: "Wirklich löschen", bundle: Bundle.app) : String(localized: "Löschen", bundle: Bundle.app))
        }
        .buttonStyle(InstrumentKeyStyle(isArmed ? .destructive : .neutral, compact: true)).kbdFocusRing()
    }

    private func pullRow(_ p: LocalProviderKind, placeholder: String) -> some View {
        let name = Binding(get: { pullName[p] ?? "" }, set: { pullName[p] = $0 })
        return SettingsKit.row(p == .ollama ? String(localized: "Modell laden (Name aus der Bibliothek)", bundle: Bundle.app)
                                            : String(localized: "Modell von Hugging Face laden", bundle: Bundle.app)) {
            HStack(spacing: 6) {
                SettingsTextField(placeholder: placeholder, text: name)
                SettingsKit.key(String(localized: "Laden", bundle: Bundle.app)) {
                    local.pull(p, name.wrappedValue)
                    pullName[p] = ""
                }
                .disabled(name.wrappedValue.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
    }

    private func pullRows(_ p: LocalProviderKind) -> some View {
        ForEach(local.pulls.filter { $0.ref.provider == p }) { pull in
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 10) {
                    Text(pull.ref.name).foregroundStyle(Theme.fgColor).lineLimit(1)
                    Spacer(minLength: 12)
                    Text(pull.error ?? Self.progressText(pull.progress))
                        .foregroundStyle(pull.error != nil ? Theme.errorTextColor : Theme.mutedColor).lineLimit(1).truncationMode(.middle)
                    SettingsKit.key(pull.error != nil ? String(localized: "OK", bundle: Bundle.app) : String(localized: "Abbrechen", bundle: Bundle.app)) {
                        local.cancelPull(pull.ref)
                    }
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

    // MARK: Funktionen

    private var featureTable: some View {
        SettingsKit.table {
            ForEach(LocalFeature.allCases, id: \.self) { f in
                SettingsKit.row(f.title, note: f.summary) {
                    SettingsKit.onOff(flag({ LocalModelSettings.featureEnabled(f) }, { LocalModelSettings.setFeatureEnabled(f, $0) }))
                }
                SettingsKit.row(String(localized: "Modell", bundle: Bundle.app)) {
                    SettingsKit.menu(modelChoice(f), modelItems(f))
                }
                if f == .quickStart {
                    SettingsKit.row(String(localized: "Projektverweis aus dem ersten Prompt entfernen, wenn sicher", bundle: Bundle.app)) {
                        SettingsKit.onOff(flag({ LocalModelSettings.quickStartStripReference }, { LocalModelSettings.quickStartStripReference = $0 }))
                    }
                }
            }
        }
    }

    /// Auswahl eines Modells schaltet die Funktion gleich mit an; „kein Modell“ lässt den Schalter, wie er ist.
    private func modelChoice(_ f: LocalFeature) -> Binding<String> {
        Binding(get: { _ = tick; return LocalModelSettings.featureModel(f)?.string ?? "" },
                set: { v in
                    let ref = LocalModelRef(string: v)
                    LocalModelSettings.setFeatureModel(f, ref)
                    if ref != nil { LocalModelSettings.setFeatureEnabled(f, true) }
                    tick += 1
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

    // MARK: Hilfen

    private func flag(_ get: @escaping () -> Bool, _ set: @escaping (Bool) -> Void) -> Binding<Bool> {
        Binding(get: { _ = tick; return get() }, set: { set($0); tick += 1 })
    }

    /// Kurz nach dem letzten Tastendruck neu prüfen, nicht bei jedem Zeichen.
    private func refreshSoon(_ p: LocalProviderKind, delay: Double = 0.7) {
        debounce?.cancel()
        debounce = Task {
            try? await Task.sleep(for: .seconds(delay))
            guard !Task.isCancelled else { return }
            await local.refresh(p)
        }
    }

    private func searchSoon(_ q: String) {
        debounce?.cancel()
        debounce = Task {
            try? await Task.sleep(for: .seconds(0.5))
            guard !Task.isCancelled else { return }
            await local.search(q)
        }
    }
}

/// Textfeld der rechten Spalte im Stil der Einstellungen, mit eigenem Fokusring.
private struct SettingsTextField: View {
    let placeholder: String
    @Binding var text: String
    @FocusState private var focused: Bool

    var body: some View {
        TextField(placeholder, text: $text)
            .textFieldStyle(.plain).foregroundStyle(Theme.fgColor)
            .focused($focused).instrumentWell(focused: focused)
    }
}

/// Bausteine im Look der Einstellungen (Überschrift, Tabelle mit Fugen, Zeile mit rechter Spalte, Dropdown).
/// Dieselben Maße wie in `SettingsView`, damit der Abschnitt nahtlos zwischen den anderen steht.
@MainActor
enum SettingsKit {
    static var controlWidth: CGFloat { 300 * Theme.scale }

    static func heading(_ s: String, note: String = "") -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(s.uppercased()).sectionTitle()
            if !note.isEmpty { Text(note).font(Theme.ui(11)).foregroundStyle(Theme.mutedColor).lineLimit(1) }
        }
        .padding(.horizontal, 16).padding(.top, 18).padding(.bottom, 6)
    }

    static func table<C: View>(@ViewBuilder _ rows: () -> C) -> some View {
        VStack(spacing: 0) {
            SwiftUI.Group(subviews: rows()) { subviews in
                ForEach(subviews.indices, id: \.self) { i in
                    if i > 0 { SeamLine() }
                    subviews[i]
                }
            }
        }
        .instrumentGroup()
        .padding(.horizontal, 16)
    }

    static func row<C: View>(_ title: String, note: String = "", @ViewBuilder _ control: () -> C) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 1) {
                Text(title).foregroundStyle(Theme.fgColor).lineLimit(1)
                if !note.isEmpty { Text(note).font(Theme.ui(10.5)).foregroundStyle(Theme.mutedColor).lineLimit(1) }
            }
            Spacer(minLength: 12)
            control().frame(width: controlWidth)
        }
        .font(Theme.ui(12))
        .padding(.horizontal, 10).padding(.vertical, 5)
    }

    static func menu<T: Hashable>(_ selection: Binding<T>, _ items: [(T, String)]) -> some View {
        Menu {
            Picker("", selection: selection) {
                ForEach(items, id: \.0) { Text($0.1).tag($0.0) }
            }
            .pickerStyle(.inline).labelsHidden()
        } label: {
            HStack(spacing: 4) {
                Text(items.first { $0.0 == selection.wrappedValue }?.1 ?? "–").lineLimit(1).foregroundStyle(Theme.fgColor)
                Spacer(minLength: 4)
                Image(systemName: "chevron.up.chevron.down").font(.system(size: 9 * Theme.scale)).foregroundStyle(Theme.mutedColor)
            }
            .instrumentWell()
            .frame(width: controlWidth).contentShape(Rectangle())
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).kbdFocusRing()
        .frame(width: controlWidth)
    }

    static func onOff(_ value: Binding<Bool>) -> some View {
        menu(value, [(true, String(localized: "an", bundle: Bundle.app)), (false, String(localized: "aus", bundle: Bundle.app))])
    }

    static func key(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Text(title) }.buttonStyle(InstrumentKeyStyle(.neutral, compact: true)).kbdFocusRing()
    }
}
