import Foundation

/// Erkennt Kommandos aus dem Bash-Tool, die in einer Session noch laufen („1 shell still running“ in Claude Code).
/// Claude Code startet jedes Bash-Kommando als eigenen Kindprozess seiner selbst, immer als Login-Shell, die zuerst
/// einen Schnappschuss der Umgebung einliest (`/bin/zsh -c source ~/.claude/shell-snapshots/snapshot-…`). Genau daran
/// hängt die Erkennung: die anderen Kinder eines Claude-Prozesses (MCP-Server über npx/uvx/ssh/node, `caffeinate`)
/// tragen den Schnappschuss nicht.
enum BackgroundShells {
    /// Teil der Kommandozeile jeder Shell, die das Bash-Tool startet.
    static let marker = "/shell-snapshots/snapshot-"

    /// pids der Claude-Prozesse, unter denen noch mindestens ein solches Kommando läuft.
    /// `nonisolated`: liest nur Prozessinfos, läuft im Poll abseits des Main-Threads.
    nonisolated static func running(pids: [Int]) -> Set<Int> {
        Set(pids.filter { pid in children(of: pid_t(pid)).contains { isToolShell(pid: $0) } })
    }

    /// Direkte Kindprozesse, wie `pgrep -P`. Ohne Puffer nennt `proc_listchildpids` die Obergrenze (alle Prozesse
    /// des Systems), mit Puffer die Zahl der geschriebenen Einträge, nicht wie sonst üblich die Bytes.
    nonisolated static func children(of pid: pid_t) -> [pid_t] {
        let capacity = proc_listchildpids(pid, nil, 0)
        guard capacity > 0 else { return [] }
        var buf = [pid_t](repeating: 0, count: Int(capacity))
        let written = proc_listchildpids(pid, &buf, Int32(MemoryLayout<pid_t>.size) * capacity)
        guard written > 0 else { return [] }
        return Array(buf.prefix(min(Int(written), buf.count))).filter { $0 > 0 }
    }

    /// Shell-Binary zuerst (ein Syscall), die teurere Kommandozeile nur für Shells: MCP-Server sind meist node,
    /// Python oder ssh und fallen schon hier raus.
    nonisolated private static func isToolShell(pid: pid_t) -> Bool {
        var path = [CChar](repeating: 0, count: Int(MAXPATHLEN))
        guard proc_pidpath(pid, &path, UInt32(MAXPATHLEN)) > 0 else { return false }
        let name = URL(fileURLWithPath: String(cString: path)).lastPathComponent
        guard name.hasSuffix("sh") else { return false }
        return arguments(of: pid)?.contains(marker) == true
    }

    /// Kommandozeile eines Prozesses (`KERN_PROCARGS2`), wie sie `ps -o args=` zeigt. Nur für eigene Prozesse lesbar,
    /// fremde liefern nil. Die einzelnen Argumente trennt ein NUL, hier reicht der Rohtext am Stück.
    nonisolated static func arguments(of pid: pid_t) -> String? {
        var mib = [CTL_KERN, KERN_PROCARGS2, pid]
        var size = 0
        guard sysctl(&mib, 3, nil, &size, nil, 0) == 0, size > 0 else { return nil }
        var buf = [UInt8](repeating: 0, count: size)
        guard sysctl(&mib, 3, &buf, &size, nil, 0) == 0 else { return nil }
        return String(decoding: buf.prefix(size).map { $0 == 0 ? UInt8(ascii: " ") : $0 }, as: UTF8.self)
    }
}
