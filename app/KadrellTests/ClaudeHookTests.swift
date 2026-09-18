import XCTest
@testable import Kadrell

final class ClaudeHookTests: XCTestCase {
    private func argv(_ json: String, env: [String: String] = [:], home: String = "/nonexistent") -> [String]? {
        ClaudeHook.statusArgv(json: Data(json.utf8), env: env, home: home)
    }

    func testEventsMapToStatus() {
        XCTAssertEqual(argv(#"{"hook_event_name":"SessionStart","session_id":"s1","session_source":"startup"}"#), ["status", "idle", "--session-id", "s1"])
        XCTAssertEqual(argv(#"{"hook_event_name":"UserPromptSubmit","session_id":"s1","prompt":"Bitte  die\nTests fixen"}"#),
                       ["status", "working", "--first-prompt", "Bitte die Tests fixen", "--session-id", "s1"])
        XCTAssertEqual(argv(#"{"hook_event_name":"PermissionRequest","session_id":"s1","tool_name":"Bash"}"#),
                       ["status", "waiting", "--waiting-for", "Bash", "--session-id", "s1"])
        XCTAssertEqual(argv(#"{"hook_event_name":"Notification","session_id":"s1","notification_type":"permission_prompt","message":"Darf ich?"}"#),
                       ["status", "waiting", "--waiting-for", "Darf ich?", "--session-id", "s1"])
        // idle_prompt (Claude ~60 s untätig): idle, nicht waiting – sonst kippt eine fertige Session zurück auf „wartet".
        XCTAssertEqual(argv(#"{"hook_event_name":"Notification","session_id":"s1","notification_type":"idle_prompt","message":"Claude is waiting for your input"}"#),
                       ["status", "idle", "--session-id", "s1"])
        XCTAssertEqual(argv(#"{"hook_event_name":"Stop","session_id":"s1","last_assistant_message":"**Fertig**, `x` gebaut."}"#),
                       ["status", "idle", "--message", "Fertig, x gebaut.", "--session-id", "s1"])
    }

    func testIgnoresSubagentsAndOtherEvents() {
        XCTAssertNil(argv(#"{"hook_event_name":"Stop","session_id":"s1","agent_id":"sub"}"#))
        XCTAssertNil(argv(#"{"hook_event_name":"PreToolUse","session_id":"s1"}"#))
        XCTAssertNil(argv(#"{"hook_event_name":"Notification","session_id":"s1","notification_type":"auth_success"}"#))
        XCTAssertNil(argv("{}"))
        XCTAssertNil(argv("kein json"))
    }

    /// Ein Slash-Command als erste Eingabe ist kein Ersatztitel, der Status kommt trotzdem an.
    func testPromptWithoutTitleValue() {
        XCTAssertEqual(argv(#"{"hook_event_name":"UserPromptSubmit","session_id":"s1","user_prompt":"<command-name>/clear</command-name>"}"#),
                       ["status", "working", "--session-id", "s1"])
    }

    func testTitleFromSessionFile() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("kadrell-hook-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir.appendingPathComponent("sessions"), withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let env = ["CLAUDE_PID": "777", "CLAUDE_CONFIG_DIR": dir.path]
        let file = dir.appendingPathComponent("sessions/777.json")
        try #"{"pid":777,"name":"Tests grün machen","nameSource":"haiku"}"#.write(to: file, atomically: true, encoding: .utf8)
        XCTAssertEqual(argv(#"{"hook_event_name":"SessionStart","session_id":"s1","cwd":"/p/repo"}"#, env: env),
                       ["status", "idle", "--title", "Tests grün machen", "--session-id", "s1"])
        try #"{"pid":777,"name":"repo-2f","nameSource":"derived"}"#.write(to: file, atomically: true, encoding: .utf8)
        XCTAssertEqual(argv(#"{"hook_event_name":"Stop","session_id":"s1","cwd":"/p/repo"}"#, env: env), ["status", "idle", "--session-id", "s1"])
        // Ohne `nameSource` (ältere CLI) entscheidet das Namensmuster.
        try #"{"pid":777,"name":"repo-2f"}"#.write(to: file, atomically: true, encoding: .utf8)
        XCTAssertEqual(argv(#"{"hook_event_name":"Stop","session_id":"s1","cwd":"/p/repo"}"#, env: env), ["status", "idle", "--session-id", "s1"])
        try #"{"pid":777,"name":"Echter Titel"}"#.write(to: file, atomically: true, encoding: .utf8)
        XCTAssertEqual(argv(#"{"hook_event_name":"Stop","session_id":"s1","cwd":"/p/repo"}"#, env: env), ["status", "idle", "--title", "Echter Titel", "--session-id", "s1"])
    }

    func testLaunchArgsCarryAllEventsAsHooks() throws {
        XCTAssertEqual(ClaudeHook.launchArgs.first, "--settings")
        let obj = try JSONSerialization.jsonObject(with: Data(ClaudeHook.launchArgs[1].utf8)) as! [String: Any]
        let hooks = obj["hooks"] as! [String: [[String: Any]]]
        XCTAssertEqual(Set(hooks.keys), Set(ClaudeHook.events))
        let command = (hooks["Stop"]![0]["hooks"] as! [[String: Any]])[0]["command"] as! String
        XCTAssertTrue(command.contains("\"$KADRELL\" hook claude"))
    }
}
