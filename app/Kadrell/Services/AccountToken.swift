import Foundation
import os

/// Access-Token eines gespeicherten, gerade nicht aktiven Accounts, damit Kadrell dessen Nutzung abfragen kann, ohne
/// umzuschalten. Läuft der Token ab, wird er wie in der `claude`-CLI über den Refresh-Token erneuert: gleiche Adresse,
/// gleiche Client-ID, gleiche Scopes. Jeder Refresh liefert einen neuen Refresh-Token, der alte ist danach verbraucht;
/// der Aufrufer muss den neuen Block deshalb sofort in den Slot zurückschreiben.
enum AccountToken {
    static let log = Logger(subsystem: "de.malura.kadrell", category: "accounts")
    /// Aus der `claude`-CLI (Produktions-Konfiguration `TOKEN_URL` und `CLIENT_ID`).
    static let tokenURL = URL(string: "https://platform.claude.com/v1/oauth/token")
    static let clientId = "9d1c250a-e61b-44d9-88ed-5944d1962f5e"
    /// So lange vor Ablauf schon erneuern, damit der Token die Abfrage sicher überlebt.
    static let margin: TimeInterval = 300

    /// Braucht der Block einen Refresh? Ohne `expiresAt` (altes Format) nein: dann den Token einfach versuchen.
    static func needsRefresh(_ oauth: [String: Any], now: Date = Date()) -> Bool {
        guard let ms = (oauth["expiresAt"] as? NSNumber)?.doubleValue else { return false }
        return Date(timeIntervalSince1970: ms / 1000).timeIntervalSince(now) < margin
    }

    /// Request wie `nte()` in der CLI: JSON mit grant_type, refresh_token, client_id und den gespeicherten Scopes.
    static func refreshRequest(_ oauth: [String: Any]) -> URLRequest? {
        guard let url = tokenURL, let refresh = oauth["refreshToken"] as? String, !refresh.isEmpty else { return nil }
        var body: [String: Any] = ["grant_type": "refresh_token", "refresh_token": refresh, "client_id": clientId]
        if let scopes = oauth["scopes"] as? [String], !scopes.isEmpty { body["scope"] = scopes.joined(separator: " ") }
        guard let data = try? JSONSerialization.data(withJSONObject: body) else { return nil }
        var req = URLRequest(url: url, timeoutInterval: 30)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        // Ohne eigenen User-Agent lehnt Cloudflare manche Clients mit 403 (1010) ab.
        req.setValue("Kadrell", forHTTPHeaderField: "User-Agent")
        req.httpBody = data
        return req
    }

    /// Antwort des Token-Endpunkts in den gespeicherten Block übernehmen; alle übrigen Felder (Abo, Tier) bleiben.
    /// Fehlt `refresh_token`, gilt wie in der CLI der alte weiter. Nil, wenn kein neuer Access-Token drinsteht.
    static func applying(_ response: Data, to oauth: [String: Any], now: Date = Date()) -> [String: Any]? {
        guard let root = (try? JSONSerialization.jsonObject(with: response)) as? [String: Any],
              let access = root["access_token"] as? String, !access.isEmpty,
              let expiresIn = (root["expires_in"] as? NSNumber)?.doubleValue else { return nil }
        var out = oauth
        out["accessToken"] = access
        out["expiresAt"] = Int64((now.timeIntervalSince1970 + expiresIn) * 1000)
        if let refresh = root["refresh_token"] as? String, !refresh.isEmpty { out["refreshToken"] = refresh }
        if let rExp = (root["refresh_token_expires_in"] as? NSNumber)?.doubleValue {
            out["refreshTokenExpiresAt"] = Int64((now.timeIntervalSince1970 + rExp) * 1000)
        }
        if let scope = root["scope"] as? String { out["scopes"] = scope.split(separator: " ").map(String.init) }
        return out
    }

    /// Schickt den Refresh-Request ab; die Antwort bei HTTP 200, sonst nil. Der gespeicherte Block bleibt dann
    /// unverändert. Request und Antwort statt des Blocks selbst, weil `[String: Any]` nicht über Actor-Grenzen darf.
    static func send(_ req: URLRequest) async -> Data? {
        do {
            let (data, resp) = try await URLSession.shared.data(for: req)
            let status = (resp as? HTTPURLResponse)?.statusCode ?? -1
            guard status == 200 else { log.warning("token refresh: HTTP \(status, privacy: .public)"); return nil }
            return data
        } catch {
            log.warning("token refresh: \(String(describing: error), privacy: .public)")
            return nil
        }
    }
}
