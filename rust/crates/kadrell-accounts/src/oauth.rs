//! OAuth-Block eines Kontos und Token-Refresh (Port von `AccountToken.swift` und den JSON-Helfern des `AccountService`).
//!
//! Läuft der Token eines inaktiven Kontos ab, wird er wie in der `claude`-CLI über den Refresh-Token erneuert: gleiche
//! Adresse, gleiche Client-ID, gleiche Scopes. Jeder Refresh liefert einen neuen Refresh-Token, der alte ist danach
//! verbraucht; der Aufrufer muss den neuen Block deshalb sofort in den Slot zurückschreiben.

use std::time::Duration;

use serde_json::{Map, Value};

use crate::http::{HttpClient, HttpRequest, Method};

pub type Obj = Map<String, Value>;

/// Aus der `claude`-CLI (Produktions-Konfiguration `TOKEN_URL` und `CLIENT_ID`).
pub const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
pub(crate) const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// So lange vor Ablauf schon erneuern, damit der Token die Abfrage sicher überlebt (Sekunden).
pub(crate) const MARGIN: f64 = 300.0;
/// Ohne eigenen User-Agent lehnt Cloudflare manche Clients mit 403 (1010) ab.
pub(crate) const USER_AGENT: &str = "Kadrell";

/// Der `claudeAiOauth`-Block aus einem vollständigen Credential-JSON.
pub fn oauth_from(raw: &str) -> Option<Obj> {
    match object_from(raw)?.get("claudeAiOauth")? {
        Value::Object(o) => Some(o.clone()),
        _ => None,
    }
}

pub fn object_from(raw: &str) -> Option<Obj> {
    match serde_json::from_str::<Value>(raw).ok()? {
        Value::Object(o) => Some(o),
        _ => None,
    }
}

pub fn string_from(object: &Obj) -> String {
    Value::Object(object.clone()).to_string()
}

pub(crate) fn text<'a>(obj: &'a Obj, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

/// Braucht der Block einen Refresh? Ohne `expiresAt` (altes Format) nein: dann den Token einfach versuchen.
pub fn needs_refresh(oauth: &Obj, now: f64) -> bool {
    match oauth.get("expiresAt").and_then(Value::as_f64) {
        Some(ms) => ms / 1000.0 - now < MARGIN,
        None => false,
    }
}

/// Request wie `nte()` in der CLI: JSON mit grant_type, refresh_token, client_id und den gespeicherten Scopes.
pub fn refresh_request(oauth: &Obj) -> Option<HttpRequest> {
    let refresh = text(oauth, "refreshToken").filter(|r| !r.is_empty())?;
    let mut body = Obj::new();
    body.insert("grant_type".into(), "refresh_token".into());
    body.insert("refresh_token".into(), refresh.into());
    body.insert("client_id".into(), CLIENT_ID.into());
    let scopes: Vec<&str> = oauth
        .get("scopes")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !scopes.is_empty() {
        body.insert("scope".into(), scopes.join(" ").into());
    }
    Some(HttpRequest {
        method: Method::Post,
        url: TOKEN_URL.into(),
        headers: vec![
            ("Content-Type".into(), "application/json".into()),
            ("User-Agent".into(), USER_AGENT.into()),
        ],
        body: Some(Value::Object(body).to_string().into_bytes()),
        timeout: Duration::from_secs(30),
    })
}

/// Antwort des Token-Endpunkts in den gespeicherten Block übernehmen; alle übrigen Felder (Abo, Tier) bleiben.
/// Fehlt `refresh_token`, gilt wie in der CLI der alte weiter. `None`, wenn kein neuer Access-Token drinsteht.
pub fn applying(response: &[u8], oauth: &Obj, now: f64) -> Option<Obj> {
    let Value::Object(root) = serde_json::from_slice::<Value>(response).ok()? else {
        return None;
    };
    let access = text(&root, "access_token").filter(|a| !a.is_empty())?;
    let expires_in = root.get("expires_in").and_then(Value::as_f64)?;
    let mut out = oauth.clone();
    out.insert("accessToken".into(), access.into());
    out.insert("expiresAt".into(), millis(now + expires_in));
    if let Some(refresh) = text(&root, "refresh_token").filter(|r| !r.is_empty()) {
        out.insert("refreshToken".into(), refresh.into());
    }
    if let Some(r_exp) = root.get("refresh_token_expires_in").and_then(Value::as_f64) {
        out.insert("refreshTokenExpiresAt".into(), millis(now + r_exp));
    }
    if let Some(scope) = text(&root, "scope") {
        let scopes: Vec<Value> = scope
            .split(' ')
            .filter(|s| !s.is_empty())
            .map(Value::from)
            .collect();
        out.insert("scopes".into(), Value::Array(scopes));
    }
    Some(out)
}

fn millis(seconds: f64) -> Value {
    Value::from((seconds * 1000.0) as i64)
}

/// Schickt den Refresh-Request ab; die Antwort bei HTTP 200, sonst `None`. Der gespeicherte Block bleibt dann
/// unverändert. Das Log nennt nur den Status, nie Token.
pub fn send(http: &dyn HttpClient, req: &HttpRequest) -> Option<Vec<u8>> {
    match http.send(req) {
        Ok(resp) if resp.status == 200 => Some(resp.body),
        Ok(resp) => {
            log::warn!("token refresh: HTTP {}", resp.status);
            None
        }
        Err(e) => {
            log::warn!("token refresh: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: f64 = 1_000_000.0;

    fn block(expires_in: f64) -> Obj {
        let v = json!({"accessToken": "old", "refreshToken": "r-old", "expiresAt": ((NOW + expires_in) * 1000.0) as i64,
            "scopes": ["user:inference", "user:profile"], "subscriptionType": "max", "rateLimitTier": "tier"});
        let Value::Object(o) = v else { unreachable!() };
        o
    }

    #[test]
    fn needs_refresh_only_close_to_expiry() {
        assert!(!needs_refresh(&block(3600.0), NOW));
        assert!(needs_refresh(&block(60.0), NOW), "innerhalb der Marge");
        assert!(needs_refresh(&block(-60.0), NOW), "schon abgelaufen");
        let mut no_expiry = Obj::new();
        no_expiry.insert("accessToken".into(), "x".into());
        assert!(
            !needs_refresh(&no_expiry, NOW),
            "ohne expiresAt einfach versuchen"
        );
    }

    #[test]
    fn refresh_request_matches_cli() {
        let req = refresh_request(&block(0.0)).unwrap();
        assert_eq!(req.url, "https://platform.claude.com/v1/oauth/token");
        assert_eq!(req.method, Method::Post);
        assert_eq!(req.header("User-Agent"), Some("Kadrell"));
        assert_eq!(req.header("content-type"), Some("application/json"));
        let body: Value = serde_json::from_slice(req.body.as_ref().unwrap()).unwrap();
        assert_eq!(
            body,
            json!({"grant_type": "refresh_token", "refresh_token": "r-old",
                "client_id": "9d1c250a-e61b-44d9-88ed-5944d1962f5e", "scope": "user:inference user:profile"})
        );
    }

    #[test]
    fn refresh_request_needs_refresh_token() {
        let mut o = Obj::new();
        o.insert("accessToken".into(), "x".into());
        assert!(refresh_request(&o).is_none());
    }

    /// Neuer Access- und Refresh-Token samt Ablauf, Abo und Tier bleiben stehen.
    #[test]
    fn applying_response_keeps_other_fields() {
        let response = br#"{"access_token":"new","refresh_token":"r-new","expires_in":28800,"refresh_token_expires_in":86400,"scope":"user:inference"}"#;
        let out = applying(response, &block(0.0), NOW).unwrap();
        assert_eq!(out["accessToken"], "new");
        assert_eq!(out["refreshToken"], "r-new");
        assert_eq!(
            out["expiresAt"].as_i64(),
            Some(((NOW + 28800.0) * 1000.0) as i64)
        );
        assert_eq!(
            out["refreshTokenExpiresAt"].as_i64(),
            Some(((NOW + 86400.0) * 1000.0) as i64)
        );
        assert_eq!(out["scopes"], json!(["user:inference"]));
        assert_eq!(out["subscriptionType"], "max");
        assert_eq!(out["rateLimitTier"], "tier");
        assert!(!needs_refresh(&out, NOW));
    }

    /// Ohne neuen Refresh-Token gilt wie in der CLI der alte weiter.
    #[test]
    fn applying_keeps_refresh_token_when_missing() {
        let out = applying(
            br#"{"access_token":"new","expires_in":3600}"#,
            &block(0.0),
            NOW,
        )
        .unwrap();
        assert_eq!(out["refreshToken"], "r-old");
    }

    #[test]
    fn applying_rejects_errors() {
        assert!(applying(br#"{"error":"invalid_grant"}"#, &block(0.0), NOW).is_none());
        assert!(applying(b"kaputt", &block(0.0), NOW).is_none());
    }
}
