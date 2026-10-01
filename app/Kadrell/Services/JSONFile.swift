import Foundation
import os

/// Gemeinsames Laden/Speichern für die JSON-Stores (sessions.json, groups.json, folders-uses.json), damit keiner
/// der drei für sich still leer läuft oder Schreibfehler verschluckt.
enum JSONFile {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "jsonfile")

    /// `{"version": 1, "items": [...]}`. Ein nacktes Array (das Format vor der Versionierung) gilt als Version 0.
    private struct Envelope<T: Codable>: Codable { var version: Int; var items: [T] }

    /// Nur der Kopf des Umschlags: erkennt eine neuere Version auch dann, wenn die Einträge nicht mehr passen.
    private struct Header: Decodable { let version: Int }

    struct LoadResult<T> {
        var items: [T]
        /// Datei war kaputt (ganz oder teilweise) und wurde vor dem Laden gesichert.
        var corrupted = false
        /// Umschlag nennt eine höhere Version, als diese Kadrell-Version kennt (Downgrade von einem neueren
        /// DMG): nicht mehr speichern, sonst geht verloren, was die neuere Version geschrieben hat.
        var newerThanKnown = false
        /// Datei existiert, war aber nicht lesbar, oder die Sicherung der kaputten Datei ist gescheitert.
        var unreadable = false
        /// Nicht speichern: das Original ist die einzige intakte Kopie.
        var readOnly: Bool { newerThanKnown || unreadable }
    }

    private enum ReadOutcome { case missing, data(Data), failed(Error) }

    /// Nur „Datei existiert nicht“ ist ein leerer Start, jeder andere Lesefehler nicht.
    private static func read(_ url: URL) -> ReadOutcome {
        do { return .data(try Data(contentsOf: url)) } catch {
            let ns = error as NSError
            let missing = (ns.domain == NSCocoaErrorDomain && ns.code == NSFileReadNoSuchFileError)
                || (ns.domain == NSPOSIXErrorDomain && ns.code == Int(ENOENT))
            return missing ? .missing : .failed(error)
        }
    }

    private static func reportUnreadable(_ url: URL, _ error: Error, _ fail: ((String) -> Void)?) {
        report(String(localized: "\(url.lastPathComponent) ist nicht lesbar (\(error.localizedDescription)), Datei wird nicht überschrieben", bundle: Bundle.app), fail)
    }

    /// Lädt ein Array `[T]`, tolerant gegen kaputte Dateien und einzelne kaputte Einträge:
    /// - Datei fehlt: leerer Start, kein Fehler. Jeder andere Lesefehler: `unreadable`, nicht überschreiben.
    /// - Höhere Version im Kopf: `newerThanKnown`, Einträge nach bestem Wissen, keine Quarantäne.
    /// - Ganze Datei unlesbar oder nur einzelne Einträge kaputt (Handbearbeitung, halb geschriebener Stand,
    ///   ein Feld künftig nicht mehr optional): Original nach `<name>.corrupt-<zeitstempel>.json` gesichert, bevor
    ///   je wieder hineingeschrieben wird, brauchbare Einträge bleiben erhalten.
    static func loadArray<T: Codable>(_ type: T.Type, from url: URL, currentVersion: Int, fail: ((String) -> Void)? = nil) -> LoadResult<T> {
        switch read(url) {
        case .missing: return LoadResult(items: [])
        case .failed(let error):
            reportUnreadable(url, error, fail)
            return LoadResult(items: [], unreadable: true)
        case .data(let data): return decodeArray(data, url: url, currentVersion: currentVersion, fail: fail)
        }
    }

    private static func decodeArray<T: Codable>(_ data: Data, url: URL, currentVersion: Int, fail: ((String) -> Void)?) -> LoadResult<T> {
        let dec = JSONDecoder()
        if let header = try? dec.decode(Header.self, from: data), header.version > currentVersion {
            let items = (try? dec.decode(Envelope<T>.self, from: data))?.items ?? recover(T.self, data, dec).items
            return LoadResult(items: items, newerThanKnown: true)
        }
        if let full = try? dec.decode(Envelope<T>.self, from: data) { return LoadResult(items: full.items) }
        if let items = try? dec.decode([T].self, from: data) { return LoadResult(items: items) }
        let (recovered, total) = recover(T.self, data, dec)
        let name = url.lastPathComponent
        let saved = quarantineReporting(url, fail) { path in
            total == 0
                ? String(localized: "\(name) ist kaputt, Original gesichert unter \(path), Datei beginnt leer", bundle: Bundle.app)
                : String(localized: "\(name): \(total - recovered.count) von \(total) Einträgen kaputt, Rest übernommen, Original gesichert unter \(path)", bundle: Bundle.app)
        }
        return LoadResult(items: recovered, corrupted: true, unreadable: !saved)
    }

    /// Entry für Entry dekodieren, was sich noch retten lässt.
    private static func recover<T: Decodable>(_ type: T.Type, _ data: Data, _ dec: JSONDecoder) -> (items: [T], total: Int) {
        let obj = try? JSONSerialization.jsonObject(with: data)
        let raw = (obj as? [Any]) ?? ((obj as? [String: Any])?["items"] as? [Any]) ?? []
        let items: [T] = raw.compactMap { entry in
            (try? JSONSerialization.data(withJSONObject: entry)).flatMap { try? dec.decode(T.self, from: $0) }
        }
        return (items, raw.count)
    }

    /// Schreibt `items`. Bis Version 1 als nacktes Array: ältere Kadrell-Versionen (etwa ein installiertes DMG
    /// neben dem Debug-Build) lesen einen Umschlag als kaputt und würden die Datei sonst leer überschreiben.
    /// Erst ein echter Formatwechsel (Version ≥ 2) schreibt den Umschlag. Fehler werden geloggt und gemeldet.
    static func saveArray<T: Codable>(_ items: [T], to url: URL, version: Int, fail: ((String) -> Void)? = nil) {
        write(url, fail) {
            let enc = JSONEncoder()
            enc.outputFormatting = [.prettyPrinted, .sortedKeys]
            return version >= 2 ? try enc.encode(Envelope(version: version, items: items)) : try enc.encode(items)
        }
    }

    /// Wie `loadArray`, aber für ein `[String: V]` (z. B. `folders-uses.json`). Ohne Versionierung: kein Feld
    /// darin ist bisher tri-state gewesen, ein Formatwechsel ist nicht in Sicht.
    static func loadDict<V: Decodable>(_ type: V.Type, from url: URL, fail: ((String) -> Void)? = nil) -> [String: V] {
        switch read(url) {
        case .missing: return [:]
        case .failed(let error): reportUnreadable(url, error, fail)
        case .data(let data):
            if let d = try? JSONDecoder().decode([String: V].self, from: data) { return d }
            let name = url.lastPathComponent
            quarantineReporting(url, fail) { String(localized: "\(name) ist kaputt, Original gesichert unter \($0), Datei beginnt leer", bundle: Bundle.app) }
        }
        return [:]
    }

    static func saveDict<V: Encodable>(_ dict: [String: V], to url: URL, fail: ((String) -> Void)? = nil) {
        write(url, fail) { try JSONEncoder().encode(dict) }
    }

    /// Ordner anlegen, atomar schreiben, Fehler loggen und melden.
    private static func write(_ url: URL, _ fail: ((String) -> Void)?, _ encode: () throws -> Data) {
        do {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try encode().write(to: url, options: .atomic)
        } catch {
            report(String(localized: "Speichern von \(url.lastPathComponent) fehlgeschlagen: \(error.localizedDescription)", bundle: Bundle.app), fail)
        }
    }

    private static func report(_ message: String, _ fail: ((String) -> Void)?) {
        log.error("\(message, privacy: .public)")
        fail?(message)
    }

    /// Sichert die kaputte Datei und meldet es; `false`, wenn das Verschieben scheitert (dann nicht überschreiben).
    @discardableResult
    private static func quarantineReporting(_ url: URL, _ fail: ((String) -> Void)?, _ saved: (String) -> String) -> Bool {
        guard let dest = quarantine(url) else {
            report(String(localized: "\(url.lastPathComponent): Sicherung fehlgeschlagen, Datei wird nicht überschrieben", bundle: Bundle.app), fail)
            return false
        }
        report(saved(dest.path), fail)
        return true
    }

    /// Verschiebt eine kaputte Datei aus dem Weg, bevor je wieder hineingeschrieben wird. Vorhandene Sicherungen
    /// bleiben liegen: bei gleichem Stempel kommt ein Zähler dazu. Ziel oder nil, wenn das Verschieben scheitert.
    private static func quarantine(_ url: URL) -> URL? {
        let fm = FileManager.default
        guard fm.fileExists(atPath: url.path) else { return nil }
        let stamp = DateFormatter.corruptStamp.string(from: Date())
        let dir = url.deletingLastPathComponent()
        let base = "\(url.deletingPathExtension().lastPathComponent).corrupt-\(stamp)"
        var dest = dir.appendingPathComponent("\(base).json")
        var n = 0
        while fm.fileExists(atPath: dest.path) {
            n += 1
            dest = dir.appendingPathComponent("\(base)-\(n).json")
        }
        do { try fm.moveItem(at: url, to: dest) } catch { return nil }
        return dest
    }
}

private extension DateFormatter {
    static let corruptStamp: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd-HHmmss"
        f.timeZone = TimeZone(identifier: "UTC")
        return f
    }()
}
