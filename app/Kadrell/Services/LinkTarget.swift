import Foundation
import UniformTypeIdentifiers

/// Ziel eines Klicks auf einen Link im Terminal. SwiftTerm erkennt Pfade, prüft sie beim Öffnen aber relativ zum
/// Arbeitsordner von Kadrell. Hier gelten relative Pfade ab dem Ordner der Session, wie in iTerm2.
enum LinkTarget: Equatable {
    case url(String)
    case path(String, isDirectory: Bool)

    /// `nil` = es gibt nichts zu öffnen (Datei existiert nicht): der Klick tut dann nichts, wie bisher.
    static func resolve(_ link: String, base: String, home: String = NSHomeDirectory(), fileManager fm: FileManager = .default) -> LinkTarget? {
        if link.range(of: #"^([A-Za-z][A-Za-z0-9+.-]*://|mailto:|tel:|file:)"#, options: .regularExpression) != nil { return .url(link) }
        var candidates = [link]
        // `datei.swift:12` und `datei.swift:12:4`: erst der ganze Text (eine Datei kann so heißen), dann ohne Position.
        if let r = link.range(of: #":[0-9]+(?::[0-9]+)?$"#, options: .regularExpression) { candidates.append(String(link[..<r.lowerBound])) }
        for c in candidates {
            let full = absolute(c, base: base, home: home)
            var isDir: ObjCBool = false
            if fm.fileExists(atPath: full, isDirectory: &isDir) { return .path(full, isDirectory: isDir.boolValue) }
        }
        return nil
    }

    /// Eingestellter Editor nur für Ordner und Textdateien. Bilder, PDFs, Archive und sonstige Binärdateien öffnet
    /// die Standard-App von macOS, ein Editor zeigt dort nur Zeichensalat. Nicht lesbar = Editor, wie bisher.
    static func prefersEditor(_ path: String, isDirectory: Bool) -> Bool {
        if isDirectory { return true }
        if let type = UTType(filenameExtension: (path as NSString).pathExtension), type.conforms(to: .image) { return false }
        guard let handle = FileHandle(forReadingAtPath: path) else { return true }
        defer { try? handle.close() }
        let head = (try? handle.read(upToCount: 8192)) ?? Data()
        return !head.contains(0)
    }

    private static func absolute(_ path: String, base: String, home: String) -> String {
        let p = path == "~" || path.hasPrefix("~/") ? home + String(path.dropFirst()) : path
        guard !p.hasPrefix("/") else { return (p as NSString).standardizingPath }
        return ((base as NSString).appendingPathComponent(p) as NSString).standardizingPath
    }
}
