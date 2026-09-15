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
        "name": "export", "status": "waiting", "waitingFor": "input needed" },
      { "id": "6e07ff2a", "cwd": "/y", "kind": "background", "startedAt": 1787083409084, "sessionId": "6e07ff2a-0a44-47ed-826b-42c7be0e2d9f",
        "name": "Docker", "state": "done" }
    ]
    """

    func testDecodeAgentsJSON() throws {
        let list = try Session.decodeList(Data(json.utf8))
        XCTAssertEqual(list.count, 5)
        XCTAssertEqual(list[0].shortId, "0ace1aab")
        XCTAssertEqual(list[0].id, "0ace1aab")   // kurze Id als Schlüssel
        XCTAssertEqual(list[1].id, "b746c5bb-ecfe-45d8-b268-7df9b23b5e44")   // ohne kurze Id: sessionId
        XCTAssertTrue(list[0].isBackground)
        XCTAssertFalse(list[0].canAttach)   // blocked ohne pid: Prozess weg
        XCTAssertTrue(list[0].isStale)
        XCTAssertTrue(list[2].canAttach)
        XCTAssertFalse(list[2].isStale)
        XCTAssertNil(list[1].shortId)
        XCTAssertTrue(list[1].isInteractive)
        XCTAssertFalse(list[1].canAttach)
        XCTAssertEqual(list[1].pid, 5395)
        XCTAssertEqual(list[3].waitingFor, "input needed")
        XCTAssertTrue(list[4].isDone)
        XCTAssertFalse(list[4].canAttach)
    }

    /// `done` heißt nur „Antwort fertig“: mit `pid` lebt der Prozess und `attach` klappt (verifiziert 15.09.2026, 2.1.273).
    func testDoneWithPidIsAttachable() throws {
        let json = """
        [{ "pid": 72412, "id": "68788841", "cwd": "/p", "kind": "background", "startedAt": 1,
           "sessionId": "68788841-0000-0000-0000-000000000000", "name": "probe", "status": "idle", "state": "done" }]
        """
        let s = try Session.decodeList(Data(json.utf8))[0]
        XCTAssertFalse(s.isDone)
        XCTAssertFalse(s.isStale)
        XCTAssertTrue(s.canAttach)
    }

    func testStatusMapping() throws {
        let list = try Session.decodeList(Data(json.utf8))
        XCTAssertEqual(list[0].status, .waiting)   // blocked
        XCTAssertEqual(list[1].status, .idle)      // idle
        XCTAssertEqual(list[2].status, .running)   // busy hat Vorrang vor working
        XCTAssertEqual(list[3].status, .waiting)   // waiting
        XCTAssertEqual(list[4].status, .idle)      // done
        XCTAssertEqual(Session.mapStatus(state: "working", status: nil), .running)
        XCTAssertEqual(Session.mapStatus(state: "stopped", status: nil), .idle)
        XCTAssertEqual(Session.mapStatus(state: "failed", status: nil), .error)
        XCTAssertEqual(Session.mapStatus(state: "errored", status: nil), .error)
        XCTAssertEqual(Session.mapStatus(state: "active", status: nil), .running)
        XCTAssertEqual(Session.mapStatus(state: "somethingnew", status: nil), .idle)
        XCTAssertEqual(Session.mapStatus(state: nil, status: nil), .idle)
    }

    func testDuplicateSessionIdsCollapse() throws {
        let dup = """
        [
          { "pid": 1, "cwd": "/a", "kind": "interactive", "startedAt": 1, "sessionId": "same", "name": "x", "status": "idle" },
          { "pid": 2, "id": "abcdef01", "cwd": "/a", "kind": "background", "startedAt": 1, "sessionId": "same", "name": "x", "state": "working" }
        ]
        """
        let list = try Session.decodeList(Data(dup.utf8))
        XCTAssertEqual(list.count, 1)
        XCTAssertEqual(list[0].shortId, "abcdef01")
    }

    func testParseBackgroundedId() {
        let out = """
        warning: --bg manages the session id; ignoring --session-id
        Starting background service…
        backgrounded · 86f99758 · kadrell-spike
          claude attach 86f99758    open in this terminal
        """
        XCTAssertEqual(ClaudeCLI.parseBackgroundedId(out), "86f99758")
        XCTAssertNil(ClaudeCLI.parseBackgroundedId("nothing"))
    }

    func testShortNameAndElapsed() {
        XCTAssertEqual(ClaudeCLI.shortName(prompt: String(repeating: "a", count: 60), cwd: "/x/y", counter: 1).count, 48)
        XCTAssertEqual(ClaudeCLI.shortName(prompt: "  ", cwd: "/x/proj", counter: 3), "proj-3")
        let now = Date()
        let s = Session(shortId: nil, cwd: "/", kind: "background", startedAt: now.addingTimeInterval(-42 * 60).timeIntervalSince1970 * 1000, sessionId: "s", name: "n")
        XCTAssertEqual(s.elapsed(now: now), "42m")
        let h = Session(shortId: nil, cwd: "/", kind: "background", startedAt: now.addingTimeInterval(-130 * 60).timeIntervalSince1970 * 1000, sessionId: "s", name: "n")
        XCTAssertEqual(h.elapsed(now: now), "2h")
        let d = Session(shortId: nil, cwd: "/", kind: "background", startedAt: now.addingTimeInterval(-2 * 86400).timeIntervalSince1970 * 1000, sessionId: "s", name: "n")
        XCTAssertEqual(d.elapsed(now: now), "2d")
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

final class SessionDuplicateTests: XCTestCase {
    func s(_ id: String, _ name: String, cwd: String = "/p/a", at: Double) -> Session {
        Session(shortId: id, cwd: cwd, kind: "background", startedAt: at, sessionId: id + "-uuid", name: name)
    }
    func testKeepsNewestPerTitleAndFolder() {
        let d = Session.duplicates(in: [s("a1", "fix", at: 1), s("a2", "fix", at: 3), s("a3", "fix", at: 2),
                                        s("b1", "fix", cwd: "/p/b", at: 1), s("c1", "c1", at: 1), s("c2", "c2", at: 2)])
        XCTAssertEqual(Set(d.map(\.id)), ["a1", "a3"])   // a2 ist die neueste; anderer Ordner und unbenannte bleiben
    }
}

final class SessionTolerantDecodeTests: XCTestCase {
    func testEntryWithoutNameDoesNotBreakTheList() throws {
        let json = #"[{"id":"79a85a2c","cwd":"/p","kind":"background","startedAt":1,"sessionId":"79a85a2c-x","state":"blocked"},{"id":"aa","cwd":"/p","kind":"background","startedAt":2,"sessionId":"aa-x","name":"fix","pid":5,"status":"idle"}]"#
        let l = try Session.decodeList(Data(json.utf8))
        XCTAssertEqual(l.count, 2)
        XCTAssertEqual(l[0].name, "79a85a2c")
        XCTAssertEqual(l[0].title, "p · 79a8")
        XCTAssertEqual(l[1].name, "fix")
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
}
