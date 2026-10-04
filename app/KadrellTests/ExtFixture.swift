import Foundation
import XCTest

/// Legt einen Extension-Ordner (`kadrell.json`, `init.lua`) in einem frischen Temp-Verzeichnis an.
struct ExtFixture {
    static func make(name: String, initLua: String, manifest: [String: Any]? = nil) throws -> URL {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("kadrell-ext-\(UUID().uuidString)").appendingPathComponent(name)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let manifest = manifest ?? ["name": name, "version": "0.1.0", "apiVersion": 1]
        try JSONSerialization.data(withJSONObject: manifest).write(to: dir.appendingPathComponent("kadrell.json"))
        try Data(initLua.utf8).write(to: dir.appendingPathComponent("init.lua"))
        return dir
    }
}

/// Startet `Kadrell ext-host <dir>` und spricht mit ihm JSON-Zeilen über stdin/stdout.
/// `@unchecked Sendable`: der Lese-Handler läuft auf einem fremden Thread, Zugriff auf `pending` und `lines` nur unter `lock`.
final class HostPipe: @unchecked Sendable {
    let process = Process()
    private let input = Pipe(), output = Pipe()
    private let lock = NSLock()
    private var pending = Data()
    private var lines: [Data] = []

    init(dir: URL) throws {
        process.executableURL = URL(fileURLWithPath: try XCTUnwrap(Bundle.main.executablePath))
        process.arguments = ["ext-host", dir.path]
        // Eigene Umgebung: die des Test-Hosts trägt die XCTest-Injektion (DYLD_*), die soll der Helper nicht erben.
        process.environment = ["HOME": NSHomeDirectory(), "PATH": "/usr/bin:/bin"]
        process.standardInput = input
        process.standardOutput = output
        // Ein abgestürzter Helper darf den Test-Host nicht per SIGPIPE mitreißen, Schreiben liefert dann nur EPIPE.
        _ = fcntl(input.fileHandleForWriting.fileDescriptor, F_SETNOSIGPIPE, 1)
        output.fileHandleForReading.readabilityHandler = { [weak self] handle in self?.receive(handle.availableData) }
        try process.run()
    }

    deinit {
        output.fileHandleForReading.readabilityHandler = nil
        if process.isRunning { process.terminate() }
    }

    func send(_ obj: [String: Any]) {
        guard var data = try? JSONSerialization.data(withJSONObject: obj) else { return }
        data.append(0x0A)
        try? input.fileHandleForWriting.write(contentsOf: data)
    }

    func next(timeout: TimeInterval) -> [String: Any]? {
        // Abfragen statt auf eine Bedingung warten: der Lese-Handler läuft mit niedrigerer QoS als der Test,
        // ein Warten darauf meldet der Thread-Checker als Prioritätsinversion.
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let line = lock.withLock({ lines.isEmpty ? nil : lines.removeFirst() }) {
                return try? JSONSerialization.jsonObject(with: line) as? [String: Any]
            }
            usleep(10_000)
        }
        return nil
    }

    func closeStdin() {
        try? input.fileHandleForWriting.close()
    }

    /// Wartet bis zu `timeout` auf das Prozessende.
    func waitForExit(timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while process.isRunning, Date() < deadline { usleep(20_000) }
        return !process.isRunning
    }

    private func receive(_ data: Data) {
        lock.withLock {
            pending.append(data)
            while let nl = pending.firstIndex(of: 0x0A) {
                lines.append(pending[pending.startIndex..<nl])
                pending.removeSubrange(pending.startIndex...nl)
            }
        }
    }
}
