import AppKit

/// Was dich auf eine Session aufmerksam macht, für alle Fenster gemeinsam: Marke „neu“, Dock-Badge und -Hüpfen,
/// Klang, VoiceOver-Ansage, Systembenachrichtigung, Hook `session-focus` und die einmaligen Tipps in der Leiste.
/// Seiteneffekte laufen über die Closures, damit Tests die Zustandsmaschine ohne App prüfen können.
@MainActor
final class AttentionTracker {
    private let defaults: UserDefaults
    /// Fertig gewordene oder wartende Sessions, die du noch nicht angesehen hast. Weg erst, wenn du sie anklickst
    /// oder fokussierst; übersteht einen Neustart.
    private(set) var unseen: Set<String> {
        didSet { if unseen != oldValue { defaults.set(Array(unseen), forKey: "sessions.unseen") } }
    }
    /// Einmaliger Tipp in der Leiste (zweite Session, Bedeutung von Gelb), siehe `showTip`.
    var tip: String? { didSet { onTip(tip) } }
    /// Wartende Sessions beim letzten Abgleich: neu dazugekommene lösen `requestAttention` aus.
    private var lastWaitingIds: Set<String> = []
    /// Status angehängter Sessions beim letzten Abgleich: Wechsel spielen Sound und lassen den Punkt im Baum aufblitzen.
    private var lastStatuses: [String: SessionStatus] = [:]
    /// Session, für die zuletzt `session-focus` gefeuert hat.
    private var hookFocus: String?
    /// Aktiver Worktree dieser Session beim letzten `session-focus` (siehe `Session.activeWorktree`). Wechselt er,
    /// obwohl die Session dieselbe bleibt, feuert der Hook erneut, sonst bekäme ein Hook-Skript den Wechsel nie mit.
    private var hookFocusWorktree: String?

    var onTip: (String?) -> Void = { _ in }
    /// nil, bis claude aufgelöst ist: bis dahin feuert kein Hook.
    var fireFocusHook: ((Session) -> Void)?
    var notify: (_ key: String, _ session: Session, _ waiting: Bool) -> Void = { _, _, _ in }
    var play: (Feedback.Sound) -> Void = { Feedback.play($0) }
    var setBadge: (Int) -> Void = { NSApp.dockTile.badgeLabel = $0 == 0 ? nil : "\($0)" }
    var requestAttention: () -> Void = { NSApp.requestUserAttention(.informationalRequest) }
    /// VoiceOver-Ankündigung, unabhängig vom aktuellen Fokus (Muster aus PaletteWindow.move).
    var announce: (String) -> Void = { text in
        NSAccessibility.post(element: NSApp as Any, notification: .announcementRequested,
                             userInfo: [.announcement: text, .priority: NSAccessibilityPriorityLevel.medium.rawValue])
    }

    init(defaults: UserDefaults = Profile.defaults) {
        self.defaults = defaults
        unseen = Set(defaults.stringArray(forKey: "sessions.unseen") ?? [])
    }

    /// true, wenn die Marke da war: dann muss die Anzeige nachziehen.
    @discardableResult func markSeen(_ key: String) -> Bool { unseen.remove(key) != nil }

    func prune(to sessions: [String: Session]) { unseen = unseen.filter { sessions[$0] != nil } }

    /// Abgleich nach jeder Änderung. `waiting` in Baumreihenfolge, `statuses` nur angehängter Sessions, `isKey`:
    /// das aktuelle Fenster ist vorn. Liefert die Übergänge, die der Baum aufblitzen lässt.
    func update(sessions: [String: Session], waiting: [String], statuses: [String: SessionStatus],
                focused: String?, isKey: Bool) -> (waiting: Set<String>, done: Set<String>) {
        fireHookIfFocusChanged(focused.flatMap { sessions[$0] })
        let waitingSet = Set(waiting)
        // Wartend oder fertig, aber noch nicht angesehen: beides zusammen, sonst verschwinden fertige Sessions aus dem Badge.
        setBadge(waitingSet.union(unseen).count)
        // Neu dazugekommene wartende Session, Fenster nicht im Vordergrund: kurz im Dock hüpfen, ohne Notification-Rechte.
        if !waitingSet.subtracting(lastWaitingIds).isEmpty, !isKey { requestAttention() }
        lastWaitingIds = waitingSet
        let changed = Feedback.transitions(from: lastStatuses, to: statuses)
        lastStatuses = statuses
        handleTransitions(changed, sessions: sessions, statuses: statuses) { $0 != focused || !isKey }
        return changed
    }

    private func fireHookIfFocusChanged(_ s: Session?) {
        guard let fireFocusHook, let s, s.id != hookFocus || s.activeWorktree != hookFocusWorktree else { return }
        hookFocus = s.id
        hookFocusWorktree = s.activeWorktree
        fireFocusHook(s)
    }

    /// Klang und Marke „neu“ nur für das, was man gerade nicht sieht (`notLooking`): andere Session oder Fenster im Hintergrund.
    private func handleTransitions(_ changed: (waiting: Set<String>, done: Set<String>), sessions: [String: Session],
                                   statuses: [String: SessionStatus], notLooking: (String) -> Bool) {
        // Einmaliger Tipp bei der allerersten wartenden Session überhaupt (Kanboard #14): sonst bleibt Gelb unerklärt.
        if !changed.waiting.isEmpty {
            showTipOnce("tip.waiting.shown", String(localized: "Gelb heißt: Claude wartet auf dich. ⌥N springt zur nächsten wartenden Session."))
        }
        // VoiceOver bekommt sonst nichts vom Kernnutzen der App mit: welche Session gerade auf einen wartet oder fertig ist.
        for id in changed.waiting { announce(String(localized: "\(sessions[id]?.title ?? "") wartet")) }
        for id in changed.done { announce(String(localized: "\(sessions[id]?.title ?? "") fertig")) }
        if changed.waiting.contains(where: notLooking) { play(.waiting) } else if changed.done.contains(where: notLooking) { play(.done) }
        for (ids, waiting) in [(changed.waiting, true), (changed.done, false)] {
            for key in ids.filter(notLooking) { if let s = sessions[key] { notify(key, s, waiting) } }
        }
        let fresh = changed.waiting.union(changed.done).filter(notLooking)
        if !fresh.isSubset(of: unseen) { unseen.formUnion(fresh) }
        // Wieder am Arbeiten: die Marke gilt der letzten Antwort, nicht der laufenden.
        unseen.subtract(unseen.filter { statuses[$0] == .running })
    }

    /// Tipp nur beim ersten Mal je `key`, danach nie wieder.
    func showTipOnce(_ key: String, _ text: String) {
        guard !defaults.bool(forKey: key) else { return }
        defaults.set(true, forKey: key)
        showTip(text)
    }

    /// Einmaliger Tipp in der Leiste (Kanboard #14): verschwindet nach 12 s von selbst oder per Klick darauf.
    private func showTip(_ text: String) {
        tip = text
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(12))
            guard let self, self.tip == text else { return }
            self.tip = nil
        }
    }
}
