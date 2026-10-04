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
        case message(ExtensionMessage), garbage, violation(Kill), stderr([String]), eof
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
    private func signal(_ sig: Int32) {
        guard !finished, process.isRunning else { return }
        kill(process.processIdentifier, sig)
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
        var protocolLines = ProtocolLines(), errLines = Data()
        var buf = [UInt8](repeating: 0, count: 1 << 16)
        while fds.contains(where: { $0.fd >= 0 }) {
            if poll(&fds, nfds_t(fds.count), -1) < 0 {
                if errno == EINTR { continue }
                break
            }
            // stderr zuerst: was der Helper vor seinem Ende dorthin schrieb, kommt vor eof an.
            for i in fds.indices where fds[i].fd >= 0 && fds[i].revents != 0 {
                let n = read(fds[i].fd, &buf, buf.count)
                if n < 0 && errno == EINTR { continue }
                if n <= 0 {
                    fds[i].fd = -1
                    if i == 1 { post(.eof) }
                    continue
                }
                let chunk = Data(buf[0..<n])
                if i == 0 {
                    let lines = stderrLines(&errLines, chunk)
                    if !lines.isEmpty { post(.stderr(lines)) }
                } else {
                    protocolLines.feed(chunk).forEach(post)
                }
            }
        }
        // Ein Rest ohne Zeilenende geht nicht verloren.
        if !errLines.isEmpty { post(.stderr([clip(errLines)])) }
    }

    /// Ganze Zeilen aus stderr, auf `maxText` Zeichen gekürzt. Ein Rest ohne Zeilenende über 4 KB gilt als eigene Zeile,
    /// damit der Puffer nicht unbegrenzt wächst.
    nonisolated private static func stderrLines(_ pending: inout Data, _ chunk: Data) -> [String] {
        pending.append(chunk)
        var lines: [String] = []
        while let nl = pending.firstIndex(of: 0x0A) {
            lines.append(clip(pending[pending.startIndex..<nl]))
            pending.removeSubrange(pending.startIndex...nl)
        }
        if pending.count > 4096 {
            lines.append(clip(pending))
            pending = Data()
        }
        return lines
    }

    nonisolated fileprivate static func clip(_ data: Data) -> String {
        String(String(decoding: data, as: UTF8.self).prefix(maxText))
    }
}

/// Zerlegt stdout in Zeilen, zählt sie gegen die Flutgrenze und dekodiert sie. Nach einem Verstoß wird alles verworfen,
/// der Main-Thread killt dann den Prozess.
private struct ProtocolLines {
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
            events.append(take(line))
        }
        if !dead && pending.count > ExtensionProcess.maxLine { events.append(violation(.longLine)) }
        return events
    }

    private mutating func take(_ line: Data) -> ExtensionProcess.Event {
        if line.count > ExtensionProcess.maxLine { return violation(.longLine) }
        let now = Date()
        recent.removeAll { now.timeIntervalSince($0) >= 1 }
        recent.append(now)
        if recent.count > ExtensionProcess.maxMessagesPerSecond { return violation(.flood) }
        return ExtensionMessage.decode(line).map { .message($0) } ?? .garbage
    }

    private mutating func violation(_ why: ExtensionProcess.Kill) -> ExtensionProcess.Event {
        dead = true
        pending = Data()
        return .violation(why)
    }
}
