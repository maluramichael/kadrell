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

    /// `add`/`remove` während eines laufenden Polls: die neue Session bleibt, die entfernte kommt nicht zurück.
    func testPollKeepsChangesMadeDuringItsAwaits() async throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: ["CLAUDE_CONFIG_DIR": dir.path])
        let r = SessionRegistry(cli: cli, url: dir.appendingPathComponent("sessions.json"))
        r.add(Session(id: "s1", cwd: dir.path, startedAt: 1, sessionId: "s1", name: "Eins"))
        r.add(Session(id: "s3", cwd: dir.path, startedAt: 3, sessionId: "s3", name: "Drei"))
        var started = false
        r.pids = { started = true; return [:] }
        let poll = Task { await r.pollNow() }
        while !started { await Task.yield() }
        r.add(Session(id: "s2", cwd: dir.path, startedAt: 2, sessionId: "s2", name: "Zwei"))
        r.remove(["s3"])
        await poll.value
        XCTAssertEqual(r.sessions.map(\.id), ["s1", "s2"])
        XCTAssertEqual(SessionRegistry(cli: cli, url: r.url).sessions.map(\.id), ["s1", "s2"])
    }

    /// Rang 5/58: eine kaputte sessions.json startet leer statt abzustürzen, das Original bleibt als
    /// `.corrupt-<datum>.json` liegen und wird vom nächsten `add` nicht überschrieben.
    func testCorruptSessionsFileIsQuarantinedNotOverwritten() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("sessions.json")
        try Data("{kaputt".utf8).write(to: url)
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: [:])

        let r = SessionRegistry(cli: cli, url: url)
        XCTAssertTrue(r.sessions.isEmpty)
        XCTAssertNotNil(r.lastError)
        XCTAssertTrue((try FileManager.default.contentsOfDirectory(atPath: dir.path)).contains { $0.hasPrefix("sessions.corrupt-") })

        r.add(Session(id: "s1", cwd: "/p", startedAt: 1, sessionId: "s1", name: ""))
        XCTAssertTrue(FileManager.default.fileExists(atPath: url.path))
        XCTAssertTrue((try FileManager.default.contentsOfDirectory(atPath: dir.path)).contains { $0.hasPrefix("sessions.corrupt-") })
    }

    /// Nacktes Array (Format vor der Schema-Version) lädt weiter wie gehabt.
    func testLegacyBareArraySessionsFileLoads() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)/sessions.json")
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(#"[{"id":"s1","cwd":"/p","startedAt":1,"sessionId":"s1","name":"alt"}]"#.utf8).write(to: url)
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: [:])

        XCTAssertEqual(SessionRegistry(cli: cli, url: url).sessions.map(\.id), ["s1"])
    }

    /// #775: eine doppelte Id in sessions.json (Handbearbeitung, Übernahme) ließ `WorkspaceView.reload` bei
    /// jedem Start abstürzen. Erste gewinnt, `add` ignoriert eine schon vorhandene Id statt zu duplizieren.
    func testDuplicateIdIsDedupedOnLoadAndRejectedOnAdd() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let url = dir.appendingPathComponent("sessions.json")
        try Data(#"[{"id":"s1","cwd":"/p","startedAt":1,"sessionId":"s1","name":"eins"},{"id":"s1","cwd":"/q","startedAt":2,"sessionId":"s1","name":"zwei"}]"#.utf8).write(to: url)
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: [:])

        let r = SessionRegistry(cli: cli, url: url)
        XCTAssertEqual(r.sessions.map(\.id), ["s1"])
        XCTAssertEqual(r.sessions[0].name, "eins")

        r.add(Session(id: "s1", cwd: "/x", startedAt: 3, sessionId: "s1", name: "drei"))
        XCTAssertEqual(r.sessions.count, 1)
        XCTAssertEqual(r.sessions[0].name, "eins")
    }

    /// #-Fund: der Schließen-Dialog verspricht "Konversation bleibt erhalten", also muss die sessionId
    /// nach `remove` in Kadrells eigenen Daten weiter auffindbar sein.
    func testRemoveArchivesClosedSessionsCappedAndPersisted() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-tests-\(UUID().uuidString)/sessions.json")
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: [:])
        let r = SessionRegistry(cli: cli, url: url)
        r.add(Session(id: "s1", cwd: "/p", startedAt: 1, sessionId: "conv-1", name: "Eins"))
        r.remove(["s1"])

        XCTAssertEqual(r.closed.map(\.session.sessionId), ["conv-1"])
        let reloaded = SessionRegistry(cli: cli, url: url)
        XCTAssertEqual(reloaded.closed.map(\.session.sessionId), ["conv-1"])

        // 60 weitere einzeln schließen: höchstens 50 bleiben, die neuesten zuerst
        for i in 2...61 {
            r.add(Session(id: "s\(i)", cwd: "/p", startedAt: 1, sessionId: "conv-\(i)", name: ""))
            r.remove(["s\(i)"])
        }
        XCTAssertEqual(r.closed.count, 50)
        XCTAssertEqual(r.closed.first?.session.sessionId, "conv-61")
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
    /// Transcripts liegen unter dem Datenordner von Claude Code, mit `CLAUDE_CONFIG_DIR` also nicht in `~/.claude`.
    func testPathFollowsConfigDir() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-config-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("projects/-p-proj"), withIntermediateDirectories: true)
        try "{}".write(to: dir.appendingPathComponent("projects/-p-proj/abc.jsonl"), atomically: true, encoding: .utf8)
        let cli = ClaudeCLI(binary: "/usr/bin/false", environment: ["CLAUDE_CONFIG_DIR": dir.path])
        XCTAssertEqual(Transcript.path(sessionId: "abc", configDir: cli.configDir), dir.path + "/projects/-p-proj/abc.jsonl")
        XCTAssertNil(Transcript.path(sessionId: "fehlt", configDir: cli.configDir))
    }

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
}
