import Foundation

/// Generische Schlüsselbund-Einträge über das `security`-CLI, denselben Weg wie `UsageService.token()` und die
/// `claude`-CLI selbst nutzen. Kein Security.framework: der Eintrag `Claude Code-credentials` gehört `security`,
/// und nur über dasselbe Binärprogramm bleibt der Zugriff ohne Nachfrage.
enum Keychain {
    /// Wert eines Eintrags, nil bei fehlendem Eintrag oder Fehler. `account` grenzt zusätzlich ein.
    static func read(service: String, account: String? = nil) async -> String? {
        var args = ["find-generic-password", "-s", service, "-w"]
        if let account { args += ["-a", account] }
        guard let r = try? await ProcessRunner.run("/usr/bin/security", args), r.status == 0 else { return nil }
        let s = r.output.trimmingCharacters(in: .whitespacesAndNewlines)
        return s.isEmpty ? nil : s
    }

    /// Legt an oder überschreibt (`-U`). Der Wert geht hex-kodiert als `-X`, damit beliebige Bytes (JSON mit
    /// Anführungszeichen) ohne Quoting durchkommen.
    /// ponytail: Hex steht kurz in der Prozessliste (nur derselbe Nutzer sieht sie), reicht lokal; ein Wechsel auf
    /// `security -i` über stdin wäre der Upgrade-Pfad, falls das je stört.
    @discardableResult
    static func write(service: String, account: String, value: String) async -> Bool {
        let hex = value.utf8.map { String(format: "%02x", $0) }.joined()
        let r = try? await ProcessRunner.run("/usr/bin/security", ["add-generic-password", "-U", "-s", service, "-a", account, "-X", hex])
        return r?.status == 0
    }

    static func delete(service: String, account: String) async {
        _ = try? await ProcessRunner.run("/usr/bin/security", ["delete-generic-password", "-s", service, "-a", account])
    }
}
