//! Update-Prüfung (Swift: `Services/UpdateChecker.swift`): nur die Prüfung und eine Pille in der Leiste, nie Download
//! oder Installation. Das Netz wird nur berührt, wenn die Einstellung „Nach Updates suchen“ es erlaubt; Tests laufen mit
//! einem Fake statt mit dem Netz.

use std::time::Duration;

use kadrell_accounts::{HttpClient, HttpRequest, Method};
use kadrell_claude::cli::parse_version;
use serde_json::Value;

/// `/download/latest.json`, wie `tools/release.sh` es neben jedem Download ablegt.
pub const MANIFEST_URL: &str = "https://kadrell.malura.de/download/latest.json";

/// Wie oft nach dem Start erneut geprüft wird.
pub const INTERVAL: Duration = Duration::from_secs(86_400);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateManifest {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub notes: Vec<String>,
}

/// Liest das Manifest; `version` und `url` sind Pflicht, der Rest darf fehlen.
pub fn parse_manifest(body: &str) -> Result<UpdateManifest, String> {
    let v: Value = serde_json::from_str(body).map_err(|e| format!("latest.json: {e}"))?;
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let notes = v.get("notes").and_then(Value::as_array).map(|a| a.iter().filter_map(|n| n.as_str().map(str::to_string)).collect());
    Ok(UpdateManifest {
        version: text("version").ok_or("latest.json: version fehlt")?,
        url: text("url").ok_or("latest.json: url fehlt")?,
        sha256: text("sha256").unwrap_or_default(),
        notes: notes.unwrap_or_default(),
    })
}

/// `a` neuer als `b`, beide „x.y.z“. Ein unbekanntes Format gilt als nicht neuer.
pub fn is_newer(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(va), Some(vb)) => vb < va,
        _ => false,
    }
}

/// Eine Vorabversion (0.x) steht in keinem Download: `latest.json` bezieht sich auf die veröffentlichte Mac-App, ein
/// Vergleich damit wäre immer „neuer“ und bliebe ein falscher Alarm.
pub fn is_prerelease(current: &str) -> bool {
    parse_version(current).is_some_and(|(major, _, _)| major == 0)
}

/// Eine Prüfung. `allowed` ist die Einstellung `checkForUpdates`: ohne sie bleibt das Netz unberührt und es gibt kein
/// Update. `Ok(None)`: aktuell. `Err`: Netz oder Antwort unbrauchbar (der Aufrufer behält den bisherigen Stand).
pub fn check(client: &dyn HttpClient, allowed: bool, current: &str) -> Result<Option<UpdateManifest>, String> {
    if !allowed || is_prerelease(current) {
        return Ok(None);
    }
    let request = HttpRequest { method: Method::Get, url: MANIFEST_URL.into(), headers: Vec::new(), body: None, timeout: Duration::from_secs(15) };
    let response = client.send(&request).map_err(|e| e.to_string())?;
    if response.status != 200 {
        return Err(format!("latest.json: HTTP {}", response.status));
    }
    let manifest = parse_manifest(&String::from_utf8_lossy(&response.body))?;
    Ok(is_newer(&manifest.version, current).then_some(manifest))
}

/// Text des Dialogs hinter der Pille: Changelog-Zeilen der neuen Version, eine je Zeile.
pub fn dialog_text(m: &UpdateManifest) -> String {
    m.notes.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use kadrell_accounts::{HttpError, HttpResponse};
    use std::sync::Mutex;

    struct Fake {
        reply: Result<HttpResponse, HttpError>,
        calls: Mutex<Vec<String>>,
    }

    impl Fake {
        fn ok(body: &str) -> Fake {
            Fake { reply: Ok(HttpResponse { status: 200, headers: Vec::new(), body: body.as_bytes().to_vec() }), calls: Mutex::new(Vec::new()) }
        }
    }

    impl HttpClient for Fake {
        fn send(&self, req: &HttpRequest) -> Result<HttpResponse, HttpError> {
            self.calls.lock().unwrap().push(req.url.clone());
            self.reply.clone()
        }
    }

    const BODY: &str = r#"{"version":"1.90.0","url":"https://kadrell.malura.de/download/Kadrell-1.90.0.dmg","sha256":"ab","notes":["Feature: neu","Fix: alt"]}"#;

    #[test]
    fn version_comparison_follows_numbers_not_text() {
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("1.9.9", "1.10.0"));
        assert!(!is_newer("1.2.3", "1.2.3"));
        assert!(!is_newer("neu", "1.0.0"), "unbekanntes Format gilt als nicht neuer");
    }

    #[test]
    fn manifest_needs_version_and_url() {
        let m = parse_manifest(BODY).unwrap();
        assert_eq!((m.version.as_str(), m.notes.len()), ("1.90.0", 2));
        assert!(parse_manifest(r#"{"version":"1.0.0"}"#).is_err());
        assert!(parse_manifest("kein json").is_err());
        assert_eq!(parse_manifest(r#"{"version":"1.0.0","url":"u"}"#).unwrap().notes, Vec::<String>::new());
    }

    #[test]
    fn a_newer_manifest_is_reported_and_an_equal_one_is_not() {
        let fake = Fake::ok(BODY);
        assert_eq!(check(&fake, true, "1.89.0").unwrap().map(|m| m.version), Some("1.90.0".to_string()));
        assert_eq!(check(&fake, true, "1.90.0").unwrap(), None);
        assert_eq!(fake.calls.lock().unwrap().as_slice(), [MANIFEST_URL, MANIFEST_URL]);
    }

    #[test]
    fn the_network_stays_untouched_when_the_setting_forbids_it() {
        let fake = Fake::ok(BODY);
        assert_eq!(check(&fake, false, "0.1.0").unwrap(), None);
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn a_prerelease_build_never_asks() {
        let fake = Fake::ok(BODY);
        assert!(is_prerelease("0.1.0") && !is_prerelease("1.0.0") && !is_prerelease("kaputt"));
        assert_eq!(check(&fake, true, "0.1.0").unwrap(), None);
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn failures_are_errors_not_updates() {
        let down = Fake { reply: Err(HttpError("offline".into())), calls: Mutex::new(Vec::new()) };
        assert!(check(&down, true, "1.0.0").is_err());
        let missing = Fake { reply: Ok(HttpResponse { status: 404, ..HttpResponse::default() }), calls: Mutex::new(Vec::new()) };
        assert!(check(&missing, true, "1.0.0").unwrap_err().contains("404"));
        assert!(check(&Fake::ok("{}"), true, "1.0.0").is_err());
    }

    #[test]
    fn dialog_text_lists_the_notes() {
        assert_eq!(dialog_text(&parse_manifest(BODY).unwrap()), "Feature: neu\nFix: alt");
    }
}
