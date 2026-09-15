import AppKit
import SwiftTerm
import os

/// Terminal einer angehängten Session. Esc verlässt den Fokus statt an Claude zu gehen.
@MainActor
final class KadrellTerminalView: LocalProcessTerminalView {
    var onEscape: (() -> Void)?
    var onExit: (() -> Void)?

    /// `keyDown` ist in SwiftTerm nicht `open`; `performKeyEquivalent` sieht jedes Tastenereignis vorher.
    /// Esc geht an Claude (Interrupt), ⌘Esc verlässt den Fokus. Alle anderen Tasten ohne Modifier gehen
    /// direkt ins Terminal, damit Menü-Kürzel ohne Modifier (`F` für Fit) das Tippen nicht abfangen.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard window?.firstResponder === self else { return super.performKeyEquivalent(with: event) }
        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask).subtracting([.shift, .capsLock, .numericPad, .function])
        if event.keyCode == 53, mods == [.command] { onEscape?(); return true }
        if mods.isEmpty { keyDown(with: event); return true }
        return super.performKeyEquivalent(with: event)
    }

    override func processTerminated(_ source: LocalProcess, exitCode: Int32?) {
        super.processTerminated(source, exitCode: exitCode)
        onExit?()
    }
}

/// Hält pro angehängter Session genau ein Terminal, hängt gestaffelt an und pflegt Text-Snapshots.
@MainActor
final class AttachManager {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "attach")
    let cli: ClaudeCLI
    private(set) var terminals: [String: KadrellTerminalView] = [:]
    private(set) var snapshots: [String: [String]] = [:]
    private var queue: [Session] = []
    /// Attach-Clients, die sofort wieder beendet wurden (z. B. „no saved transcript“): nicht automatisch neu versuchen.
    private(set) var failed: Set<String> = []
    private var startedAt: [String: Date] = [:]
    private var queueTask: Task<Void, Never>?
    private var snapshotTask: Task<Void, Never>?
    var onChange: (() -> Void)?
    var onEscape: (() -> Void)?

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
    func terminal(for key: String) -> KadrellTerminalView? { terminals[key] }
    func lines(for key: String) -> [String] { snapshots[key] ?? [] }

    /// Ersetzt die Warteschlange; Reihenfolge = Abstand zur Viewport-Mitte, eine Session alle 500 ms.
    func enqueue(_ sessions: [Session]) {
        queue = sessions.filter { $0.canAttach && terminals[$0.id] == nil && !failed.contains($0.id) }
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

    func attachNow(_ session: Session) {
        guard session.canAttach, let id = session.shortId, terminals[session.id] == nil else { return }
        queue.removeAll { $0.id == session.id }
        failed.remove(session.id)
        startedAt[session.id] = Date()
        let t = KadrellTerminalView(frame: NSRect(x: 0, y: 0, width: 960, height: 600), font: Theme.font(12), options: .default)
        t.nativeBackgroundColor = Theme.bg
        t.nativeForegroundColor = Theme.fg
        t.caretColor = Theme.fg
        t.onEscape = { [weak self] in self?.onEscape?() }
        let key = session.id
        t.onExit = { [weak self] in
            guard let self else { return }
            if let t0 = self.startedAt[key], Date().timeIntervalSince(t0) < 5 {
                self.failed.insert(key)
                self.snapshots[key] = t.terminalStateSnapshot().visibleRows.map(\.text).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
                AttachManager.log.warning("attach \(key, privacy: .public) sofort beendet: \(self.snapshots[key]?.joined(separator: " ") ?? "", privacy: .public)")
            }
            self.detach(key, signal: false)
        }
        t.startProcess(executable: cli.binary, args: ["attach", id], environment: cli.environmentList,
                       execName: "claude", currentDirectory: session.cwd)
        terminals[key] = t
        onChange?()
    }

    /// Hängt aus: `SIGHUP` an den Attach-Client (die Hintergrund-Session läuft weiter, siehe docs/kadrell-verifikation.md).
    func detach(_ key: String, signal: Bool = true) {
        guard let t = terminals.removeValue(forKey: key) else { return }
        if signal, t.process.running { kill(t.process.shellPid, SIGHUP) }
        t.removeFromSuperview()
        if !failed.contains(key) { snapshots[key] = nil }
        onChange?()
    }

    /// Entfernt Terminals von Sessions, die es nicht mehr gibt oder die beendet wurden.
    func sync(with sessions: [Session]) {
        let live = Set(sessions.filter(\.canAttach).map(\.id))
        for key in terminals.keys where !live.contains(key) { detach(key) }
    }

    func detachAll() {
        for key in Array(terminals.keys) { detach(key) }
    }

    private func refreshSnapshots() {
        var changed = false
        for (key, t) in terminals {
            var rows = t.terminalStateSnapshot().visibleRows.map { $0.text.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression) }
            while rows.last?.isEmpty == true { rows.removeLast() }
            if snapshots[key] != rows { snapshots[key] = rows; changed = true }
        }
        if changed { onChange?() }
    }
}
