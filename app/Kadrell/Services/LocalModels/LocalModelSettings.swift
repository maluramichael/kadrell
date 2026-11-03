import Foundation

/// Funktionen von Kadrell, die ein lokales Modell nutzen können. Jede ist für sich an- und abschaltbar und hat ihr
/// eigenes Modell. Neue Funktionen: Fall ergänzen, Titel und Beschreibung dazu, fertig; Einstellungen und
/// `LocalModels.complete(for:…)` sind generisch.
enum LocalFeature: String, CaseIterable, Sendable {
    case quickStart, piSession

    var title: String {
        switch self {
        case .quickStart: String(localized: "Schnellstart: Idee rein, Session raus", bundle: Bundle.app)
        case .piSession: String(localized: "Pi-Sessions", bundle: Bundle.app)
        }
    }

    var summary: String {
        switch self {
        case .quickStart: String(localized: "ordnet die Idee einem Projekt zu, wenn der Name im Text nicht eindeutig ist", bundle: Bundle.app)
        case .piSession: String(localized: "Modell für Pi statt Claude, im Schnellstart mit Tab umschalten", bundle: Bundle.app)
        }
    }

    /// Eintrag „kein Modell“ in der Auswahl: was die Funktion dann tut.
    var noModel: String {
        switch self {
        case .quickStart: String(localized: "kein Modell (nur Namenssuche)", bundle: Bundle.app)
        case .piSession: String(localized: "kein Modell (nur Claude)", bundle: Bundle.app)
        }
    }

    /// Wie lange die Funktion auf das Modell wartet, bevor sie ohne Modell weitermacht. Pi-Sessions fragen nie selbst.
    var timeout: TimeInterval {
        switch self {
        case .quickStart: 12
        case .piSession: 0
        }
    }
}

/// Einstellungen der lokalen Modelle, im Profil wie alle anderen (`Profile.defaults`).
enum LocalModelSettings {
    private static func value<T>(_ key: String, _ fallback: T) -> T { Profile.defaults.object(forKey: key) as? T ?? fallback }
    private static func store(_ key: String, _ value: Any?) { Profile.defaults.set(value, forKey: key) }

    /// Ollama nutzen, Default an: kostet nichts, solange keine Funktion es braucht (nur Anfragen an localhost).
    static var ollamaEnabled: Bool { get { value("local.ollama.enabled", true) } set { store("local.ollama.enabled", newValue) } }
    /// Adresse des laufenden Ollama-Dienstes.
    static var ollamaURL: String { get { value("local.ollama.url", OllamaClient.defaultURL) } set { store("local.ollama.url", newValue) } }
    /// llama.cpp nutzen, Default aus: startet bei Bedarf einen eigenen `llama-server`.
    static var llamaEnabled: Bool { get { value("local.llama.enabled", false) } set { store("local.llama.enabled", newValue) } }
    /// Pfad zu `llama-server`, "" = automatisch (PATH, Homebrew).
    static var llamaBinary: String { get { value("local.llama.binary", "") } set { store("local.llama.binary", newValue) } }
    /// Zusätzlicher Ordner mit GGUF-Dateien (`--models-dir`), "" = nur der Cache von llama.cpp.
    static var llamaModelsDir: String { get { value("local.llama.modelsDir", "") } set { store("local.llama.modelsDir", newValue) } }
    /// Adresse eines schon laufenden `llama-server`. "" = Kadrell startet ihn selbst als Kindprozess.
    static var llamaURL: String { get { value("local.llama.url", "") } set { store("local.llama.url", newValue) } }

    static func enabled(_ provider: LocalProviderKind) -> Bool { provider == .ollama ? ollamaEnabled : llamaEnabled }

    static func featureEnabled(_ f: LocalFeature) -> Bool { value("local.feature.\(f.rawValue).enabled", false) }
    static func setFeatureEnabled(_ f: LocalFeature, _ on: Bool) { store("local.feature.\(f.rawValue).enabled", on) }
    static func featureModel(_ f: LocalFeature) -> LocalModelRef? { LocalModelRef(string: featureModelSetting(f)) }
    static func setFeatureModel(_ f: LocalFeature, _ ref: LocalModelRef?) { setFeatureModelSetting(f, ref?.string ?? "") }
    /// Gespeicherter Wert der Modellauswahl: `provider|name`, "" = kein Modell, bei Pi auch `PiModel.piDefaultSetting`.
    static func featureModelSetting(_ f: LocalFeature) -> String { value("local.feature.\(f.rawValue).model", "") }
    static func setFeatureModelSetting(_ f: LocalFeature, _ v: String) { store("local.feature.\(f.rawValue).model", v) }

    /// Womit Pi-Sessions starten, nil = Funktion aus oder kein Modell gewählt.
    @MainActor static var piModel: PiModel? {
        guard featureEnabled(.piSession) else { return nil }
        if featureModelSetting(.piSession) == PiModel.piDefaultSetting { return .piDefault }
        return LocalModels.shared.activeModel(for: .piSession).map(PiModel.local)
    }

    /// Schnellstart: das Modell darf den Verweis auf das Projekt aus dem ersten Prompt nehmen, wenn es sicher ist.
    static var quickStartStripReference: Bool {
        get { value("local.quickStart.stripReference", true) } set { store("local.quickStart.stripReference", newValue) }
    }
}
