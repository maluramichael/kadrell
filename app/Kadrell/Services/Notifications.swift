import UserNotifications

/// Systembenachrichtigung „<Gruppe> › <Titel> wartet“, wenn du gerade woanders bist (Einstellung „Systembenachrichtigungen“).
/// Rechte fragt macOS erst bei der ersten tatsächlich fälligen Benachrichtigung ab, nicht beim App-Start.
@MainActor
enum Notifications {
    enum Level: String, CaseIterable {
        case off, waiting, all
        var title: String {
            switch self {
            case .off: String(localized: "aus")
            case .waiting: String(localized: "nur wartet")
            case .all: String(localized: "wartet und fertig")
            }
        }
    }

    /// Klick auf eine Benachrichtigung, vom AppDelegate gesetzt: fokussiert die Session.
    static var onSelect: ((String) -> Void)?

    static func setup() { UNUserNotificationCenter.current().delegate = Delegate.shared }

    /// Reine Entscheidung ohne UN-Zugriff, testbar: bei welchem Level welches Ereignis eine Benachrichtigung auslöst.
    static func shouldNotify(level: Level, waiting: Bool) -> Bool { level != .off && (waiting || level == .all) }

    /// Gleiche `identifier` je Session: eine neue Benachrichtigung ersetzt die vorige statt sich zu stapeln.
    /// `threadIdentifier` gruppiert alle Benachrichtigungen einer Gruppe im Mitteilungszentrum.
    static func notify(sessionKey: String, group: String, title: String, message: String?, waiting: Bool) {
        guard shouldNotify(level: Settings.notifications, waiting: waiting) else { return }
        Task {
            let center = UNUserNotificationCenter.current()
            if await center.notificationSettings().authorizationStatus == .notDetermined {
                _ = try? await center.requestAuthorization(options: [.alert, .sound])
            }
            let content = UNMutableNotificationContent()
            content.title = "\(group) › \(title)"
            content.body = message ?? (waiting ? String(localized: "wartet auf dich") : String(localized: "ist fertig"))
            content.threadIdentifier = group
            content.userInfo = ["sessionKey": sessionKey]
            try? await center.add(UNNotificationRequest(identifier: sessionKey, content: content, trigger: nil))
        }
    }

    /// Kein gespeicherter Zustand, nur Weiterleitung an `Notifications.onSelect`: unbedenklich Sendable.
    private final class Delegate: NSObject, UNUserNotificationCenterDelegate, @unchecked Sendable {
        static let shared = Delegate()
        func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping () -> Void) {
            if let key = response.notification.request.content.userInfo["sessionKey"] as? String {
                Task { @MainActor in Notifications.onSelect?(key) }
            }
            completionHandler()
        }
        /// Banner auch zeigen, während Kadrell selbst gerade aktiv ist (andere Session/anderes Fenster im Fokus).
        func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
            completionHandler([.banner, .sound])
        }
    }
}
