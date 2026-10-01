import AppKit
import SwiftTerm
import os

/// Terminal einer laufenden Session.
@MainActor
final class KadrellTerminalView: LocalProcessTerminalView {
    var onExit: ((Int32?) -> Void)?
    /// `terminalDelegate` ist weak, deshalb hält das Terminal seinen Guard selbst.
    private var delegateGuard: TerminalDelegateGuard?

    /// Ersetzt das Terminal als eigenen Delegate durch den Guard (OSC 52 lesen gesperrt).
    func installDelegateGuard() {
        let g = TerminalDelegateGuard(self)
        delegateGuard = g
        terminalDelegate = g
    }

    enum LinkAction: Equatable { case open, reveal(URL) }

    /// Endungen, die beim Öffnen etwas ausführen oder installieren.
    nonisolated private static let runnableExtensions: Set<String> = ["app", "command", "tool", "terminal", "workflow", "pkg", "sh", "scpt", "applescript"]

    /// Links mit Schema (http, mailto, …) öffnen wie bisher. Lokale Pfade, die beim Öffnen etwas starten würden
    /// (App, Skript, ausführbare Datei), zeigt der Finder nur an: ein Programm in der Kachel bestimmt den Linktext.
    nonisolated static func linkAction(for link: String) -> LinkAction {
        let path: String
        if let url = URL(string: link), let scheme = url.scheme {
            guard scheme.lowercased() == "file" else { return .open }
            path = url.path
        } else {
            path = localPath(link)
        }
        let fm = FileManager.default
        var isDir: ObjCBool = false
        let exists = fm.fileExists(atPath: path, isDirectory: &isDir)
        let runnable = runnableExtensions.contains(URL(fileURLWithPath: path).pathExtension.lowercased())
            || (exists && !isDir.boolValue && fm.isExecutableFile(atPath: path))
        return runnable ? .reveal(URL(fileURLWithPath: path)) : .open
    }

    /// Wie SwiftTerm: ~ auflösen, „datei:12:4“ ohne Zeilenangabe, wenn nur so eine Datei existiert.
    nonisolated private static func localPath(_ link: String) -> String {
        let path = NSString(string: link).expandingTildeInPath
        guard !FileManager.default.fileExists(atPath: path),
              let r = path.range(of: #":[0-9]+(?::[0-9]+)?$"#, options: .regularExpression) else { return path }
        let stripped = String(path[..<r.lowerBound])
        return FileManager.default.fileExists(atPath: stripped) ? stripped : path
    }

    override func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
        switch Self.linkAction(for: link) {
        case .open: super.requestOpenLink(source: source, link: link, params: params)
        case .reveal(let url): NSWorkspace.shared.activateFileViewerSelecting([url])
        }
    }

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
    /// Tasten ohne Modifier gehen direkt ins Terminal, damit kein Menü-Kürzel das Tippen abfängt. Ebenso ⌥-Zeichen
    /// (QWERTZ ⌥5 „[“), sonst löst das Hauptmenü „Kachel 5“ aus; auf U.S. ändert sich nichts.
    /// ⌘Esc fängt die App fensterweit ab (Event-Monitor), Esc allein geht an Claude.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask).subtracting([.shift, .capsLock, .numericPad, .function])
        if mods.isEmpty || Hotkey.yieldsToTyping(event), window?.firstResponder === self { keyDown(with: event); return true }
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
        onExit?(exitCode)
    }
}

/// Delegate vor dem Terminal: reicht alles an `LocalProcessTerminalView` weiter (Eingabe, Größe, Titel, Links),
/// nur OSC 52 lesen bleibt gesperrt. Sonst liest jedes Programm in einer Kachel (`cat` auf eine präparierte Datei,
/// ein Remote-Host) ungefragt die Zwischenablage. Kopieren per OSC 52 bleibt erlaubt, Claude Code nutzt es.
@MainActor
final class TerminalDelegateGuard: TerminalViewDelegate {
    private weak var terminal: KadrellTerminalView?
    private var loggedRead = false

    init(_ terminal: KadrellTerminalView) { self.terminal = terminal }

    func clipboardRead(source: TerminalView) -> Data? {
        if !loggedRead {
            loggedRead = true
            AttachManager.log.notice("OSC 52: Lesen der Zwischenablage abgelehnt")
        }
        return nil
    }

    func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) { terminal?.sizeChanged(source: source, newCols: newCols, newRows: newRows) }
    func setTerminalTitle(source: TerminalView, title: String) { terminal?.setTerminalTitle(source: source, title: title) }
    func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) { terminal?.hostCurrentDirectoryUpdate(source: source, directory: directory) }
    func send(source: TerminalView, data: ArraySlice<UInt8>) { terminal?.send(source: source, data: data) }
    func scrolled(source: TerminalView, position: Double) { terminal?.scrolled(source: source, position: position) }
    func requestOpenLink(source: TerminalView, link: String, params: [String: String]) { terminal?.requestOpenLink(source: source, link: link, params: params) }
    func bell(source: TerminalView) { terminal?.bell(source: source) }
    func clipboardCopy(source: TerminalView, content: Data) { terminal?.clipboardCopy(source: source, content: content) }
    func iTermContent(source: TerminalView, content: ArraySlice<UInt8>) { terminal?.iTermContent(source: source, content: content) }
    func rangeChanged(source: TerminalView, startY: Int, endY: Int) { terminal?.rangeChanged(source: source, startY: startY, endY: endY) }

    func kittyClipboardCapabilities(source: TerminalView) -> KittyClipboardCapabilities {
        terminal?.kittyClipboardCapabilities(source: source) ?? []
    }

    func kittyClipboardAvailableMimeTypes(source: TerminalView, location: KittyClipboardLocation) -> [String]? {
        terminal?.kittyClipboardAvailableMimeTypes(source: source, location: location)
    }

    func kittyClipboardRead(source: TerminalView, location: KittyClipboardLocation, mimeType: String) -> KittyClipboardReadResult? {
        terminal?.kittyClipboardRead(source: source, location: location, mimeType: mimeType)
    }

    func kittyClipboardWrite(source: TerminalView, location: KittyClipboardLocation, content: KittyClipboardWriteContent) -> KittyClipboardWriteResult {
        terminal?.kittyClipboardWrite(source: source, location: location, content: content) ?? .unsupported
    }

    func kittyClipboardRequestPermission(source: TerminalView, request: KittyClipboardPermissionRequest) -> KittyClipboardPermissionResult {
        terminal?.kittyClipboardRequestPermission(source: source, request: request) ?? .deny
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
    /// Zeitpunkt des letzten `attachNow` je Session: stirbt der Prozess kurz danach mit Fehlercode, ist das ein
    /// Startfehler (kaputtes Flag, falscher Ordner), kein normales `/exit` (Kanboard #20).
    private var attachStarted: [String: CFAbsoluteTime] = [:]
    /// Exit-Code eines Startfehlers je Session, nur gesetzt bei schnellem, unerwartetem Ende.
    private(set) var exitCodes: [String: Int32] = [:]
    /// Ordner der Session existiert nicht mehr: kein Prozess gestartet, die Kachel zeigt das statt „STARTET …“ endlos.
    private(set) var missingFolder: Set<String> = []
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
    func isMissingFolder(_ key: String) -> Bool { missingFolder.contains(key) }
    func exitCode(for key: String) -> Int32? { exitCodes[key] }
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
        exitCodes[session.id] = nil
        missingFolder.remove(session.id)
        snapshots[session.id] = nil
        let key = session.id
        // Ordner weg (gelöscht, Worktree entfernt): kein Prozess, sonst startet der Klick auf „Klick setzt fort“
        // denselben kaputten Aufruf endlos neu (Kanboard #20). Remote-Sessions haben kein lokales `cwd`.
        if session.host == nil, !FileManager.default.fileExists(atPath: session.cwd) {
            ended.insert(key)
            missingFolder.insert(key)
            onChange?()
            return
        }
        var options = TerminalOptions.default
        options.scrollback = Settings.terminalScrollback
        let t = KadrellTerminalView(frame: NSRect(x: 0, y: 0, width: 960, height: 600), font: Settings.terminalFont, options: options)
        t.lineSpacing = CGFloat(Settings.terminalLineSpacing)
        t.nativeBackgroundColor = Theme.bg
        applyColors(t)
        t.installDelegateGuard()
        attachStarted[key] = CFAbsoluteTimeGetCurrent()
        // Wie $TMUX_PANE: `kadrell` in dieser Session weiß, wo es läuft, und findet den Socket.
        var env = cli.environment
        env["KADRELL_SESSION_KEY"] = key
        env["KADRELL_SOCKET"] = ControlSocket.defaultPath
        env["KADRELL"] = Bundle.main.executablePath
        let executable: String, args: [String], execName: String
        if let host = session.host {
            // Wie das tmux-Popup (M-h): an die laufende Remote-tmux hängen, ohne tmux dort eine Login-Shell.
            // Endet ssh (Detach, Fehler), bleibt die Kachel mit den letzten Zeilen stehen, Klick verbindet neu.
            let remote = session.tmuxSession.map { "tmux new -As " + SSHConfig.shellQuote($0) } ?? "tmux attach || tmux new -As main"
            let cmd = "command -v tmux >/dev/null 2>&1 && { \(remote); } || { echo 'kein tmux auf diesem Host, normale Shell'; exec \"$SHELL\" -l; }"
            AttachManager.log.info("ssh \(host, privacy: .private): \(remote, privacy: .private)")
            // `--`: ein Host wie `-oProxyCommand=…` bleibt Hostname und wird keine Option.
            (executable, args, execName) = ("/usr/bin/ssh", ["-t", "-o", "ConnectTimeout=5", "--", host, cmd], "ssh")
        } else if session.isShell {
            // Login-Shell des Nutzers: argv[0] mit Bindestrich, wie Terminal.app sie startet.
            let shell = env["SHELL"] ?? "/bin/zsh"
            (executable, args, execName) = (shell, [], "-" + URL(fileURLWithPath: shell).lastPathComponent)
        } else {
            let hasTranscript = Transcript.path(sessionId: session.sessionId, configDir: cli.configDir) != nil
            var claudeArgs = ClaudeCLI.sessionArgs(sessionId: session.sessionId, hasTranscript: hasTranscript)
                + ClaudeCLI.launchArgs(allowBypass: Settings.claudeAllowBypass, mode: Settings.claudeMode, model: Settings.claudeModel, effort: Settings.claudeEffort)
                + ClaudeHook.launchArgs
            AttachManager.log.info("claude \(claudeArgs.joined(separator: " "), privacy: .private) in \(session.cwd, privacy: .private)")
            if let prompt = initialPrompts.removeValue(forKey: key), !hasTranscript { claudeArgs += ClaudeCLI.promptArgs(prompt) }
            (executable, args, execName) = (cli.binary, claudeArgs, "claude")
        }
        t.startProcess(executable: executable, args: args, environment: env.map { "\($0.key)=\($0.value)" }, execName: execName, currentDirectory: session.cwd)
        // Das Ende kommt asynchron auf dem Main-Thread, `onExit` danach zu setzen verpasst nichts.
        let pid = t.process.shellPid
        t.onExit = { [weak self] exitCode in
            // `t` hält die Closure und die Closure `t`: erst nach dem gemeldeten Ende lösen, sonst bleibt jedes Terminal im Speicher.
            t.onExit = nil
            self?.processEnded(t, key: key, pid: pid, exitCode: exitCode)
        }
        terminals[key] = t
        onChange?()
    }

    /// Prozess `pid` von Terminal `t` ist beendet. Erwartet (von Kadrell gestoppt) ist das Ende nur, wenn `closing`
    /// genau diese pid hält. Ist `t` schon ersetzt oder ausgehängt (Stop, sofort fortgesetzt), betrifft das Ende
    /// das neue Terminal nicht: dann nur den `closing`-Eintrag räumen.
    private func processEnded(_ t: KadrellTerminalView, key: String, pid: pid_t, exitCode: Int32?) {
        let expected = closing[key] == pid
        if expected { closing[key] = nil }
        guard terminals[key] === t else { return }
        if !expected { markEnded(t, key: key, exitCode: exitCode) }
        detach(key, signal: false)
        if ended.contains(key) { onEnded?(key) }
    }

    /// Unerwartetes Ende (`/exit`, Absturz): letzten Bildschirm festhalten, Kachel zeigt „Klick setzt fort“.
    private func markEnded(_ t: KadrellTerminalView, key: String, exitCode: Int32?) {
        ended.insert(key)
        snapshots[key] = AttachManager.snapshotRows(t, trimTrailing: false)
        AttachManager.log.warning("claude \(key, privacy: .public) beendet: \(self.snapshots[key]?.suffix(3).joined(separator: " ") ?? "", privacy: .private)")
        // Innerhalb weniger Sekunden mit Fehlercode gestorben: kein reguläres `/exit`, sondern ein Startfehler
        // (kaputtes Flag, alte CLI, nicht eingeloggt). Die Kachel bekommt eine eigene Meldung statt der
        // normalen Schraffur, damit ein Klick nicht denselben Fehler stumm wiederholt.
        let quick = attachStarted[key].map { CFAbsoluteTimeGetCurrent() - $0 < 5 } ?? false
        if let exitCode, exitCode != 0, quick { exitCodes[key] = exitCode }
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
        if let t = terminals[key] { snapshots[key] = AttachManager.snapshotRows(t, trimTrailing: false) }
        ended.insert(key)
        detach(key)
    }

    /// Beendet Prozesse von Sessions, die aus Kadrell entfernt wurden.
    func sync(with sessions: [Session]) {
        let live = Set(sessions.map(\.id))
        for key in terminals.keys where !live.contains(key) { detach(key) }
        ended.formIntersection(live)
        missingFolder.formIntersection(live)
        exitCodes = exitCodes.filter { live.contains($0.key) }
        attachStarted = attachStarted.filter { live.contains($0.key) }
    }

    /// Schrift und Zeilenabstand aus den Einstellungen auf alle offenen Terminals; SwiftTerm passt Spalten und Zeilen selbst an.
    func applyTerminalSettings() {
        let font = Settings.terminalFont, spacing = CGFloat(Settings.terminalLineSpacing)
        let recolor = themeId != Theme.current.id
        themeId = Theme.current.id
        let scrollback = Settings.terminalScrollback, resize = scrollback != appliedScrollback
        appliedScrollback = scrollback
        for t in terminals.values {
            if t.font != font { t.font = font }
            if t.lineSpacing != spacing { t.lineSpacing = spacing }
            if recolor { applyColors(t) }
            // Laufende Terminals übernehmen den neuen Verlauf sofort, beim Verkleinern fallen die ältesten Zeilen weg.
            if resize { t.changeScrollback(scrollback) }
        }
    }

    private var themeId = Theme.current.id
    private var appliedScrollback = Settings.terminalScrollback

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
            let rows = AttachManager.snapshotRows(t, trimTrailing: true)
            guard snapshots[key] != rows else { continue }
            snapshots[key] = rows
            changed = true
        }
        if changed { onChange?() }
    }

    /// Sichtbare Zeilen als Text. `trimTrailing`: Leerraum am Zeilenende und leere Zeilen am Ende weg (Kachel, `capture`),
    /// sonst alle leeren Zeilen weg (letzter Bildschirm eines beendeten Prozesses).
    static func snapshotRows(_ t: TerminalView, trimTrailing: Bool) -> [String] {
        let rows = t.terminalStateSnapshot().visibleRows.map(\.text)
        guard trimTrailing else { return rows.filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty } }
        var trimmed = rows.map { row in
            var s = row
            while s.last?.isWhitespace == true { s.removeLast() }
            return s
        }
        while trimmed.last?.isEmpty == true { trimmed.removeLast() }
        return trimmed
    }
}
