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
    private var snapshotTask: Task<Void, Never>?
    /// Erste Nachricht für neue Sessions (`kadrell new … <prompt>`), wird beim ersten Start verbraucht.
    var initialPrompts: [String: String] = [:]
    var onChange: (() -> Void)?
    /// Claude hat sich selbst beendet (`/exit`, zweimal ⌃C, Absturz), nicht von Kadrell gestoppt.
    var onEnded: ((String) -> Void)?

    init(cli: ClaudeCLI) {
        self.cli = cli
        snapshotTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(500))
                self?.refreshSnapshots()
            }
        }
    }

    var attachedCount: Int { terminals.count }
    func isAttached(_ key: String) -> Bool { terminals[key] != nil }
    func isEnded(_ key: String) -> Bool { ended.contains(key) }
    func terminal(for key: String) -> KadrellTerminalView? { terminals[key] }
    func lines(for key: String) -> [String] { snapshots[key] ?? [] }
    /// Schlüssel der Session je pid ihres Claude-Prozesses.
    var pids: [Int: String] { Dictionary(terminals.map { (Int($0.value.process.shellPid), $0.key) }, uniquingKeysWith: { a, _ in a }) }

    /// Ersetzt die Warteschlange; Reihenfolge wie übergeben (sichtbare zuerst), eine Session alle 500 ms.
    func enqueue(_ sessions: [Session]) {
        queue = sessions.filter { terminals[$0.id] == nil && !ended.contains($0.id) }
        guard queueTask == nil, !queue.isEmpty else { return }
        queueTask = Task { [weak self] in
            while let self, !self.queue.isEmpty, !Task.isCancelled {
                let next = self.queue.removeFirst()
                self.attachNow(next)
                try? await Task.sleep(for: .milliseconds(500))
            }
            self?.queueTask = nil
        }
    }

    /// Startet den Claude-Prozess der Session, auch wenn er vorher beendet war.
    func attachNow(_ session: Session) {
        guard terminals[session.id] == nil else { return }
        queue.removeAll { $0.id == session.id }
        ended.remove(session.id)
        snapshots[session.id] = nil
        let t = KadrellTerminalView(frame: NSRect(x: 0, y: 0, width: 960, height: 600), font: Settings.terminalFont, options: .default)
        t.lineSpacing = CGFloat(Settings.terminalLineSpacing)
        t.nativeBackgroundColor = Theme.bg
        t.nativeForegroundColor = Theme.fg
        t.caretColor = Theme.fg
        let key = session.id
        t.onExit = { [weak self] in
            guard let self else { return }
            if self.closing.removeValue(forKey: key) == nil {
                self.ended.insert(key)
                self.snapshots[key] = t.terminalStateSnapshot().visibleRows.map(\.text).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
                AttachManager.log.warning("claude \(key, privacy: .public) beendet: \(self.snapshots[key]?.suffix(3).joined(separator: " ") ?? "", privacy: .public)")
            }
            self.detach(key, signal: false)
            if self.ended.contains(key) { self.onEnded?(key) }
        }
        let hasTranscript = Transcript.path(sessionId: session.sessionId) != nil
        var args = ClaudeCLI.sessionArgs(sessionId: session.sessionId, hasTranscript: hasTranscript)
            + ClaudeCLI.launchArgs(allowBypass: Settings.claudeAllowBypass, mode: Settings.claudeMode, model: Settings.claudeModel, effort: Settings.claudeEffort)
        AttachManager.log.info("claude \(args.joined(separator: " "), privacy: .public) in \(session.cwd, privacy: .public)")
        if let prompt = initialPrompts.removeValue(forKey: key), !hasTranscript { args.append(prompt) }
        // Wie $TMUX_PANE: `kadrell` in dieser Session weiß, wo es läuft, und findet den Socket.
        var env = cli.environment
        env["KADRELL_SESSION_KEY"] = key
        env["KADRELL_SOCKET"] = ControlSocket.defaultPath
        env["KADRELL"] = Bundle.main.executablePath
        t.startProcess(executable: cli.binary, args: args, environment: env.map { "\($0.key)=\($0.value)" },
                       execName: "claude", currentDirectory: session.cwd)
        terminals[key] = t
        onChange?()
    }

    /// Beendet den Claude-Prozess per `SIGHUP`. Die Konversation liegt im Transcript und lässt sich fortsetzen.
    func detach(_ key: String, signal: Bool = true) {
        guard let t = terminals.removeValue(forKey: key) else { return }
        if signal, t.process.running { closing[key] = t.process.shellPid; kill(t.process.shellPid, SIGHUP) }
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
        for t in terminals.values {
            if t.font != font { t.font = font }
            if t.lineSpacing != spacing { t.lineSpacing = spacing }
        }
    }

    func detachAll() {
        for key in Array(terminals.keys) { detach(key) }
    }

    /// Beenden der App: SIGHUP an alle, auf das gemeldete Ende warten, was nach `timeout` noch lebt, bekommt SIGKILL.
    func shutdown(timeout: TimeInterval = 5) async {
        detachAll()
        let deadline = Date().addingTimeInterval(timeout)
        while !closing.isEmpty, Date() < deadline { try? await Task.sleep(for: .milliseconds(100)) }
        for (key, pid) in closing {
            AttachManager.log.warning("claude \(key, privacy: .public) reagiert nicht auf SIGHUP, SIGKILL")
            kill(pid, SIGKILL)
        }
    }

    private func refreshSnapshots() {
        var changed = false
        for (key, t) in terminals {
            var rows = t.terminalStateSnapshot().visibleRows.map { $0.text.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression) }
            while rows.last?.isEmpty == true { rows.removeLast() }
            guard snapshots[key] != rows else { continue }
            snapshots[key] = rows
            // Nur Kacheln ohne eingehängtes Terminal zeigen den Snapshot; eingehängte zeichnen sich selbst.
            if t.superview == nil { changed = true }
        }
        if changed { onChange?() }
    }
}
