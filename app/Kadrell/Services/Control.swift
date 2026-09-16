import Foundation

/// Fernsteuerung wie `tmux`: `kadrell <befehl>` schickt eine Zeile JSON an den Socket der laufenden App
/// (`ControlSocket`), die App führt aus und antwortet mit Ausgabe und Exit-Code. Hier nur der reine Teil:
/// Protokoll, Befehle parsen, Ziele auflösen, Hilfetext.
struct ControlRequest: Codable, Sendable {
    var argv: [String]
    /// Arbeitsordner des Aufrufers, für relative Pfade.
    var cwd: String
    /// `KADRELL_SESSION_KEY` des Aufrufers, wenn er in einer Kadrell-Kachel läuft (wie `$TMUX_PANE`).
    var caller: String?
}

struct ControlResponse: Codable, Sendable {
    var status: Int32
    var stdout: String = ""
    var stderr: String = ""

    static func ok(_ out: String = "") -> ControlResponse { ControlResponse(status: 0, stdout: out.isEmpty || out.hasSuffix("\n") ? out : out + "\n") }
    static func fail(_ message: String) -> ControlResponse { ControlResponse(status: 1, stderr: "kadrell: " + message + "\n") }
}

struct ControlError: Error, Equatable {
    let message: String
    init(_ message: String) { self.message = message }
}

enum ControlCommand: Equatable {
    case help
    case list(json: Bool)
    case newGroup(dir: String, name: String?, color: String?)
    case newSession(target: String?, dir: String?, name: String?, detached: Bool, prompt: String?, resume: String? = nil)
    case select(target: String?, add: Bool)
    case layout(LayoutMode)
    case zoom(target: String?)
    case rename(target: String?, name: String)
    case setGroup(target: String?, name: String?, color: String?, favorite: Bool?)
    case stop(target: String?)
    case resume(target: String?)
    case killSession(target: String?)
    case killGroup(target: String?)
    case send(target: String?, text: String, enter: Bool, keys: Bool)
    case capture(target: String?, all: Bool)

    static let usage = """
    Kadrell fernsteuern (die App muss laufen, sonst wird sie gestartet).

    Ziele (-t): Session-Key oder dessen Anfang, sessionId, Titel; Gruppen-Id oder deren Anfang, Gruppenname.
    Ohne -t gilt die Session, in der das Kommando läuft ($KADRELL_SESSION_KEY), sonst die fokussierte.

      kadrell ls [--json]                                  Gruppen und Sessions auflisten
      kadrell new-group <ordner> [--name N] [--color #rrggbb]
                                                           Gruppe anlegen (als Favorit, bleibt auch leer stehen)
      kadrell new [-t gruppe] [-c ordner] [--name N] [-d] [--resume sessionId] [prompt …]
                                                           Session starten, optional mit erster Nachricht;
                                                           -d: im Hintergrund, Auswahl bleibt;
                                                           --resume: bestehende Konversation übernehmen
                                                           (Claude darf dort nicht mehr laufen, -c = ihr Ordner)
      kadrell select [-t session|gruppe] [-a]              zeigen (-a: zur Auswahl dazu/weg)
      kadrell layout grid|stack                            Layout
      kadrell zoom [-t session]                            Zoom ein/aus
      kadrell rename [-t session] <name>                   umbenennen (leer = Titel von Claude Code)
      kadrell set-group [-t gruppe] [--name N] [--color #rrggbb] [--favorite on|off]
      kadrell stop [-t session]                            Claude beenden, Kachel bleibt
      kadrell resume [-t session]                          Claude wieder starten
      kadrell kill [-t session]                            Session beenden und entfernen
      kadrell kill-group [-t gruppe]                       Gruppe samt Sessions beenden und entfernen
      kadrell send [-t session] [--no-enter] <text …>      Text eingeben und abschicken
      kadrell send [-t session] -k <taste …>               Tasten: Enter Escape Tab Up Down Left Right BSpace C-c C-d
      kadrell capture [-t session] [--all]                 Bildschirm ausgeben (--all: mit Verlauf)

    Die Session-Keys stehen in `kadrell ls`. Exit-Code 1 bei Fehlern, Meldung auf stderr.
    """

    /// Tastennamen für `send -k`, angelehnt an tmux `send-keys`.
    static let keyNames: [String: String] = [
        "enter": "\r", "escape": "\u{1b}", "esc": "\u{1b}", "tab": "\t", "bspace": "\u{7f}", "space": " ",
        "up": "\u{1b}[A", "down": "\u{1b}[B", "right": "\u{1b}[C", "left": "\u{1b}[D",
        "c-c": "\u{03}", "c-d": "\u{04}", "c-l": "\u{0c}", "c-u": "\u{15}",
    ]

    static func parse(_ argv: [String]) throws -> ControlCommand {
        guard let cmd = argv.first else { return .help }
        let rest = Array(argv.dropFirst())
        switch cmd {
        case "help", "-h", "--help":
            return .help
        case "ls", "list":
            let a = try Args(rest, bools: ["--json"])
            try a.noPositional()
            return .list(json: a.has("--json"))
        case "new-group":
            let a = try Args(rest, values: ["--name", "--color"])
            guard a.positional.count == 1 else { throw ControlError("new-group braucht genau einen Ordner") }
            return .newGroup(dir: a.positional[0], name: a["--name"], color: try color(a["--color"]))
        case "new", "new-session":
            let a = try Args(rest, values: ["-t", "-c", "--name", "--resume"], bools: ["-d"])
            let prompt = a.positional.joined(separator: " ")
            if a["--resume"] != nil, !prompt.isEmpty { throw ControlError("--resume und prompt schließen sich aus") }
            return .newSession(target: a["-t"], dir: a["-c"], name: a["--name"], detached: a.has("-d"), prompt: prompt.isEmpty ? nil : prompt, resume: a["--resume"])
        case "select":
            let a = try Args(rest, values: ["-t"], bools: ["-a"])
            try a.noPositional()
            return .select(target: a["-t"], add: a.has("-a"))
        case "layout":
            guard rest.count == 1, let m = LayoutMode(rawValue: rest[0]) else { throw ControlError("layout grid|stack") }
            return .layout(m)
        case "zoom":
            let a = try Args(rest, values: ["-t"])
            try a.noPositional()
            return .zoom(target: a["-t"])
        case "rename":
            let a = try Args(rest, values: ["-t"])
            return .rename(target: a["-t"], name: a.positional.joined(separator: " "))
        case "set-group":
            return try parseSetGroup(rest)
        case "stop", "resume", "kill", "kill-session", "kill-group":
            let a = try Args(rest, values: ["-t"])
            try a.noPositional()
            switch cmd {
            case "stop": return .stop(target: a["-t"])
            case "resume": return .resume(target: a["-t"])
            case "kill-group": return .killGroup(target: a["-t"])
            default: return .killSession(target: a["-t"])
            }
        case "send", "send-keys":
            return try parseSend(rest)
        case "capture", "capture-pane":
            let a = try Args(rest, values: ["-t"], bools: ["--all"])
            try a.noPositional()
            return .capture(target: a["-t"], all: a.has("--all"))
        default:
            throw ControlError("unbekannter Befehl „\(cmd)“, siehe kadrell help")
        }
    }

    private static func parseSetGroup(_ rest: [String]) throws -> ControlCommand {
        let a = try Args(rest, values: ["-t", "--name", "--color", "--favorite"])
        try a.noPositional()
        let fav = a["--favorite"]
        guard fav == nil || fav == "on" || fav == "off" else { throw ControlError("--favorite on|off") }
        return .setGroup(target: a["-t"], name: a["--name"], color: try color(a["--color"]), favorite: fav.map { $0 == "on" })
    }

    private static func parseSend(_ rest: [String]) throws -> ControlCommand {
        let a = try Args(rest, values: ["-t"], bools: ["--no-enter", "-k"])
        guard !a.positional.isEmpty else { throw ControlError("send braucht Text oder Tasten") }
        let keys = a.has("-k")
        if keys, let bad = a.positional.first(where: { keyNames[$0.lowercased()] == nil }) { throw ControlError("unbekannte Taste „\(bad)“") }
        return .send(target: a["-t"], text: a.positional.joined(separator: " "), enter: !keys && !a.has("--no-enter"), keys: keys)
    }

    private static func color(_ c: String?) throws -> String? {
        guard let c else { return nil }
        let hex = c.hasPrefix("#") ? String(c.dropFirst()) : c
        guard hex.range(of: "^[0-9a-fA-F]{6}$", options: .regularExpression) != nil else { throw ControlError("Farbe als #rrggbb") }
        return "#" + hex.lowercased()
    }

    /// Flags vor, zwischen und nach den übrigen Argumenten; `--` beendet die Flags.
    private struct Args {
        var values: [String: String] = [:]
        var flags: Set<String> = []
        var positional: [String] = []

        init(_ args: [String], values valued: Set<String> = [], bools: Set<String> = []) throws {
            var i = 0, flagsDone = false
            while i < args.count {
                let a = args[i]
                if !flagsDone, a == "--" { flagsDone = true }
                else if !flagsDone, valued.contains(a) {
                    guard i + 1 < args.count else { throw ControlError("\(a) braucht einen Wert") }
                    values[a] = args[i + 1]
                    i += 1
                } else if !flagsDone, bools.contains(a) { flags.insert(a) }
                else if !flagsDone, a.hasPrefix("-"), a.count > 1 { throw ControlError("unbekannte Option \(a)") }
                else { positional.append(a) }
                i += 1
            }
        }

        subscript(_ key: String) -> String? { values[key] }
        func has(_ flag: String) -> Bool { flags.contains(flag) }
        func noPositional() throws {
            if let p = positional.first { throw ControlError("unerwartetes Argument „\(p)“") }
        }
    }
}

/// Ziele auflösen: exakt vor Anfang, Anfang und Name gleichrangig, mehrdeutig ist ein Fehler.
enum ControlTarget {
    static func session(_ t: String?, sessions: [Session], caller: String?, focused: String?) throws -> Session {
        guard let t else {
            if let c = caller, let s = sessions.first(where: { $0.id == c }) { return s }
            if let f = focused, let s = sessions.first(where: { $0.id == f }) { return s }
            throw ControlError("keine Session angegeben (-t) und keine fokussiert")
        }
        if let s = sessions.first(where: { $0.id == t || $0.sessionId == t }) { return s }
        let lower = t.lowercased()
        let hits = sessions.filter { $0.id.hasPrefix(t) || $0.sessionId.hasPrefix(t) || $0.title.lowercased() == lower }
        guard hits.count <= 1 else { throw ControlError("„\(t)“ ist mehrdeutig: " + hits.map { String($0.id.prefix(8)) }.joined(separator: ", ")) }
        guard let s = hits.first else { throw ControlError("keine Session „\(t)“") }
        return s
    }

    static func group(_ t: String?, groups: [Group], sessions: [Session], caller: String?, focused: String?) throws -> Group {
        guard let t else {
            let s = try session(nil, sessions: sessions, caller: caller, focused: focused)
            guard let g = groups.first(where: { $0.sessionIds.contains(s.id) }) else { throw ControlError("Session ohne Gruppe") }
            return g
        }
        if let g = groups.first(where: { $0.id == t }) { return g }
        let lower = t.lowercased()
        let hits = groups.filter { $0.id.hasPrefix(t) || $0.name.lowercased() == lower }
        guard hits.count <= 1 else { throw ControlError("„\(t)“ ist mehrdeutig: " + hits.map { "\($0.name) (\($0.id.prefix(8)))" }.joined(separator: ", ")) }
        guard let g = hits.first else { throw ControlError("keine Gruppe „\(t)“") }
        return g
    }

    /// `select -t`: Session oder Gruppe. Passt beides, muss der Aufrufer genauer werden.
    static func sessionOrGroup(_ t: String?, groups: [Group], sessions: [Session], caller: String?, focused: String?) throws -> Either {
        guard t != nil else { return .session(try session(nil, sessions: sessions, caller: caller, focused: focused)) }
        let s = try? session(t, sessions: sessions, caller: caller, focused: focused)
        let g = try? group(t, groups: groups, sessions: sessions, caller: caller, focused: focused)
        switch (s, g) {
        case let (s?, nil): return .session(s)
        case let (nil, g?): return .group(g)
        case (.some, .some): throw ControlError("„\(t!)“ passt auf eine Session und eine Gruppe, bitte Key oder Id angeben")
        case (nil, nil):
            // Fehlermeldung der Session-Suche, damit „mehrdeutig“ nicht als „nicht gefunden“ erscheint.
            _ = try session(t, sessions: sessions, caller: caller, focused: focused)
            throw ControlError("keine Session oder Gruppe „\(t!)“")
        }
    }

    enum Either { case session(Session), group(Group) }

    /// `~`, relative Pfade gegen den Ordner des Aufrufers, `..` aufgelöst.
    static func path(_ p: String, cwd: String) -> String {
        let expanded = (p as NSString).expandingTildeInPath
        let abs = expanded.hasPrefix("/") ? expanded : (cwd as NSString).appendingPathComponent(expanded)
        return URL(fileURLWithPath: abs).standardizedFileURL.path
    }
}
