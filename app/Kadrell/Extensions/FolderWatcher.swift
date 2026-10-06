import CoreServices
import Foundation

/// Meldet Änderungen unter einem Ordner, gebündelt nach `latency` Sekunden. FSEvents statt DispatchSource: eine
/// Quelle auf einem Ordner sieht nur neue, gelöschte und umbenannte Einträge, kein Schreiben in eine bestehende
/// Datei (so speichern manche Editoren) und nichts in Unterordnern. Der Ordner darf fehlen und später entstehen.
@MainActor
final class FolderWatcher {
    private let path: String
    private let latency: TimeInterval
    private let onChange: () -> Void
    private var stream: FSEventStreamRef?

    init(_ url: URL, latency: TimeInterval, onChange: @escaping () -> Void) {
        path = url.path
        self.latency = latency
        self.onChange = onChange
        open()
    }

    /// Ohne `stop()` bleibt der Watcher am Leben: der Stream hält ihn.
    func stop() { close() }

    private func open() {
        // Der Stream hält den Watcher selbst (retain/release), damit eine nachgereichte Meldung nie ins Leere zeigt.
        var context = FSEventStreamContext(
            version: 0, info: Unmanaged.passUnretained(self).toOpaque(),
            retain: { info in
                if let info { _ = Unmanaged<FolderWatcher>.fromOpaque(info).retain() }
                return info
            },
            release: { info in if let info { Unmanaged<FolderWatcher>.fromOpaque(info).release() } },
            copyDescription: nil)
        let callback: FSEventStreamCallback = { _, info, count, _, flags, _ in
            guard let info else { return }
            let watcher = Unmanaged<FolderWatcher>.fromOpaque(info).takeUnretainedValue()
            let rootChanged = (0..<count).contains { flags[$0] & FSEventStreamEventFlags(kFSEventStreamEventFlagRootChanged) != 0 }
            MainActor.assumeIsolated { watcher.fired(rootChanged: rootChanged) }
        }
        let flags = FSEventStreamCreateFlags(kFSEventStreamCreateFlagWatchRoot)
        guard let s = FSEventStreamCreate(nil, callback, &context, [path] as CFArray,
                                          FSEventStreamEventId(kFSEventStreamEventIdSinceNow), latency, flags) else { return }
        FSEventStreamSetDispatchQueue(s, .main)
        FSEventStreamStart(s)
        stream = s
    }

    private func close() {
        guard let s = stream else { return }
        FSEventStreamStop(s)
        FSEventStreamInvalidate(s)
        FSEventStreamRelease(s)
        stream = nil
    }

    private func fired(rootChanged: Bool) {
        guard stream != nil else { return }
        // Ein Stream auf einen damals fehlenden Ordner meldet nach dessen Entstehen nur das, nichts darin: neu aufsetzen.
        // Erst nach dem Callback: den eigenen Stream darin freizugeben, zieht FSEvents den Boden unter den Füßen weg.
        if rootChanged {
            DispatchQueue.main.async { [weak self] in
                MainActor.assumeIsolated {
                    guard let self, self.stream != nil else { return }
                    self.close()
                    self.open()
                }
            }
        }
        onChange()
    }
}
