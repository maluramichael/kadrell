import Foundation

/// Ein Extension-Helper (`Kadrell ext-host <ordner>`) als Kindprozess: JSON-Zeilen über stdin/stdout, stderr ins Log.
/// Überwacht ihn selbst (ready, ping, Flut, Zeilenlänge, voller Eingangspuffer), killt hart und meldet das über
/// `onExit`. Eine Instanz gehört zu genau einem Prozess, `start` nur einmal aufrufen.
@MainActor
final class ExtensionProcess {
    static let readyTimeout: Duration = .seconds(3)
    static let pingInterval: Duration = .seconds(5)
    nonisolated static let maxMessagesPerSecond = 50
    nonisolated static let maxLine = 1 << 20
    static let maxInputBuffer = 1 << 20
    nonisolated static let maxText = 500
    nonisolated static let maxLinesPerSecond = 50

    /// Gründe für einen harten Kill.
    enum Kill: Sendable {
        case noReady, noPong, flood, longLine, inputFull

        var reason: String {
            switch self {
            case .noReady: String(localized: "Startet nicht (kein Ready binnen 3 s)", bundle: Bundle.app)
            case .noPong: String(localized: "Antwortet nicht mehr (kein Pong binnen 5 s)", bundle: Bundle.app)
            case .flood: String(localized: "Zu viele Nachrichten (mehr als 50 pro Sekunde)", bundle: Bundle.app)
            case .longLine: String(localized: "Nachricht zu groß (über 1 MB)", bundle: Bundle.app)
            case .inputFull: String(localized: "Nimmt keine Nachrichten mehr an (über 1 MB unverarbeitet)", bundle: Bundle.app)
            }
        }
    }

    /// Was der Lese-Thread an den Main-Thread meldet.
    fileprivate enum Event: Sendable {
        case message(ExtensionMessage), garbage, violation(Kill), stderr([String]), skipped(Int), eof
    }

    var onMessage: (ExtensionMessage) -> Void = { _ in }
    /// `expected` nur für Exits nach `stop()`, alles andere (auch ein selbst gewähltes Ende mit Status 0) ist ein Absturz.
    var onExit: (_ expected: Bool, _ reason: String) -> Void = { _, _ in }
    var onStderr: (String) -> Void = { _ in }

    private let process = Process()
    private let input = Pipe(), output = Pipe(), errors = Pipe()
    private var outbox = Data()
    private var writable: DispatchSourceWrite?
    private var readyTimer: Task<Void, Never>?
    private var timers: [Task<Void, Never>] = []
    private var awaitingPong: Int?
    private var lastPing = 0
    private var stopping = false
    private var killed: Kill?
    private var eof = false
    private var termination: (reason: Process.TerminationReason, status: Int32)?
    private var finished = false
    private var lastError: String?
    private var lastStderr: String?

    init(dir: URL, executable: String = Bundle.main.executablePath!, environment: [String: String]) {
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = ["ext-host", dir.path]
        process.environment = environment
        process.standardInput = input
        process.standardOutput = output
        process.standardError = errors
        // Nur der Helper soll die Pipes bekommen (Process setzt sie per dup2 ein, das hebt close-on-exec auf). Sonst erbt
        // jede Terminal-Session sie, SwiftTerm schließt beim Start nichts, und nach Kadrells Ende sähe der Helper nie EOF.
        for pipe in [input, output, errors] {
            for h in [pipe.fileHandleForReading, pipe.fileHandleForWriting] { _ = fcntl(h.fileDescriptor, F_SETFD, FD_CLOEXEC) }
        }
    }

    var pid: pid_t { process.processIdentifier }
    /// Nach `stop()` oder `kill()`: was der Prozess jetzt noch meldet, zählt nicht mehr.
    var isStopping: Bool { stopping }

    /// Die fds, die Kadrell nach dem Start offen hält (für Tests).
    var parentDescriptors: [Int32] {
        [input.fileHandleForWriting, output.fileHandleForReading, errors.fileHandleForReading].map(\.fileDescriptor)
    }

    /// Ohne `stop()` fallen gelassen: stdin schließen, der Helper beendet sich dann selbst, auch in einer Endlosschleife.
    deinit { try? input.fileHandleForWriting.close() }

    func start(hello: HostMessage) throws {
        let fd = input.fileHandleForWriting.fileDescriptor
        // Ein toter Helper darf Kadrell nicht per SIGPIPE mitreißen; nicht blockierend, damit die App nie wartet.
        _ = fcntl(fd, F_SETNOSIGPIPE, 1)
        _ = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL) | O_NONBLOCK)
        // Über die Main-Queue statt per Task: die Reihenfolge der Meldungen muss erhalten bleiben (Nachrichten vor eof).
        process.terminationHandler = { [weak self] p in
            let reason = p.terminationReason, status = p.terminationStatus
            DispatchQueue.main.async { self?.terminated(reason, status) }
        }
        try process.run()
        let out = output.fileHandleForReading, err = errors.fileHandleForReading
        let post: @Sendable (Event) -> Void = { [weak self] e in DispatchQueue.main.async { self?.handle(e) } }
        Thread { Self.pump(out: out, err: err, post: post) }.start()
        readyTimer = after(Self.readyTimeout) { [weak self] in self?.hardKill(.noReady) }
        send(hello)
    }

    func send(_ m: HostMessage) {
        guard process.isRunning, killed == nil, !finished else { return }
        outbox.append(m.line())
        flush()
        if outbox.count > Self.maxInputBuffer { hardKill(.inputFull) }
    }

    /// `shutdown`, nach 1 s SIGTERM, nach weiteren 1 s SIGKILL. Kehrt sofort zurück, das Ende kommt über `onExit`.
    func stop() {
        guard process.isRunning, killed == nil, !finished, !stopping else { return }
        stopping = true
        cancelTimers()
        send(.shutdown)
        timers = [after(.seconds(1)) { [weak self] in self?.signal(SIGTERM) },
                  after(.seconds(2)) { [weak self] in self?.signal(SIGKILL) }]
    }

    // MARK: Überwachung

    private func after(_ delay: Duration, repeating: Bool = false, _ body: @escaping @MainActor () -> Void) -> Task<Void, Never> {
        Task {
            repeat {
                try? await Task.sleep(for: delay)
                if Task.isCancelled { return }
                body()
            } while repeating
        }
    }

    private func cancelTimers() {
        readyTimer?.cancel()
        readyTimer = nil
        timers.forEach { $0.cancel() }
        timers = []
    }

    private func ping() {
        if awaitingPong != nil { return hardKill(.noPong) }
        lastPing += 1
        awaitingPong = lastPing
        send(.ping(lastPing))
    }

    private func hardKill(_ why: Kill) {
        guard killed == nil, !finished else { return }
        killed = why
        cancelTimers()
        signal(SIGKILL)
    }

    /// Nur solange der Prozess lebt: nach dem Einsammeln könnte die pid schon einem anderen Prozess gehören.
    /// Kinder aus `kadrell.exec` zuerst: sie liegen in eigenen Prozessgruppen und liefen sonst ohne ihren Timeout weiter.
    private func signal(_ sig: Int32) {
        guard !finished, process.isRunning else { return }
        ProcessRunner.killChildren(of: process.processIdentifier)
        Darwin.kill(process.processIdentifier, sig)
    }

    // MARK: Eingang (Main-Thread)

    private func handle(_ e: Event) {
        switch e {
        case .message(let m): receive(m)
        case .garbage: if !finished { onStderr(String(localized: "ungültige Nachricht verworfen", bundle: Bundle.app)) }
        case .violation(let why): hardKill(why)
        case .stderr(let lines):
            lastStderr = lines.last ?? lastStderr
            lines.forEach(onStderr)
        case .skipped(let n): onStderr(String(localized: "\(n) Zeilen übersprungen", bundle: Bundle.app))
        case .eof:
            eof = true
            finishIfDone()
        }
    }

    private func receive(_ m: ExtensionMessage) {
        guard !finished, killed == nil else { return }
        switch m {
        case .ready where readyTimer != nil:
            readyTimer?.cancel()
            readyTimer = nil
            timers.append(after(Self.pingInterval, repeating: true) { [weak self] in self?.ping() })
        case .pong(let id) where id == awaitingPong:
            awaitingPong = nil
        case .log(let level, let text):
            let clipped = String(text.prefix(Self.maxText))
            if level == "error" { lastError = clipped.split(separator: "\n", maxSplits: 1).first.map(String.init) }
            return onMessage(.log(level: level, text: clipped))
        default: break
        }
        onMessage(m)
    }

    // MARK: Ausgang

    private func flush() {
        let fd = input.fileHandleForWriting.fileDescriptor
        while !outbox.isEmpty {
            let n = outbox.withUnsafeBytes { write(fd, $0.baseAddress, $0.count) }
            if n > 0 { outbox.removeFirst(n); continue }
            if n < 0 && errno == EINTR { continue }
            if n < 0 && errno == EAGAIN { break }
            // EPIPE: der Helper ist weg, das Ende meldet der terminationHandler.
            outbox = Data()
        }
        if outbox.isEmpty {
            writable?.cancel()
            writable = nil
        } else if writable == nil {
            let src = DispatchSource.makeWriteSource(fileDescriptor: fd, queue: .main)
            src.setEventHandler { [weak self] in MainActor.assumeIsolated { self?.flush() } }
            src.resume()
            writable = src
        }
    }

    // MARK: Ende

    private func terminated(_ reason: Process.TerminationReason, _ status: Int32) {
        termination = (reason, status)
        finishIfDone()
    }

    /// Erst wenn der Prozess eingesammelt und stdout zu Ende gelesen ist, damit die letzte Fehlermeldung im Grund steht.
    private func finishIfDone() {
        guard eof, let t = termination, !finished else { return }
        finished = true
        cancelTimers()
        writable?.cancel()
        writable = nil
        outbox = Data()
        try? input.fileHandleForWriting.close()
        onExit(stopping, exitReason(t.reason, t.status))
    }

    private func exitReason(_ reason: Process.TerminationReason, _ status: Int32) -> String {
        if stopping { return String(localized: "Gestoppt", bundle: Bundle.app) }
        if let killed { return killed.reason }
        let code = Int(status)
        let base = reason == .uncaughtSignal ? String(localized: "Beendet durch Signal \(code)", bundle: Bundle.app)
                                             : String(localized: "Beendet mit Status \(code)", bundle: Bundle.app)
        return (lastError ?? lastStderr).map { base + ": " + $0 } ?? base
    }

    // MARK: Lese-Thread

    /// Liest stdout und stderr bis EOF. Blockiert nur diesen Thread; dekodiert wird hier, nicht auf dem Main-Thread,
    /// denn tief verschachteltes JSON kostet pro Zeile spürbar Zeit.
    nonisolated private static func pump(out: FileHandle, err: FileHandle, post: @Sendable (Event) -> Void) {
        var fds = [pollfd(fd: err.fileDescriptor, events: Int16(POLLIN), revents: 0),
                   pollfd(fd: out.fileDescriptor, events: Int16(POLLIN), revents: 0)]
        var protocolLines = ProtocolLines(), errLines = StderrLines()
        var buf = [UInt8](repeating: 0, count: 1 << 16)
        while fds.contains(where: { $0.fd >= 0 }) {
            // Steht eine Sammelzeile aus, nur bis zum Fensterende warten, damit sie auch ohne weitere Ausgabe rausgeht.
            let due = [protocolLines.logs.due, errLines.throttle.due].compactMap { $0 }.min()
            let wait = due.map { Int32(max(0, $0.timeIntervalSinceNow * 1000).rounded(.up)) } ?? -1
            if poll(&fds, nfds_t(fds.count), wait) < 0 {
                if errno == EINTR { continue }
                break
            }
            let now = Date()
            (protocolLines.logs.tick(now) + errLines.throttle.tick(now)).forEach(post)
            // stderr zuerst: was der Helper vor seinem Ende dorthin schrieb, kommt vor eof an.
            for i in fds.indices where fds[i].fd >= 0 && fds[i].revents != 0 {
                let n = read(fds[i].fd, &buf, buf.count)
                if n < 0 && errno == EINTR { continue }
                if n <= 0 {
                    fds[i].fd = -1
                    (i == 1 ? protocolLines.logs.flush() + [.eof] : errLines.finish()).forEach(post)
                    continue
                }
                let chunk = Data(buf[0..<n])
                (i == 0 ? errLines.feed(chunk, now) : protocolLines.feed(chunk)).forEach(post)
            }
        }
    }

    nonisolated fileprivate static func clip(_ data: Data) -> String {
        String(String(decoding: data, as: UTF8.self).prefix(maxText))
    }
}

/// Höchstens 50 Einträge pro Sekunde (festes Fenster), der Rest wird gezählt und einmal pro Sekunde als Sammelzeile
/// gemeldet. Für Log-Zeilen und stderr: kein Kill, aber eine Extension, die viel ausgibt, legt die App nicht lahm.
private struct Throttle {
    private var windowStart = Date.distantPast
    private var sent = 0
    private var skipped = 0

    /// Wann die ausstehende Sammelzeile fällig ist, nil ohne übersprungene Einträge.
    var due: Date? { skipped > 0 ? windowStart.addingTimeInterval(1) : nil }

    /// Darf der Eintrag durch? Eine fällige Sammelzeile des abgelaufenen Fensters landet vorher in `events`.
    mutating func admit(_ now: Date, _ events: inout [ExtensionProcess.Event]) -> Bool {
        events += tick(now)
        guard sent < ExtensionProcess.maxLinesPerSecond else {
            skipped += 1
            return false
        }
        sent += 1
        return true
    }

    /// Nach Ablauf des Fensters: neues Fenster, ausstehende Sammelzeile raus.
    mutating func tick(_ now: Date) -> [ExtensionProcess.Event] {
        guard now.timeIntervalSince(windowStart) >= 1 else { return [] }
        windowStart = now
        sent = 0
        return flush()
    }

    /// Ausstehende Sammelzeile sofort, etwa bei EOF.
    mutating func flush() -> [ExtensionProcess.Event] {
        defer { skipped = 0 }
        return skipped > 0 ? [.skipped(skipped)] : []
    }
}

/// Zerlegt stdout in Zeilen und dekodiert sie. Log-Nachrichten laufen durch die Drossel, alle anderen zählen gegen
/// die Flutgrenze. Nach einem Verstoß wird alles verworfen, der Main-Thread killt dann den Prozess.
private struct ProtocolLines {
    var logs = Throttle()
    private var pending = Data()
    private var recent: [Date] = []
    private var dead = false

    mutating func feed(_ chunk: Data) -> [ExtensionProcess.Event] {
        guard !dead else { return [] }
        pending.append(chunk)
        var events: [ExtensionProcess.Event] = []
        while !dead, let nl = pending.firstIndex(of: 0x0A) {
            // Erst herausschneiden: ein Verstoß in take leert pending.
            let line = pending[pending.startIndex..<nl]
            pending.removeSubrange(pending.startIndex...nl)
            take(line, &events)
        }
        if !dead && pending.count > ExtensionProcess.maxLine { events.append(violation(.longLine)) }
        return events
    }

    private mutating func take(_ line: Data, _ events: inout [ExtensionProcess.Event]) {
        if line.count > ExtensionProcess.maxLine { return events.append(violation(.longLine)) }
        let now = Date()
        let message = ExtensionMessage.decode(line)
        if let message, case .log = message {
            if logs.admit(now, &events) { events.append(.message(message)) }
            return
        }
        recent.removeAll { now.timeIntervalSince($0) >= 1 }
        recent.append(now)
        if recent.count > ExtensionProcess.maxMessagesPerSecond { return events.append(violation(.flood)) }
        events.append(message.map { .message($0) } ?? .garbage)
    }

    private mutating func violation(_ why: ExtensionProcess.Kill) -> ExtensionProcess.Event {
        dead = true
        pending = Data()
        return .violation(why)
    }
}

/// stderr in Zeilen, auf `maxText` Zeichen gekürzt und gedrosselt.
private struct StderrLines {
    var throttle = Throttle()
    private var pending = Data()

    mutating func feed(_ chunk: Data, _ now: Date) -> [ExtensionProcess.Event] {
        pending.append(chunk)
        var lines: [String] = []
        while let nl = pending.firstIndex(of: 0x0A) {
            lines.append(ExtensionProcess.clip(pending[pending.startIndex..<nl]))
            pending.removeSubrange(pending.startIndex...nl)
        }
        // Ein Rest ohne Zeilenende über 4 KB gilt als eigene Zeile, damit der Puffer nicht unbegrenzt wächst.
        if pending.count > 4096 {
            lines.append(ExtensionProcess.clip(pending))
            pending = Data()
        }
        return admit(lines, now)
    }

    /// Bei EOF: Rest ohne Zeilenende und ausstehende Sammelzeile sofort.
    mutating func finish() -> [ExtensionProcess.Event] {
        let rest = pending.isEmpty ? [] : [ExtensionProcess.clip(pending)]
        pending = Data()
        return admit(rest, Date()) + throttle.flush()
    }

    private mutating func admit(_ lines: [String], _ now: Date) -> [ExtensionProcess.Event] {
        var events: [ExtensionProcess.Event] = []
        var allowed: [String] = []
        for line in lines where throttle.admit(now, &events) { allowed.append(line) }
        if !allowed.isEmpty { events.append(.stderr(allowed)) }
        return events
    }
}
