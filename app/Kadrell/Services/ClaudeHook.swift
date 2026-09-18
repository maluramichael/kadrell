import Foundation

/// `kadrell hook claude`: Claude Code ruft seine Hooks mit JSON auf stdin auf (`hook_event_name`, `session_id`, …)
/// und vererbt ihnen die Umgebung der Kachel (`KADRELL_SOCKET`, `KADRELL_SESSION_KEY`, `CLAUDE_PID`). Hier wird
/// das Ereignis auf `kadrell status …` abgebildet, die Session meldet sich damit selbst, statt dass Kadrell ihre
/// Dateien pollt. nil = Ereignis ist für den Status uninteressant.
enum ClaudeHook {
    /// Ereignisse, die Kadrell jedem Claude-Prozess als Hooks mitgibt.
    static let events = ["SessionStart", "UserPromptSubmit", "PermissionRequest", "Notification", "Stop"]
    /// Die Hook-Zeile: außerhalb von Kadrell (kein Socket) stumm, sonst dieses Binary. `$KADRELL` und
    /// `$KADRELL_SOCKET` stehen in der Umgebung jeder Kachel, claude vererbt sie an den Hook.
    static let command = #"[ -n "$KADRELL_SOCKET" ] && exec "$KADRELL" hook claude || exit 0"#

    /// `--settings <json>` für den Start jedes Claude-Prozesses: die Hooks gelten nur für diesen Prozess, ohne Eintrag
    /// in `~/.claude/settings.json`. Dass claude Hooks aus `--settings` lädt, ist nicht dokumentiert, aber mit
    /// 2.1.274 verifiziert (`docs/kadrell-verifikation.md`): fällt es weg, greift wieder der Poll über `Agent.local`.
    static let launchArgs: [String] = {
        let hook: [String: Any] = ["type": "command", "command": command, "timeout": 5]
        let hooks = Dictionary(uniqueKeysWithValues: events.map { ($0, [["matcher": "", "hooks": [hook]]]) })
        let data = try? JSONSerialization.data(withJSONObject: ["hooks": hooks], options: [.sortedKeys, .withoutEscapingSlashes])
        return ["--settings", String(decoding: data ?? Data(), as: UTF8.self)]
    }()
    /// `notification_type`-Werte, bei denen Claude blockiert auf den Nutzer wartet (Freigabe, Frage). `idle_prompt`
    /// gehört bewusst NICHT dazu: das feuert ~60 s nachdem Claude fertig ist und niemand tippt, die Session ist dann
    /// untätig (idle), nicht blockiert – siehe eigener Zweig unten.
    static let waitingNotifications: Set = ["permission_prompt", "agent_needs_input", "elicitation_dialog"]

    static func statusArgv(json: Data, env: [String: String], home: String = NSHomeDirectory()) -> [String]? {
        guard let obj = try? JSONSerialization.jsonObject(with: json) as? [String: Any],
              obj["agent_id"] == nil, let event = obj["hook_event_name"] as? String else { return nil }
        let title = { titleArgs(env: env, cwd: obj["cwd"] as? String ?? "", home: home) }
        var args: [String]
        switch event {
        case "SessionStart": args = ["idle"] + title()
        case "UserPromptSubmit":
            args = ["working"]
            if let p = (obj["prompt"] ?? obj["user_prompt"]) as? String, let flat = Transcript.flatPrompt(p) { args += ["--first-prompt", flat] }
        case "PermissionRequest": args = ["waiting"] + flag("--waiting-for", obj["tool_name"])
        case "Notification":
            guard let type = obj["notification_type"] as? String else { return nil }
            if type == "idle_prompt" { args = ["idle"] + title() }
            else if waitingNotifications.contains(type) { args = ["waiting"] + flag("--waiting-for", obj["message"]) }
            else { return nil }
        case "Stop":
            args = ["idle"] + title()
            if let m = obj["last_assistant_message"] as? String, let flat = Transcript.flatAnswer(m) { args += ["--message", flat] }
        default: return nil
        }
        return ["status"] + args + flag("--session-id", obj["session_id"])
    }

    private static func flag(_ name: String, _ value: Any?) -> [String] {
        guard let v = value as? String, !v.isEmpty else { return [] }
        return [name, v]
    }

    /// Titel aus `<configDir>/sessions/<CLAUDE_PID>.json`, nur ein echter: `nameSource` „derived“ (bzw. ohne das
    /// Feld das Muster `<ordner>-<2 hex>`) ist der automatische Name, den Kadrell wie bisher verwirft.
    static func titleArgs(env: [String: String], cwd: String, home: String) -> [String] {
        guard let pid = env["CLAUDE_PID"] else { return [] }
        let dir = env["CLAUDE_CONFIG_DIR"] ?? home + "/.claude"
        guard let data = FileManager.default.contents(atPath: "\(dir)/sessions/\(pid).json"),
              let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let name = obj["name"] as? String, !name.isEmpty else { return [] }
        let derived = (obj["nameSource"] as? String).map { $0 == "derived" } ?? Session.isAutoName(name, cwd: cwd)
        return derived ? [] : ["--title", name]
    }
}
