//! macOS: das `security`-CLI wie in `Keychain.swift`. Kein Security.framework, denn der Eintrag
//! `Claude Code-credentials` gehört `security`, nur dasselbe Binärprogramm bleibt ohne Nachfrage zugänglich.

use std::process::Command;

use super::SecretStore;

const SECURITY: &str = "/usr/bin/security";

pub struct SecuritySecretStore;

impl SecretStore for SecuritySecretStore {
    fn read(&self, service: &str, account: Option<&str>) -> Option<String> {
        let args = read_args(service, account);
        let out = Command::new(SECURITY).args(&args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!text.is_empty()).then(|| decode_security_hex(&text).unwrap_or(text))
    }

    fn write(&self, service: &str, account: &str, value: &str) -> bool {
        // Hex-kodiert als `-X`, damit beliebige Bytes (JSON mit Anführungszeichen) ohne Quoting durchkommen.
        // Das Hex steht kurz in der Prozessliste (nur derselbe Nutzer sieht sie), wie in der Swift-App.
        let args = write_args(service, account, value);
        Command::new(SECURITY)
            .args(&args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn delete(&self, service: &str, account: &str) {
        let _ = Command::new(SECURITY)
            .args(["delete-generic-password", "-s", service, "-a", account])
            .output();
    }
}

/// `security -w` gibt Daten, die es nicht für druckbar hält (Umlaute, Zeilenumbrüche), als Hex aus. Wandelt so eine
/// Ausgabe zurück: nur wenn sie reines Hex gerader Länge ist, gültiges UTF-8 ergibt und dabei ein Nicht-ASCII- oder
/// Steuerzeichen enthält (nur dann hätte `security` überhaupt hex-kodiert). Ein gewöhnlicher Wert wie `abcd` oder
/// `41424344` bleibt, wie er ist. Die Swift-App liest die Ausgabe ohne diesen Schritt.
fn decode_security_hex(text: &str) -> Option<String> {
    if text.len() < 2 || !text.len().is_multiple_of(2) || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok()).collect();
    let decoded = String::from_utf8(bytes?).ok()?;
    decoded.chars().any(|c| !c.is_ascii() || c.is_control()).then_some(decoded)
}

fn read_args(service: &str, account: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = ["find-generic-password", "-s", service, "-w"]
        .map(String::from)
        .to_vec();
    if let Some(account) = account {
        args.push("-a".into());
        args.push(account.into());
    }
    args
}

fn write_args(service: &str, account: &str, value: &str) -> Vec<String> {
    let hex: String = value.bytes().map(|b| format!("{b:02x}")).collect();
    [
        "add-generic-password",
        "-U",
        "-s",
        service,
        "-a",
        account,
        "-X",
        &hex,
    ]
    .map(String::from)
    .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_args_match_swift() {
        assert_eq!(
            read_args("svc", None),
            ["find-generic-password", "-s", "svc", "-w"]
        );
        assert_eq!(
            read_args("svc", Some("a")),
            ["find-generic-password", "-s", "svc", "-w", "-a", "a"]
        );
    }

    #[test]
    fn hex_output_is_decoded_only_when_security_would_have_encoded() {
        let hex = |t: &str| t.bytes().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(decode_security_hex(&hex("Müller ö")).as_deref(), Some("Müller ö"));
        assert_eq!(decode_security_hex(&hex("zwei\nZeilen")).as_deref(), Some("zwei\nZeilen"), "Zeilenumbruch");
        assert_eq!(decode_security_hex("abcd"), None, "gewöhnlicher Wert aus Hexziffern");
        assert_eq!(decode_security_hex("41424344"), None, "dekodiert zu reinem ASCII, also war es Klartext");
        assert_eq!(decode_security_hex("c3bc7"), None, "ungerade Länge");
        assert_eq!(decode_security_hex("zz"), None);
        assert_eq!(decode_security_hex("ff"), None, "kein gültiges UTF-8");
        assert_eq!(decode_security_hex(""), None);
        assert_eq!(decode_security_hex(BLOCK_FOR_TEST), None, "ein JSON-Block ist kein Hex");
    }

    #[test]
    fn write_args_hex_encode_value() {
        let args = write_args("svc", "a", "{\"x\":1}");
        assert_eq!(
            &args[..7],
            ["add-generic-password", "-U", "-s", "svc", "-a", "a", "-X"]
        );
        assert_eq!(args[7], "7b2278223a317d");
    }

    /// Echter Schlüsselbund, aber nur Wegwerf-Einträge mit eigenem Dienstnamen (`de.malura.kadrell.rust.test.<pid>-<n>`),
    /// nie `Claude Code-credentials`. Der Eintrag wird beim Fallenlassen gelöscht, auch nach einem fehlgeschlagenen Test.
    struct Scratch {
        service: String,
    }

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let service = format!("de.malura.kadrell.rust.test.{}-{tag}", std::process::id());
            assert_ne!(service, super::super::LIVE_SERVICE, "nie den echten Eintrag benutzen");
            assert!(service.starts_with("de.malura.kadrell.rust.test."));
            Scratch { service }
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            SecuritySecretStore.delete(&self.service, "konto-a");
            SecuritySecretStore.delete(&self.service, "konto-b");
        }
    }

    const BLOCK_FOR_TEST: &str = r#"{\"a\":1}"#;

    const BLOCK: &str = r#"{"claudeAiOauth":{"accessToken":"sk-test-123","refreshToken":"r","expiresAt":1,"scopes":["a","b"]}}"#;

    #[test]
    fn keychain_roundtrip_with_json_value() {
        let s = Scratch::new("roundtrip");
        let store = SecuritySecretStore;
        assert_eq!(store.read(&s.service, Some("konto-a")), None, "vorher nichts");
        assert!(store.write(&s.service, "konto-a", BLOCK));
        assert_eq!(store.read(&s.service, Some("konto-a")).as_deref(), Some(BLOCK));
        assert_eq!(store.read(&s.service, None).as_deref(), Some(BLOCK), "ohne Konto: der Eintrag des Dienstes");
        assert_eq!(store.read(&s.service, Some("konto-b")), None, "anderes Konto: nichts");
    }

    #[test]
    fn keychain_write_updates_an_existing_entry() {
        let s = Scratch::new("update");
        let store = SecuritySecretStore;
        assert!(store.write(&s.service, "konto-a", "eins"));
        assert!(store.write(&s.service, "konto-a", "zwei"), "-U überschreibt statt zu scheitern");
        assert_eq!(store.read(&s.service, Some("konto-a")).as_deref(), Some("zwei"));
        assert!(store.write(&s.service, "konto-b", "drei"));
        assert_eq!(store.read(&s.service, Some("konto-a")).as_deref(), Some("zwei"), "zweites Konto stört das erste nicht");
    }

    #[test]
    fn keychain_delete_removes_only_that_entry() {
        let s = Scratch::new("delete");
        let store = SecuritySecretStore;
        store.write(&s.service, "konto-a", "eins");
        store.write(&s.service, "konto-b", "zwei");
        store.delete(&s.service, "konto-a");
        assert_eq!(store.read(&s.service, Some("konto-a")), None);
        assert_eq!(store.read(&s.service, Some("konto-b")).as_deref(), Some("zwei"));
        store.delete(&s.service, "konto-a"); // schon weg: kein Fehler, keine Ausnahme
    }

    /// `security -w` gibt Daten, die es nicht für druckbar hält (Umlaute, Zeilenumbrüche), als Hex aus. Ein OAuth-Block ist
    /// ASCII; stünde dort doch einmal ein Nicht-ASCII-Zeichen, darf `read` nicht stillschweigend Hex liefern.
    #[test]
    fn keychain_value_with_non_ascii_comes_back_unchanged() {
        let s = Scratch::new("unicode");
        let store = SecuritySecretStore;
        let value = r#"{"name":"Müller \u00e4 ö"}"#;
        assert!(store.write(&s.service, "konto-a", value));
        assert_eq!(store.read(&s.service, Some("konto-a")).as_deref(), Some(value));
    }
}
