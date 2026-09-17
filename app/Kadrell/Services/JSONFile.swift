import Foundation
import os

/// Gemeinsames Laden/Speichern für die JSON-Stores (sessions.json, groups.json, folders-uses.json), damit keiner
/// der drei für sich still leer läuft oder Schreibfehler verschluckt.
enum JSONFile {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "jsonfile")

    /// `{"version": 1, "items": [...]}`. Ein nacktes Array (das Format vor der Versionierung) gilt als Version 0.
    private struct Envelope<T: Codable>: Codable { var version: Int; var items: [T] }

    struct LoadResult<T> {
        var items: [T]
        /// Datei war kaputt (ganz oder teilweise) und wurde vor dem Laden gesichert.
        var corrupted = false
        /// Umschlag nennt eine höhere Version, als diese Kadrell-Version kennt (Downgrade von einem neueren
        /// DMG): nicht mehr speichern, sonst geht verloren, was die neuere Version geschrieben hat.
        var newerThanKnown = false
    }

    /// Lädt ein Array `[T]`, tolerant gegen kaputte Dateien und einzelne kaputte Einträge:
    /// - Datei fehlt: leerer Start, kein Fehler.
    /// - Ganze Datei unlesbar oder nur einzelne Einträge kaputt (Handbearbeitung, halb geschriebener Stand,
    ///   ein Feld künftig nicht mehr optional): Original nach `<name>.corrupt-<datum>.json` gesichert, bevor
    ///   je wieder hineingeschrieben wird, brauchbare Einträge bleiben erhalten.
    static func loadArray<T: Codable>(_ type: T.Type, from url: URL, currentVersion: Int, fail: ((String) -> Void)? = nil) -> LoadResult<T> {
        guard let data = try? Data(contentsOf: url) else { return LoadResult(items: []) }
        let dec = JSONDecoder()
        if let full = try? dec.decode(Envelope<T>.self, from: data) {
            return LoadResult(items: full.items, newerThanKnown: full.version > currentVersion)
        }
        if let items = try? dec.decode([T].self, from: data) { return LoadResult(items: items) }
        let obj = try? JSONSerialization.jsonObject(with: data)
        let rawArray = (obj as? [Any]) ?? ((obj as? [String: Any])?["items"] as? [Any]) ?? []
        let recovered: [T] = rawArray.compactMap { entry in
            (try? JSONSerialization.data(withJSONObject: entry)).flatMap { try? dec.decode(T.self, from: $0) }
        }
        quarantine(url)
        let message = rawArray.isEmpty
            ? "\(url.lastPathComponent) ist kaputt, Original gesichert, Datei beginnt leer"
            : "\(url.lastPathComponent): \(rawArray.count - recovered.count) von \(rawArray.count) Einträgen kaputt, Rest übernommen, Original gesichert"
        report(message, fail)
        return LoadResult(items: recovered, corrupted: true)
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
        guard let data = try? Data(contentsOf: url) else { return [:] }
        if let d = try? JSONDecoder().decode([String: V].self, from: data) { return d }
        quarantine(url)
        report("\(url.lastPathComponent) ist kaputt, Original gesichert, Datei beginnt leer", fail)
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
            report("Speichern von \(url.lastPathComponent) fehlgeschlagen: \(error.localizedDescription)", fail)
        }
    }

    private static func report(_ message: String, _ fail: ((String) -> Void)?) {
        log.error("\(message, privacy: .public)")
        fail?(message)
    }

    /// Verschiebt eine kaputte Datei aus dem Weg, bevor je wieder hineingeschrieben wird.
    private static func quarantine(_ url: URL) {
        guard FileManager.default.fileExists(atPath: url.path) else { return }
        let stamp = DateFormatter.corruptStamp.string(from: Date())
        let base = url.deletingPathExtension().lastPathComponent
        let dest = url.deletingLastPathComponent().appendingPathComponent("\(base).corrupt-\(stamp).json")
        try? FileManager.default.removeItem(at: dest)
        try? FileManager.default.moveItem(at: url, to: dest)
    }
}

private extension DateFormatter {
    static let corruptStamp: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.timeZone = TimeZone(identifier: "UTC")
        return f
    }()
}
