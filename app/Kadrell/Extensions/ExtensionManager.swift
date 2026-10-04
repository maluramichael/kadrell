import Foundation

extension ExtensionState {
    /// Klartext für Log und Dialog.
    var label: String {
        switch self {
        case .off: String(localized: "Aus", bundle: Bundle.app)
        case .starting: String(localized: "Startet", bundle: Bundle.app)
        case .running: String(localized: "Läuft", bundle: Bundle.app)
        case .reloading: String(localized: "Lädt neu", bundle: Bundle.app)
        case .failed(let reason): String(localized: "Fehler: \(reason)", bundle: Bundle.app)
        }
    }
}

/// Alle Extensions eines Profils: welche an sind, ihre Prozesse, Panels, Statuseinträge und Logs. Je Extension
/// entscheidet ein `ExtensionSupervisor`, hier werden seine Aktionen ausgeführt und Events, Steuerbefehle
/// (`kadrell.run`) und Dateiänderungen weitergereicht.
@MainActor
final class ExtensionManager {
    static let maxLog = 200
    static let reloadDelay: TimeInterval = 0.3

    private(set) var found: [FoundExtension] = []
    var onChange: () -> Void = {}

    private let environment: [String: String]
    private let control: ([String]) async -> ControlResponse
    private let sessions: () -> [JSONValue]
    let catalogDir: URL
    private var watcher: FolderWatcher?
    private var supervisors: [String: ExtensionSupervisor] = [:]
    /// Aktueller Prozess je Extension, eingetragen bis zu seinem Ende, auch während `stop()`.
    private var procs: [String: ExtensionProcess] = [:]
    /// Abgelöste Prozesse, die noch sauber enden dürfen; was sie melden, zählt nicht mehr.
    private var retired: [ExtensionProcess] = []
    private var restarts: [String: Task<Void, Never>] = [:]
    /// Zählt Starts: ein Start, der noch auf seine Einstellungen wartet, verfällt, sobald ein neuerer begonnen hat.
    private var launches: [String: Int] = [:]
    private var trees: [String: PanelTree] = [:]
    private var statuses: [String: (text: String, color: ThemeColor?, action: String?)] = [:]
    private var logs: [String: [String]] = [:]
    /// Änderungszeiten der Lua-Dateien und des Manifests je Extension, um nach einer Dateimeldung die geänderten zu finden.
    private var stamps: [String: [String: Date]] = [:]
    /// `app.ready` ist schon gelaufen: jeder später gestartete Prozess bekommt es direkt nach `hello`.
    private var appReady = false
    /// Letztes `session.focus`: ein später gestarteter Prozess bekommt es nach `app.ready`.
    private var focus: JSONValue?

    init(environment: [String: String], control: @escaping ([String]) async -> ControlResponse,
         sessions: @escaping () -> [JSONValue], catalogDir: URL = ExtensionCatalog.dir) {
        self.environment = environment
        self.control = control
        self.sessions = sessions
        self.catalogDir = catalogDir
    }

    var panels: [(name: String, tree: PanelTree)] { found.compactMap { f in trees[f.name].map { (f.name, $0) } } }

    var statusItems: [(name: String, text: String, color: ThemeColor?, action: String?)] {
        found.compactMap { f in statuses[f.name].map { (f.name, $0.text, $0.color, $0.action) } }
    }

    func state(_ name: String) -> ExtensionState { supervisors[name]?.state ?? .off }

    func log(_ name: String) -> [String] { logs[name] ?? [] }

    func start() {
        watcher = FolderWatcher(catalogDir, latency: Self.reloadDelay) { [weak self] in self?.rescan() }
        rescan()
    }

    func setEnabled(_ name: String, _ on: Bool) {
        Settings.enabledExtensions = Settings.enabledExtensions.filter { $0 != name } + (on ? [name] : [])
        guard !on || found.contains(where: { $0.name == name && $0.problem == nil }) else { return }
        feed(name, on ? .enable : .disable)
    }

    func reload(_ name: String) { feed(name, .reload) }

    func emit(_ event: String, _ data: JSONValue) {
        if event == "app.ready" { appReady = true }
        if event == "session.focus" { focus = data }
        for (name, p) in procs where isLive(p, name) { p.send(.event(name: event, data: data)) }
    }

    func action(_ name: String, id: String) {
        guard let p = procs[name], isLive(p, name) else { return }
        p.send(.event(name: "ui.action", data: .object(["id": .string(id)])))
    }

    /// Beim Beenden: alle stoppen wie beim Abschalten, ohne die Auswahl in den Einstellungen anzufassen.
    func shutdownAll() {
        watcher?.stop()
        watcher = nil
        supervisors.keys.forEach { feed($0, .disable) }
    }

    // MARK: Katalog

    /// Neu einlesen: verschwundene stoppen und vergessen, geänderte eingeschaltete neu laden, fehlende starten.
    private func rescan() {
        let now = ExtensionCatalog.scan(catalogDir)
        let names = Set(now.map(\.name))
        for f in found where !names.contains(f.name) { vanish(f.name) }
        found = now
        for f in now {
            let stamp = Self.stamp(f.dir)
            let changed = stamps[f.name].map { $0 != stamp } ?? false
            stamps[f.name] = stamp
            reconcile(f, changed: changed)
        }
        onChange()
    }

    private func reconcile(_ f: FoundExtension, changed: Bool) {
        let wanted = f.problem == nil && Settings.enabledExtensions.contains(f.name)
        switch (wanted, state(f.name)) {
        case (false, .off): break
        case (false, _): feed(f.name, .disable)
        case (true, .off): feed(f.name, .enable)
        case (true, _): if changed { feed(f.name, .reload) }
        }
    }

    private func vanish(_ name: String) {
        feed(name, .disable)
        supervisors[name] = nil
        logs[name] = nil
        stamps[name] = nil
        launches[name] = nil
    }

    /// Nur Lua-Dateien und Manifest: schreibt eine Extension eigene Dateien in ihren Ordner, lädt sie nicht endlos neu.
    private static func stamp(_ dir: URL) -> [String: Date] {
        var out: [String: Date] = [:]
        for url in ExtensionCatalog.luaFiles(dir) + [dir.appendingPathComponent("kadrell.json")] {
            out[url.path] = (try? url.resourceValues(forKeys: [.contentModificationDateKey]))?.contentModificationDate
        }
        return out
    }

    // MARK: Zustandsmaschine

    private func feed(_ name: String, _ input: SupervisorInput) {
        switch input {
        case .enable, .reload, .disable: restarts.removeValue(forKey: name)?.cancel()
        default: break
        }
        var sup = supervisors[name] ?? ExtensionSupervisor()
        let before = sup.state
        let actions = sup.handle(input, now: Date())
        supervisors[name] = sup
        if sup.state != before { append(name, sup.state.label) }
        actions.forEach { perform($0, name) }
        onChange()
    }

    private func perform(_ action: SupervisorAction, _ name: String) {
        switch action {
        case .spawn: spawn(name)
        case .terminate:
            // Kein Prozess (Start wartet noch auf Einstellungen oder ist gescheitert): gilt als schon beendet.
            if let p = procs[name] { p.stop() } else { feed(name, .exited(expected: true, reason: "")) }
        case .scheduleRestart(let delay):
            restarts[name] = Task { [weak self] in
                try? await Task.sleep(for: .seconds(delay))
                if !Task.isCancelled { self?.feed(name, .restartDue) }
            }
        case .clearUI:
            trees[name] = nil
            statuses[name] = nil
        }
    }

    private func spawn(_ name: String) {
        guard let ext = found.first(where: { $0.name == name }) else { return }
        let n = launches[name, default: 0] + 1
        launches[name] = n
        Task {
            let config = await ExtensionSettings.values(for: ext)
            guard launches[name] == n, state(name) == .starting else { return }
            launch(ext, config: config)
        }
    }

    /// Prozess anlegen und starten im selben Durchlauf: dazwischen darf kein Terminal die Pipes erben.
    private func launch(_ ext: FoundExtension, config: [String: JSONValue]) {
        let name = ext.name
        retire(name)
        let p = ExtensionProcess(dir: ext.dir, environment: environment)
        p.onMessage = { [weak self, weak p] m in if let p { self?.received(m, from: p, name) } }
        p.onStderr = { [weak self] line in
            self?.append(name, line)
            self?.onChange()
        }
        p.onExit = { [weak self, weak p] expected, reason in if let p { self?.exited(p, name, expected: expected, reason: reason) } }
        procs[name] = p
        do {
            try p.start(hello: .hello(api: ExtensionManifest.supportedAPI, name: name, dir: ext.dir.path, storageDir: storageDir(name),
                                      config: config, locale: Bundle.app.preferredLocalizations.first ?? "en", sessions: sessions()))
        } catch {
            procs[name] = nil
            return feed(name, .exited(expected: false, reason: error.localizedDescription))
        }
        guard appReady else { return }
        p.send(.event(name: "app.ready", data: .object([:])))
        if let focus { p.send(.event(name: "session.focus", data: focus)) }
    }

    /// Den aktuellen Prozess ablösen: er stoppt sanft (`hard`: sofort SIGKILL), seine Meldungen zählen nicht mehr.
    private func retire(_ name: String, hard: Bool = false) {
        guard let p = procs.removeValue(forKey: name) else { return }
        retired.append(p)
        if hard { p.kill() } else { p.stop() }
    }

    private func exited(_ p: ExtensionProcess, _ name: String, expected: Bool, reason: String) {
        retired.removeAll { $0 === p }
        guard procs[name] === p else { return }
        procs[name] = nil
        feed(name, .exited(expected: expected, reason: reason))
    }

    /// Persistenz der Extension (`kadrell.storage`), pro Profil, überlebt Reload und Neustart.
    private func storageDir(_ name: String) -> String {
        let dir = Profile.directory.appendingPathComponent("extensions/\(name)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.path
    }

    // MARK: Nachrichten

    /// Ein Prozess, der gerade stoppt, ist nicht mehr live: sein spätes ready darf keinen neuen Start überholen.
    private func isLive(_ p: ExtensionProcess, _ name: String) -> Bool {
        procs[name] === p && !p.isStopping && (state(name) == .starting || state(name) == .running)
    }

    private func received(_ m: ExtensionMessage, from p: ExtensionProcess, _ name: String) {
        switch m {
        case .ready: if isLive(p, name) { feed(name, .ready) }
        case .pong: break
        case .log(let level, let text):
            append(name, level == "info" ? text : "[\(level)] \(text)")
            onChange()
        case .run(let id, let argv):
            // Jede Anfrage bekommt eine Antwort, sonst wartet die Coroutine in Lua ewig. Ein stoppender Prozess führt nichts mehr aus.
            let live = isLive(p, name)
            Task {
                let r = live ? await control(argv) : .fail("extension is stopping")
                p.send(.result(id: id, status: r.status, stdout: r.stdout, stderr: r.stderr))
            }
        case .panel(let v): if isLive(p, name) { setPanel(v, name) }
        case .status(let v): if isLive(p, name) { setStatus(v, name) }
        }
    }

    private func setPanel(_ v: JSONValue?, _ name: String) {
        guard let v else {
            trees[name] = nil
            return onChange()
        }
        switch PanelValidation.tree(v) {
        case .success(let result):
            var seen = Set<String>()
            for w in result.warnings where seen.insert(w).inserted { append(name, w) }
            trees[name] = result.0
            onChange()
        case .failure(.tooLarge(let count)):
            // Wie ein Absturz: Prozess sofort weg, Neustart nach Backoff.
            retire(name, hard: true)
            let reason = String(localized: "Panel zu groß (\(count) Knoten, höchstens \(PanelValidation.maxNodes))", bundle: Bundle.app)
            feed(name, .exited(expected: false, reason: reason))
        }
    }

    private func setStatus(_ v: JSONValue?, _ name: String) {
        let item = v.flatMap(PanelValidation.status)
        if v != nil, item == nil {
            append(name, String(localized: "Knoten „\("status")“ ohne Feld „\("text")“ übersprungen", bundle: Bundle.app))
        }
        statuses[name] = item
        onChange()
    }

    /// Ringpuffer: höchstens `maxLog` Zeilen zu je höchstens 500 Zeichen.
    private func append(_ name: String, _ line: String) {
        var log = logs[name, default: []]
        log.append(String(line.prefix(PanelValidation.maxText)))
        if log.count > Self.maxLog { log.removeFirst(log.count - Self.maxLog) }
        logs[name] = log
    }
}
