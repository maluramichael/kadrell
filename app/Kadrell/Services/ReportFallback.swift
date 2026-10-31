import Foundation

/// Abgleich für Kacheln, die sich per Hook melden (`SessionRegistry.report`). Claude Code feuert bei einem Abbruch durch
/// den Nutzer (Esc, ⌃C, auch mitten in einem Werkzeug) keinen `Stop`-Hook und auch kein späteres `idle_prompt`
/// (Hooks-Doku, mit 2.1.314 geprüft, `docs/kadrell-verifikation.md`): die gemeldete Kachel bliebe ewig „arbeitet“.
/// `sessions/<pid>.json` dagegen springt beim Abbruch sofort auf `idle` und trägt in `statusUpdatedAt`, wann. Gelesen
/// wird die Datei nur für gemeldete Kacheln, die gerade nicht untätig sind, also ein paar hundert Byte je Poll.
enum ReportFallback {
    /// So lange darf eine Meldung „arbeitet“ ohne passenden Stand in der Datei bleiben: Claude schreibt `busy` im selben
    /// Moment, in dem es den `UserPromptSubmit`-Hook startet. Steht danach noch das alte `idle` da, kam die Eingabe nie
    /// bei Claude an (z. B. Esc, während Hooks die Eingabe noch prüfen).
    static let grace: Double = 5_000

    /// Neuer Rohstatus samt `waitingFor` für eine gemeldete Kachel, nil = die Meldung gilt weiter. Alle Zeiten in ms
    /// seit 1970. Ohne `statusUpdatedAt` (ältere CLI) bleibt es bei der Meldung.
    static func correction(current: SessionStatus, reportedAt: Double, file: Agent?, now: Double) -> (status: String, waitingFor: String?)? {
        guard current != .idle, let file, let raw = file.status, let written = file.statusUpdatedAt else { return nil }
        let status = Session.mapStatus(state: nil, status: raw)
        guard status != current else { return nil }
        // Die Datei ist neuer als die letzte Meldung: Abbruch (idle), Freigabe erteilt (busy) oder abgelehnt (idle).
        if written > reportedAt { return (raw, status == .waiting ? file.waitingFor : nil) }
        // Gemeldet „arbeitet“, die Datei steht aber noch auf dem `idle` von davor: die Runde hat nie begonnen.
        if current == .running, status == .idle, now - reportedAt > grace { return (raw, nil) }
        return nil
    }
}
