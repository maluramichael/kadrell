import AppKit
import SwiftTerm
import os

/// Meldet Titel-Escapes (OSC 0/2) weiter; die View selbst kann den Delegate nicht spielen, ihre
/// gleichnamigen Methoden kollidieren mit dem Protokoll.
final class TitleWatcher: LocalProcessTerminalViewDelegate {
    let onTitle: @MainActor (String) -> Void
    init(onTitle: @escaping @MainActor (String) -> Void) { self.onTitle = onTitle }
    func sizeChanged(source: LocalProcessTerminalView, newCols: Int, newRows: Int) {}
    func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
    func processTerminated(source: TerminalView, exitCode: Int32?) {}
    func setTerminalTitle(source: LocalProcessTerminalView, title: String) {
        MainActor.assumeIsolated { onTitle(title) }
    }
}

/// Terminal einer angehängten Session.
@MainActor
final class KadrellTerminalView: LocalProcessTerminalView {
    var onExit: (() -> Void)?
    /// `processDelegate` ist weak, deshalb hier festhalten.
    var titleWatcher: TitleWatcher? { didSet { processDelegate = titleWatcher } }

    /// `keyDown` ist in SwiftTerm nicht `open`; `performKeyEquivalent` sieht jedes Tastenereignis vorher.
    /// Tasten ohne Modifier gehen direkt ins Terminal, damit kein Menü-Kürzel das Tippen abfängt.
    /// ⌘Esc fängt die App fensterweit ab (Event-Monitor), Esc allein geht an Claude.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask).subtracting([.shift, .capsLock, .numericPad, .function])
        if mods.isEmpty, window?.firstResponder === self { keyDown(with: event); return true }
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
    /// Von Kadrell selbst beendete Clients (SIGHUP): deren Exit ist kein Nutzer-Ausstieg.
    private var closing: Set<String> = []
    private var queueTask: Task<Void, Never>?
    private var snapshotTask: Task<Void, Never>?
    var onChange: (() -> Void)?
    /// Der Nutzer hat den Attach-Client selbst verlassen (Ctrl-C/Ctrl-D), die Session lief dabei noch.
    var onClientExit: ((String) -> Void)?

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

    /// Ersetzt die Warteschlange; Reihenfolge wie übergeben (sichtbare zuerst), eine Session alle 500 ms.
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
        let key = session.id
        // Ctrl-C/Ctrl-D/← in der Session detacht in den Agent-View, der setzt den Titel „… claude agents“.
        // Für Kadrell heißt das: der Nutzer ist raus, Client beenden und wie einen Ausstieg behandeln.
        t.titleWatcher = TitleWatcher { [weak self] title in
            guard let self, title.hasSuffix("claude agents"), self.terminals[key] != nil else { return }
            self.detach(key)
            self.onClientExit?(key)
        }
        t.onExit = { [weak self] in
            guard let self else { return }
            if let t0 = self.startedAt[key], Date().timeIntervalSince(t0) < 5 {
                self.failed.insert(key)
                self.snapshots[key] = t.terminalStateSnapshot().visibleRows.map(\.text).filter { !$0.trimmingCharacters(in: .whitespaces).isEmpty }
                AttachManager.log.warning("attach \(key, privacy: .public) sofort beendet: \(self.snapshots[key]?.joined(separator: " ") ?? "", privacy: .public)")
            }
            let expected = self.closing.remove(key) != nil || self.failed.contains(key)
            self.detach(key, signal: false)
            if !expected { self.onClientExit?(key) }
        }
        t.startProcess(executable: cli.binary, args: ["attach", id], environment: cli.environmentList,
                       execName: "claude", currentDirectory: session.cwd)
        terminals[key] = t
        onChange?()
    }

    /// Hängt aus: `SIGHUP` an den Attach-Client (die Hintergrund-Session läuft weiter, siehe docs/kadrell-verifikation.md).
    func detach(_ key: String, signal: Bool = true) {
        guard let t = terminals.removeValue(forKey: key) else { return }
        if signal, t.process.running { closing.insert(key); kill(t.process.shellPid, SIGHUP) }
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
