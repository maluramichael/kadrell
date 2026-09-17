import AppKit
import SwiftTerm
import os

/// Terminal einer laufenden Session.
@MainActor
final class KadrellTerminalView: LocalProcessTerminalView {
    var onExit: (() -> Void)?

    /// Sync: Taste an ein Terminal ohne Tastatur. `keyDown` taugt dafür nur bei Funktionstasten (Pfeile, F-Tasten, Pos1/Ende),
    /// die SwiftTerm selbst kodiert. Text, ⏎, ⌫, Esc und ⌃-Tasten laufen dort über das Eingabesystem von macOS, und das liefert
    /// immer an das Terminal mit der Tastatur. Deshalb gehen deren Zeichen direkt per `insertText` raus.
    /// ponytail: im Kitty-Tastaturmodus kommen Esc und ⌃-Tasten so als klassische Bytes an, und tote Tasten (^ e) nur als Grundzeichen.
    static func forward(_ event: NSEvent, to t: TerminalView) {
        if let s = event.charactersIgnoringModifiers?.unicodeScalars.first, (0xF700...0xF8FF).contains(s.value) {
            t.keyDown(with: event)
        } else if let chars = event.characters, !chars.isEmpty {
            t.insertText(chars, replacementRange: NSRange(location: NSNotFound, length: 0))
        }
    }

    /// `keyDown` ist in SwiftTerm nicht `open`; `performKeyEquivalent` sieht jedes Tastenereignis vorher.
    /// Tasten ohne Modifier gehen direkt ins Terminal, damit kein Menü-Kürzel das Tippen abfängt.
    /// ⌘Esc fängt die App fensterweit ab (Event-Monitor), Esc allein geht an Claude.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask).subtracting([.shift, .capsLock, .numericPad, .function])
        if mods.isEmpty, window?.firstResponder === self { keyDown(with: event); return true }
        return super.performKeyEquivalent(with: event)
    }

    /// Mit Metal liegt eine Zeichenfläche als Unteransicht über dem Terminal. Trifft der Klick sie, macht das Fenster
    /// nichts zum First Responder und die Kachel bekommt keine Tastatur. Deshalb nimmt das Terminal den Klick selbst,
    /// nur echte Bedienelemente (Scroller, Suchleiste) behalten ihn.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let v = super.hitTest(point) else { return nil }
        return v is NSControl || v.acceptsFirstResponder ? v : self
    }

    override func processTerminated(_ source: LocalProcess, exitCode: Int32?) {
        super.processTerminated(source, exitCode: exitCode)
        onExit?()
    }
}

/// Hält pro Session höchstens einen Claude-Prozess im Terminal (Kind von Kadrell), startet gestaffelt
/// und pflegt Text-Snapshots. Beendet Kadrell, enden die Prozesse; beim nächsten Start setzt `--resume` fort.
@MainActor
final class AttachManager {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "attach")
    let cli: ClaudeCLI
    private(set) var terminals: [String: KadrellTerminalView] = [:]
    private(set) var snapshots: [String: [String]] = [:]
    private var queue: [Session] = []
    /// Claude hat sich beendet (`/exit`, Absturz) oder wurde gestoppt: nicht automatisch neu starten, erst auf Klick.
    private(set) var ended: Set<String> = []
    /// Von Kadrell selbst beendete Prozesse (SIGHUP), mit pid, bis ihr Ende gemeldet ist.
    private var closing: [String: pid_t] = [:]
    private var queueTask: Task<Void, Never>?
    /// App wird beendet: nichts mehr starten, sonst setzt das Polling die eben beendeten Sessions fort.
    private var shuttingDown = false
    /// Erste Nachricht für neue Sessions (`kadrell new … <prompt>`), wird beim ersten Start verbraucht.
    var initialPrompts: [String: String] = [:]
    var onChange: (() -> Void)?
    /// Claude hat sich selbst beendet (`/exit`, zweimal ⌃C, Absturz), nicht von Kadrell gestoppt.
    var onEnded: ((String) -> Void)?

    init(cli: ClaudeCLI) {
        self.cli = cli
    }

    var attachedCount: Int { terminals.count }
    func isAttached(_ key: String) -> Bool { terminals[key] != nil }
    func isEnded(_ key: String) -> Bool { ended.contains(key) }
    func terminal(for key: String) -> KadrellTerminalView? { terminals[key] }
    func lines(for key: String) -> [String] { snapshots[key] ?? [] }
    /// Schlüssel der Session je pid ihres Claude-Prozesses.
    var pids: [Int: String] { Dictionary(terminals.map { (Int($0.value.process.shellPid), $0.key) }, uniquingKeysWith: { a, _ in a }) }

    /// Sessions je Welle, statt strikt eine alle 500 ms: bei vielen sichtbaren Kacheln (Grid, Auto-Modus) lebt
    /// sonst die letzte erst nach mehreren Sekunden, ohne dass das an CPU oder Prozessstart läge.
    private static let batchSize = 4

    /// Ersetzt die Warteschlange; Reihenfolge wie übergeben (sichtbare zuerst), eine Welle von `batchSize` alle 500 ms.
    func enqueue(_ sessions: [Session]) {
        queue = sessions.filter { terminals[$0.id] == nil && !ended.contains($0.id) }
        guard queueTask == nil, !queue.isEmpty else { return }
        queueTask = Task { [weak self] in
            while let self, !self.queue.isEmpty, !Task.isCancelled {
                for _ in 0..<min(AttachManager.batchSize, self.queue.count) { self.attachNow(self.queue.removeFirst()) }
                guard !self.queue.isEmpty else { break }
                try? await Task.sleep(for: .milliseconds(500))
            }
            self?.queueTask = nil
        }
    }

    /// Startet den Claude-Prozess der Session, auch wenn er vorher beendet war.
    func attachNow(_ session: Session) {
        guard !shuttingDown, terminals[session.id] == nil else { return }
        queue.removeAll { $0.id == session.id }
        ended.remove(session.id)
        snapshots[session.id] = nil
        let t = KadrellTerminalView(frame: NSRect(x: 0, y: 0, width: 960, height: 600), font: Settings.terminalFont, options: .default)
        t.lineSpacing = CGFloat(Settings.terminalLineSpacing)
        t.nativeBackgroundColor = Theme.bg
        applyColors(t)
        let key = session.id
        t.onExit = { [weak self] in
            // `t` hält die Closure und die Closure `t`: erst nach dem gemeldeten Ende lösen, sonst bleibt jedes Terminal im Speicher.
            t.onExit = nil
            guard let self else { return }
            if self.closing.removeValue(forKey: key) == nil {
                self.ended.insert(key)
                self.snapshots[key] = t.terminalStateSnapshot().visibleRows.map(\.text).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
                AttachManager.log.warning("claude \(key, privacy: .public) beendet: \(self.snapshots[key]?.suffix(3).joined(separator: " ") ?? "", privacy: .private)")
            }
            self.detach(key, signal: false)
            if self.ended.contains(key) { self.onEnded?(key) }
        }
        // Wie $TMUX_PANE: `kadrell` in dieser Session weiß, wo es läuft, und findet den Socket.
        var env = cli.environment
        env["KADRELL_SESSION_KEY"] = key
        env["KADRELL_SOCKET"] = ControlSocket.defaultPath
        env["KADRELL"] = Bundle.main.executablePath
        if let host = session.host {
            // Wie das tmux-Popup (M-h): an die laufende Remote-tmux hängen, ohne tmux dort eine Login-Shell.
            // Endet ssh (Detach, Fehler), bleibt die Kachel mit den letzten Zeilen stehen, Klick verbindet neu.
            let remote = session.tmuxSession.map { "tmux new -As " + SSHConfig.shellQuote($0) } ?? "tmux attach || tmux new -As main"
            let cmd = "command -v tmux >/dev/null 2>&1 && { \(remote); } || { echo 'kein tmux auf diesem Host, normale Shell'; exec \"$SHELL\" -l; }"
            AttachManager.log.info("ssh \(host, privacy: .private): \(remote, privacy: .private)")
            // `--`: ein Host wie `-oProxyCommand=…` bleibt Hostname und wird keine Option.
            t.startProcess(executable: "/usr/bin/ssh", args: ["-t", "-o", "ConnectTimeout=5", "--", host, cmd],
                           environment: env.map { "\($0.key)=\($0.value)" }, execName: "ssh", currentDirectory: session.cwd)
            terminals[key] = t
            onChange?()
            return
        }
        if session.isShell {
            // Login-Shell des Nutzers: argv[0] mit Bindestrich, wie Terminal.app sie startet.
            let shell = env["SHELL"] ?? "/bin/zsh"
            t.startProcess(executable: shell, environment: env.map { "\($0.key)=\($0.value)" },
                           execName: "-" + URL(fileURLWithPath: shell).lastPathComponent, currentDirectory: session.cwd)
            terminals[key] = t
            onChange?()
            return
        }
        let hasTranscript = Transcript.path(sessionId: session.sessionId, configDir: cli.configDir) != nil
        var args = ClaudeCLI.sessionArgs(sessionId: session.sessionId, hasTranscript: hasTranscript)
            + ClaudeCLI.launchArgs(allowBypass: Settings.claudeAllowBypass, mode: Settings.claudeMode, model: Settings.claudeModel, effort: Settings.claudeEffort)
        AttachManager.log.info("claude \(args.joined(separator: " "), privacy: .private) in \(session.cwd, privacy: .private)")
        if let prompt = initialPrompts.removeValue(forKey: key), !hasTranscript { args += ClaudeCLI.promptArgs(prompt) }
        t.startProcess(executable: cli.binary, args: args, environment: env.map { "\($0.key)=\($0.value)" },
                       execName: "claude", currentDirectory: session.cwd)
        terminals[key] = t
        onChange?()
    }

    /// Beendet den Claude-Prozess per `SIGHUP`. Die Konversation liegt im Transcript und lässt sich fortsetzen.
    /// `closing` merkt sich den Schlüssel schon vor der Prüfung auf `running`: ist der Prozess in diesem Moment
    /// schon beendet, aber `onExit` noch nicht zugestellt, gilt der spätere Aufruf trotzdem als erwartet, nicht
    /// als unbeaufsichtigtes Sessionende (das würde sonst fälschlich `ended`/`onEnded` auslösen).
    func detach(_ key: String, signal: Bool = true) {
        guard let t = terminals.removeValue(forKey: key) else { return }
        if signal {
            closing[key] = t.process.shellPid
            if t.process.running { AttachManager.signal(t.process.shellPid, SIGHUP) }
        }
        t.removeFromSuperview()
        if !ended.contains(key) { snapshots[key] = nil }
        onChange?()
    }

    /// Stoppen: Prozess beenden, Kachel bleibt mit dem letzten Bildschirm stehen, bis ein Klick fortsetzt.
    func stop(_ key: String) {
        if let t = terminals[key] { snapshots[key] = t.terminalStateSnapshot().visibleRows.map(\.text).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty } }
        ended.insert(key)
        detach(key)
    }

    /// Beendet Prozesse von Sessions, die aus Kadrell entfernt wurden.
    func sync(with sessions: [Session]) {
        let live = Set(sessions.map(\.id))
        for key in terminals.keys where !live.contains(key) { detach(key) }
        ended.formIntersection(live)
    }

    /// Schrift und Zeilenabstand aus den Einstellungen auf alle offenen Terminals; SwiftTerm passt Spalten und Zeilen selbst an.
    func applyTerminalSettings() {
        let font = Settings.terminalFont, spacing = CGFloat(Settings.terminalLineSpacing)
        let recolor = themeId != Theme.current.id
        themeId = Theme.current.id
        for t in terminals.values {
            if t.font != font { t.font = font }
            if t.lineSpacing != spacing { t.lineSpacing = spacing }
            if recolor { applyColors(t) }
        }
    }

    private var themeId = Theme.current.id

    /// Schrift, Cursor und ANSI-Farben aus dem Farbschema; den Hintergrund setzt die Kachel.
    private func applyColors(_ t: KadrellTerminalView) {
        t.nativeForegroundColor = Theme.fg
        t.caretColor = Theme.fg
        let ansi = Theme.current.ansi.map { c -> SwiftTerm.Color in
            let s = c.usingColorSpace(.sRGB) ?? c
            return SwiftTerm.Color(red: UInt16(s.redComponent * 65535), green: UInt16(s.greenComponent * 65535), blue: UInt16(s.blueComponent * 65535))
        }
        t.installColors(ansi + ansi)
    }

    func detachAll() {
        for key in Array(terminals.keys) { detach(key) }
    }

    /// Beenden der App: SIGHUP an alle, auf das gemeldete Ende warten, was nach `timeout` noch lebt, bekommt SIGKILL.
    func shutdown(timeout: TimeInterval = 5) async {
        shuttingDown = true
        queue.removeAll()
        detachAll()
        let deadline = Date().addingTimeInterval(timeout)
        while !closing.isEmpty, Date() < deadline { try? await Task.sleep(for: .milliseconds(100)) }
        for (key, pid) in closing {
            AttachManager.log.warning("claude \(key, privacy: .public) reagiert nicht auf SIGHUP, SIGKILL")
            AttachManager.signal(pid, SIGKILL)
        }
    }

    /// `forkpty` macht den Prozess zum Session- und Gruppenleiter: das Signal geht an die ganze Gruppe, damit Kinder
    /// (MCP-Server, Befehle aus dem Bash-Tool) nicht verwaist weiterlaufen. Gibt es die Gruppe nicht mehr, nur an die pid.
    static func signal(_ pid: pid_t, _ sig: Int32) {
        if kill(-pid, sig) != 0 { kill(pid, sig) }
    }

    /// Textzeilen aller angehängten Terminals aktualisieren. Kein Hintergrund-Timer: einzige Verbraucher sind die
    /// ⌘P-Suche (ruft vor dem Öffnen einmal auf) und `CellView.drawLines` für Kacheln ohne eingehängtes Terminal,
    /// beides seltene, gezielte Momente statt ein Polling mehrmals pro Sekunde.
    func refreshSnapshots() {
        var changed = false
        // Ein eingehängtes Terminal zeichnet sich selbst; nur ausgehängte brauchen den Snapshot überhaupt.
        for (key, t) in terminals where t.superview == nil {
            var rows = t.terminalStateSnapshot().visibleRows.map { row -> String in
                var s = row.text
                while let last = s.last, last.isWhitespace { s.removeLast() }
                return s
            }
            while rows.last?.isEmpty == true { rows.removeLast() }
            guard snapshots[key] != rows else { continue }
            snapshots[key] = rows
            changed = true
        }
        if changed { onChange?() }
    }
}
