import XCTest
@testable import Kadrell

final class SessionParsingTests: XCTestCase {
    let json = """
    [
      { "id": "0ace1aab", "cwd": "/Users/dev/development/projects/homelab", "kind": "background",
        "startedAt": 1786719895859, "sessionId": "0ace1aab-c1a7-45e2-9856-dea5d34eda81",
        "name": "Weltport für Raspberry Pi Display Dashboard prüfen", "state": "blocked" },
      { "pid": 5395, "cwd": "/Users/dev/development/acme/intern/demoapp", "kind": "interactive",
        "startedAt": 1789040585856, "sessionId": "b746c5bb-ecfe-45d8-b268-7df9b23b5e44", "name": "demoapp-78", "status": "idle" },
      { "pid": 18859, "id": "86f99758", "cwd": "/private/tmp/kadrell-spike", "kind": "background", "startedAt": 1789479734977,
        "sessionId": "86f99758-a2e1-4923-b27e-1374ddea59b9", "name": "kadrell-spike", "status": "busy", "state": "working" },
      { "pid": 76489, "cwd": "/x", "kind": "interactive", "startedAt": 1789452498737, "sessionId": "c6994699-de7e-4a0a-86dc-e79a90802d67",
        "name": "export", "status": "waiting", "waitingFor": "input needed" }
    ]
    """

    func testDecodeAgentsJSON() throws {
        let list = try Agent.decodeList(Data(json.utf8))
        XCTAssertEqual(list.count, 4)
        XCTAssertEqual(list[0].shortId, "0ace1aab")
        XCTAssertFalse(list[0].isRunningBackground)   // ohne pid: gestoppt oder weg
        XCTAssertNil(list[1].shortId)
        XCTAssertFalse(list[1].isRunningBackground)   // interaktiv
        XCTAssertEqual(list[1].pid, 5395)
        XCTAssertTrue(list[2].isRunningBackground)
        XCTAssertEqual(list[3].waitingFor, "input needed")
    }

    func testEntryWithoutNameDoesNotBreakTheList() throws {
        let json = #"[{"id":"79a85a2c","cwd":"/p","kind":"background","startedAt":1,"sessionId":"79a85a2c-x","state":"blocked"},{"id":"aa","cwd":"/p","kind":"background","startedAt":2,"sessionId":"aa-x","name":"fix","pid":5,"status":"idle"}]"#
        let l = try Agent.decodeList(Data(json.utf8))
        XCTAssertEqual(l.count, 2)
        XCTAssertEqual(l[0].name, "79a85a2c")
        XCTAssertEqual(l[1].name, "fix")
    }

    func testStatusMapping() {
        XCTAssertEqual(Session.mapStatus(state: nil, status: "busy"), .running)
        XCTAssertEqual(Session.mapStatus(state: nil, status: "waiting"), .waiting)
        XCTAssertEqual(Session.mapStatus(state: nil, status: "idle"), .idle)
        XCTAssertEqual(Session.mapStatus(state: "working", status: nil), .running)
        XCTAssertEqual(Session.mapStatus(state: "failed", status: nil), .error)
        XCTAssertEqual(Session.mapStatus(state: "somethingnew", status: nil), .idle)
        XCTAssertEqual(Session.mapStatus(state: nil, status: nil), .idle)
    }

    /// Verifiziert 16.09.2026 (2.1.273): ohne Titel heißen interaktive Sessions `<ordner>-<2 hex>`, bei jedem Start anders.
    func testAutoNameAndTitle() {
        XCTAssertTrue(Session.isAutoName("claude-agent-overview-ad", cwd: "/p/claude-agent-overview"))
        XCTAssertTrue(Session.isAutoName("homelab-72", cwd: "/p/homelab"))
        XCTAssertFalse(Session.isAutoName("homelab-fix", cwd: "/p/homelab"))
        XCTAssertFalse(Session.isAutoName("kadrell-adopt-spike", cwd: "/tmp/kadrell-spike"))
        XCTAssertEqual(Session(id: "abcdef12", cwd: "/p/homelab", startedAt: 0, sessionId: "x", name: "").title, "homelab · abcd")
        XCTAssertEqual(Session(id: "abcdef12", cwd: "/p/homelab", startedAt: 0, sessionId: "x", name: "Fix").title, "Fix")
    }

    func testSessionArgs() {
        XCTAssertEqual(ClaudeCLI.sessionArgs(sessionId: "u", hasTranscript: true), ["--resume", "u"])
        XCTAssertEqual(ClaudeCLI.sessionArgs(sessionId: "u", hasTranscript: false), ["--session-id", "u"])
        XCTAssertEqual(ClaudeCLI.launchArgs(allowBypass: false, mode: "", model: "", effort: ""), [])
        XCTAssertEqual(ClaudeCLI.launchArgs(allowBypass: true, mode: "bypassPermissions", model: "opus", effort: "high"),
                       ["--allow-dangerously-skip-permissions", "--permission-mode", "bypassPermissions", "--model", "opus", "--effort", "high"])
    }

    func testElapsed() {
        let now = Date()
        func s(_ ago: TimeInterval) -> Session { Session(id: "s", cwd: "/", startedAt: now.addingTimeInterval(-ago).timeIntervalSince1970 * 1000, sessionId: "s", name: "n") }
        XCTAssertEqual(s(42 * 60).elapsed(now: now), "42m")
        XCTAssertEqual(s(130 * 60).elapsed(now: now), "2h")
        XCTAssertEqual(s(2 * 86400).elapsed(now: now), "2d")
    }
}

@MainActor
final class SessionRegistryTests: XCTestCase {
    func agents(_ json: String) throws -> [Agent] { try Agent.decodeList(Data(json.utf8)) }

    /// Terminal ohne Claude merkt sich den Ordner der Shell über deren pid.
    func testShellCwdOfProcess() throws {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/bin/sleep")
        p.arguments = ["5"]
        p.currentDirectoryURL = URL(fileURLWithPath: "/private/tmp")
        try p.run()
        defer { p.terminate() }
        XCTAssertEqual(SessionRegistry.cwd(pid: p.processIdentifier), "/private/tmp")
        XCTAssertNil(SessionRegistry.cwd(pid: 999_999))
    }

    /// Live-Werte nur über die eigene pid: der gestoppte Hintergrund-Eintrag mit derselben sessionId zählt nicht,
    /// eine neue sessionId (nach `/clear`) wird übernommen, Auto-Namen überschreiben keinen Titel.
    func testMergeByOwnPid() throws {
        let stored = [Session(id: "a7adb9af", cwd: "/p/proj", startedAt: 1, sessionId: "old", name: "Fix"),
                      Session(id: "k2", cwd: "/p/proj", startedAt: 2, sessionId: "k2", name: "")]
        let list = try agents("""
        [{ "id": "a7adb9af", "cwd": "/p/proj", "kind": "background", "startedAt": 1, "sessionId": "old", "name": "Fix", "state": "done" },
         { "pid": 11, "cwd": "/p/proj", "kind": "interactive", "startedAt": 5, "sessionId": "new", "name": "proj-c6", "status": "busy" },
         { "pid": 99, "cwd": "/p/proj", "kind": "interactive", "startedAt": 5, "sessionId": "k2", "name": "Fremd", "status": "busy" }]
        """)
        let merged = SessionRegistry.merge(stored, agents: list, pids: [11: "a7adb9af"])
        XCTAssertEqual(merged[0].sessionId, "new")
        XCTAssertEqual(merged[0].name, "Fix")
        XCTAssertEqual(merged[0].status, .running)
        XCTAssertEqual(merged[0].pid, 11)
        XCTAssertNil(merged[1].pid)   // pid 99 gehört nicht Kadrell
        XCTAssertEqual(merged[1].status, .idle)
        XCTAssertEqual(merged[1].name, "")

        let titled = SessionRegistry.merge(stored, agents: try agents(#"[{"pid":11,"cwd":"/p/proj","kind":"interactive","startedAt":5,"sessionId":"new","name":"Neuer Titel","status":"idle"}]"#), pids: [11: "a7adb9af"])
        XCTAssertEqual(titled[0].name, "Neuer Titel")

        // Von Hand vergebener Name (F2) bleibt Titel, auch wenn Claude Code später umbenennt; alte JSON ohne Feld lädt.
        var manual = stored
        manual[0].customName = "Meins"
        let kept = SessionRegistry.merge(manual, agents: try agents(#"[{"pid":11,"cwd":"/p/proj","kind":"interactive","startedAt":5,"sessionId":"new","name":"Neuer Titel","status":"idle"}]"#), pids: [11: "a7adb9af"])
        XCTAssertEqual(kept[0].title, "Meins")
        let roundtrip = try JSONDecoder().decode([Session].self, from: JSONEncoder().encode(kept))
        XCTAssertEqual(roundtrip[0].title, "Meins")
        XCTAssertNil(try JSONDecoder().decode(Session.self, from: Data(#"{"id":"a","cwd":"/p","startedAt":1,"sessionId":"a","name":"x"}"#.utf8)).customName)
    }

    func testPersistsAddAndRemove() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)/sessions.json")
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: [:])
        let r = SessionRegistry(cli: cli, url: url)
        r.add(Session(id: "s1", cwd: "/p", startedAt: 1, sessionId: "s1", name: "", rawStatus: "busy", pid: 3))
        r.add(Session(id: "s2", cwd: "/p", startedAt: 2, sessionId: "s2", name: "Zwei"))
        r.remove(["s1"])
        let reloaded = SessionRegistry(cli: cli, url: url).sessions
        XCTAssertEqual(reloaded.map(\.id), ["s2"])
        XCTAssertEqual(reloaded[0].name, "Zwei")
        XCTAssertNil(reloaded[0].pid)
    }
}

final class UsageParsingTests: XCTestCase {
    func testParsesLimitsArray() {
        let json = """
        {"five_hour":{"utilization":42.0,"resets_at":"2026-09-15T16:00:00.984304+00:00"},
         "seven_day":{"utilization":73.0,"resets_at":"2026-09-16T01:00:00.984327+00:00"},
         "limits":[{"kind":"session","percent":42,"resets_at":"2026-09-15T16:00:00.984304+00:00"},
                   {"kind":"weekly_all","percent":73},
                   {"kind":"weekly_scoped","percent":48,"scope":{"model":{"display_name":"Fable"}}}]}
        """
        let u = Usage.parse(Data(json.utf8))
        XCTAssertEqual(u.session, 42)
        XCTAssertEqual(u.weekly, 73)
        XCTAssertEqual(u.fable, 48)
        XCTAssertNotNil(u.sessionResets)
    }

    func testFallsBackAndSurvivesGarbage() {
        let u = Usage.parse(Data("{\"five_hour\":{\"utilization\":10.4},\"seven_day\":{\"utilization\":\"x\"}}".utf8))
        XCTAssertEqual(u.session, 10)
        XCTAssertNil(u.weekly)
        XCTAssertNil(u.fable)
        XCTAssertEqual(Usage.parse(Data("not json".utf8)), .empty)
        XCTAssertEqual(Usage.parse(Data("[1,2]".utf8)), .empty)
    }
}

final class TranscriptTests: XCTestCase {
    /// Transcript: angeschnittene erste Zeile, Tool-Aufruf ohne Text und Sidechain-Antwort werden übersprungen.
    func testTranscriptLastText() {
        let jsonl = """
        ,"cut":"off"}
        {"type":"assistant","message":{"content":[{"type":"text","text":"Fix ist drin.\\n\\nSoll ich **pushen**?"}]}}
        {"type":"user","message":{"content":"ja"}}
        {"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"subagent"}]}}
        {"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash"}]}}
        """
        XCTAssertEqual(Transcript.lastText(jsonl: Data(jsonl.utf8)), "Fix ist drin. Soll ich pushen?")
        XCTAssertNil(Transcript.lastText(jsonl: Data("{\"type\":\"user\"}".utf8)))
    }

    /// Ersatztitel: Meta-Zeilen, Slash-Commands und Bild-Platzhalter fallen raus, lange Texte werden gekürzt.
    func testFirstPrompt() throws {
        let jsonl = """
        {"type":"user","isMeta":true,"message":{"content":"<local-command-caveat>x</local-command-caveat>"}}
        {"type":"user","message":{"content":"<command-name>/model</command-name>"}}
        {"type":"user","message":{"content":[{"type":"text","text":"[Image #1]  Umbenennen\\ngeht nicht immer, bitte prüfen und dann reparieren"},{"type":"image"}]}}
        {"type":"user","message":{"content":"zweite"}}
        """
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".jsonl")
        try Data(jsonl.utf8).write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        XCTAssertEqual(Transcript.firstPrompt(path: url.path, limit: 29), "Umbenennen geht nicht immer,…")
        XCTAssertEqual(Transcript.firstPrompt(path: url.path), "Umbenennen geht nicht immer, bitte prüfen und dann…")
        XCTAssertEqual(Session(id: "a", cwd: "/p/proj", startedAt: 1, sessionId: "a", name: "", firstPrompt: "Hallo").title, "Hallo")
        XCTAssertNil(Transcript.firstPrompt(path: "/gibt/es/nicht.jsonl"))
    }

    /// Worktree-Erkennung: Pfad-Felder und Bash-Kommandos aus `tool_use`, neueste zuerst, Sidechain fällt raus.
    func testToolCandidates() {
        let jsonl = """
        {"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls"}}]}}
        {"type":"assistant","isSidechain":true,"message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/sub/x.php"}}]}}
        {"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"relative.txt"}},{"type":"tool_use","name":"Edit","input":{"file_path":"/repo-wt-1/src/x.php"}}]}}
        {"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cd /repo-wt-1 && npm test"}}]}}
        """
        let candidates = Transcript.toolCandidates(jsonl: Data(jsonl.utf8))
        XCTAssertEqual(candidates, ["cd /repo-wt-1 && npm test", "/repo-wt-1/src/x.php", "ls"])
    }

    /// `refresh` liest bei gewachsener Datei nur die neuen Bytes: neuer Text/Tool-Aufruf setzt sich durch, wächst
    /// die Datei aber ohne neue Assistant-Zeile, bleiben Text und Kandidaten des Caches stehen statt zu verschwinden.
    func testRefreshReadsOnlyGrowthAndKeepsOldWhereNothingNew() throws {
        let path = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".jsonl")
        try Data((#"{"type":"assistant","message":{"content":[{"type":"text","text":"erste Antwort"}]}}"# + "\n").utf8).write(to: path)
        defer { try? FileManager.default.removeItem(at: path) }
        func size() -> UInt64 { UInt64((try? FileManager.default.attributesOfItem(atPath: path.path)[.size] as? Int) ?? 0) }
        let cache = ["s1": Transcript.Entry(path: path.path, size: size(), text: "erste Antwort", toolCandidates: ["ls"])]

        let h1 = try FileHandle(forWritingTo: path)
        h1.seekToEndOfFile()
        h1.write(Data((#"{"type":"assistant","message":{"content":[{"type":"text","text":"zweite Antwort"},{"type":"tool_use","name":"Bash","input":{"command":"pwd"}}]}}"# + "\n").utf8))
        try h1.close()
        let grown = Transcript.refresh(["s1"], cache: cache, wantText: true)
        XCTAssertEqual(grown["s1"]?.text, "zweite Antwort")
        XCTAssertEqual(grown["s1"]?.toolCandidates, ["pwd", "ls"])
        XCTAssertEqual(grown["s1"]?.size, size())

        let h2 = try FileHandle(forWritingTo: path)
        h2.seekToEndOfFile()
        h2.write(Data((#"{"type":"user","message":{"content":"ok"}}"# + "\n").utf8))
        try h2.close()
        let noNewAssistant = Transcript.refresh(["s1"], cache: grown, wantText: true)
        XCTAssertEqual(noNewAssistant["s1"]?.text, "zweite Antwort")
        XCTAssertEqual(noNewAssistant["s1"]?.toolCandidates, ["pwd", "ls"])
        XCTAssertEqual(noNewAssistant["s1"]?.size, size())

        // Unveränderte Größe: der Cache-Eintrag geht unverändert durch, ohne die Datei erneut zu lesen.
        let unchanged = Transcript.refresh(["s1"], cache: noNewAssistant, wantText: true)
        XCTAssertEqual(unchanged["s1"]?.text, "zweite Antwort")

        // `wantText: false` (Settings.showLastMessage aus): kein Text, Tool-Kandidaten bleiben für die Worktree-Erkennung.
        // Bekannter Pfad über eine leere Basis (size 0), damit der Test nicht den echten `~/.claude/projects`-Scan auslöst.
        let freshCache = ["s1": Transcript.Entry(path: path.path, size: 0, text: nil, toolCandidates: [])]
        let noText = Transcript.refresh(["s1"], cache: freshCache, wantText: false)
        XCTAssertNil(noText["s1"]?.text)
        XCTAssertEqual(noText["s1"]?.toolCandidates, ["pwd"])
    }
}
