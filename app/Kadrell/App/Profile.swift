import AppKit
import os

/// Profil dieser Instanz: eigene Sessions, Gruppen, Einstellungen und eigener Socket. Ohne Namen ist es das
/// Standardprofil am bisherigen Ort, dafür muss nichts umziehen. `--profile <name>` (oder `KADRELL_PROFILE`) liegt
/// unter `profiles/<name>`, `--profile tmp` in einem Temp-Ordner, der beim Beenden verschwindet.
/// Mehrere Profile laufen parallel, dasselbe Profil nur einmal (Lock-Datei).
enum Profile {
    static let name: String? = {
        let r = resolveName(arguments: CommandLine.arguments, environment: ProcessInfo.processInfo.environment,
                            underTest: NSClassFromString("XCTestCase") != nil)
        if let bad = r.invalid {
            Logger(subsystem: "de.malura.kadrell", category: "profile").error("Ungültiger Profilname: \(bad, privacy: .public)")
            FileHandle.standardError.write(Data("kadrell: ungültiger Profilname „\(bad)“ (erlaubt: A-Z a-z 0-9 . _ -)\n".utf8))
            exit(64)
        }
        return r.name
    }()

    /// `--profile` vor `KADRELL_PROFILE`. Ein ungültiger Name kommt als `invalid` zurück, nie als Standardprofil.
    /// Unter XCTest ohne Angabe `tmp`, damit Tests nicht in die echten Daten schreiben.
    static func resolveName(arguments: [String], environment: [String: String], underTest: Bool) -> (name: String?, invalid: String?) {
        var raw: String?
        if let i = arguments.firstIndex(of: "--profile"), i + 1 < arguments.count { raw = arguments[i + 1] }
        else if let e = environment["KADRELL_PROFILE"], !e.isEmpty { raw = e }
        guard let raw else { return (underTest ? "tmp" : nil, nil) }
        guard let n = clean(raw) else { return (nil, raw) }
        return (n, nil)
    }

    /// Einstellungen, die ein neues Profil vom Standardprofil übernimmt. Raus: Zustand statt Vorliebe
    /// (Auswahl, Lesestand, Session-Reihenfolge, offene Fenster, Statistik) und `autoswitch.*`, weil die
    /// Account-Slots global im Schlüsselbund liegen und zwei Instanzen sonst gegeneinander wechseln.
    static func copiedPreferences(_ base: [String: Any]) -> [String: Any] {
        let prefixes = ["workspace.selected", "stats.", "autoswitch."]
        let keys: Set = ["sessions.unseen", "sidebar.flatOrder", "windows.open"]
        return base.filter { k, _ in !keys.contains(k) && !prefixes.contains { k.hasPrefix($0) } }
    }
    static var isTemporary: Bool { name == "tmp" }

    private static let bundleId = Bundle.main.bundleIdentifier ?? "de.malura.kadrell"
    private static let pid = ProcessInfo.processInfo.processIdentifier

    static let directory: URL = {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!.appendingPathComponent("de.malura.kadrell")
        let dir = switch name {
        case nil: base
        case "tmp": FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tmp-\(pid)")
        case let n?: base.appendingPathComponent("profiles/\(n)")
        }
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }()

    private static var suiteName: String? {
        switch name {
        case nil: nil
        case "tmp": "de.malura.kadrell.profile.tmp-\(pid)"
        case let n?: "de.malura.kadrell.profile.\(n)"
        }
    }

    /// Einstellungen des Profils. Ein neues Profil übernimmt die des Standardprofils, ohne Auswahl und Lesestand:
    /// die gehören zu Sessions, die es dort nicht gibt.
    nonisolated(unsafe) static let defaults: UserDefaults = {
        guard let suite = suiteName, let d = UserDefaults(suiteName: suite) else { return .standard }
        if d.persistentDomain(forName: suite)?.isEmpty ?? true, let base = UserDefaults.standard.persistentDomain(forName: bundleId) {
            d.setPersistentDomain(copiedPreferences(base), forName: suite)
        }
        return d
    }()

    /// Titelzusatz fürs Fenster, leer im Standardprofil.
    static var label: String { name.map { " · \($0)" } ?? "" }

    private static func clean(_ s: String) -> String? {
        let n = s.trimmingCharacters(in: .whitespaces)
        guard !n.isEmpty, n.range(of: "^[A-Za-z0-9._-]+$", options: .regularExpression) != nil else { return nil }
        return n
    }

    // MARK: Nur einmal pro Profil

    nonisolated(unsafe) private static var lockFD: Int32 = -1

    /// Sperrt das Profil für diese Instanz. Hält schon eine andere es, kommt deren pid zurück.
    static func acquire() -> pid_t? {
        let path = directory.appendingPathComponent("kadrell.lock").path
        let fd = open(path, O_CREAT | O_RDWR, 0o600)
        guard fd >= 0 else { return nil }
        guard flock(fd, LOCK_EX | LOCK_NB) == 0 else {
            let other = (try? String(contentsOfFile: path, encoding: .utf8)).flatMap { pid_t($0.trimmingCharacters(in: .whitespacesAndNewlines)) }
            close(fd)
            return other ?? 0
        }
        ftruncate(fd, 0)
        let s = "\(pid)\n"
        _ = s.withCString { write(fd, $0, strlen($0)) }
        lockFD = fd
        return nil
    }

    /// Temp-Profil: Ordner und Einstellungen wieder weg.
    static func cleanUp() {
        guard isTemporary, let suite = suiteName else { return }
        UserDefaults.standard.removePersistentDomain(forName: suite)
        // cfprefsd lässt sonst eine leere plist liegen, pro Temp-Start eine.
        let plist = FileManager.default.urls(for: .libraryDirectory, in: .userDomainMask).first!.appendingPathComponent("Preferences/\(suite).plist")
        try? FileManager.default.removeItem(at: plist)
        try? FileManager.default.removeItem(at: directory)
    }

    /// Reste von Temp-Profilen, deren Prozess nicht mehr läuft: Absturz, Test-Läufe, oder cfprefsd hat die plist
    /// nach dem Löschen noch einmal geschrieben. Läuft bei jedem Start.
    static func sweepStale() {
        let fm = FileManager.default
        func dead(_ name: String, prefix: String, suffix: String = "") -> Bool {
            guard name.hasPrefix(prefix), name.hasSuffix(suffix),
                  let p = pid_t(name.dropFirst(prefix.count).dropLast(suffix.count)), p != pid else { return false }
            return kill(p, 0) != 0 && errno == ESRCH
        }
        let prefs = fm.urls(for: .libraryDirectory, in: .userDomainMask).first!.appendingPathComponent("Preferences")
        for n in (try? fm.contentsOfDirectory(atPath: prefs.path)) ?? [] where dead(n, prefix: "de.malura.kadrell.profile.tmp-", suffix: ".plist") {
            UserDefaults.standard.removePersistentDomain(forName: String(n.dropLast(6)))
            try? fm.removeItem(at: prefs.appendingPathComponent(n))
        }
        let tmp = fm.temporaryDirectory
        for n in (try? fm.contentsOfDirectory(atPath: tmp.path)) ?? [] where dead(n, prefix: "kadrell-tmp-") {
            try? fm.removeItem(at: tmp.appendingPathComponent(n))
        }
    }

    /// Weitere Instanz mit frischem Temp-Profil, derselbe Build.
    static func launchTemporary() {
        _ = try? ProcessRunner.spawn("/usr/bin/open", ["-n", Bundle.main.bundlePath, "--args", "--profile", "tmp"])
    }
}
