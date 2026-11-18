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
        (!text.is_empty()).then_some(text)
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
    fn write_args_hex_encode_value() {
        let args = write_args("svc", "a", "{\"x\":1}");
        assert_eq!(
            &args[..7],
            ["add-generic-password", "-U", "-s", "svc", "-a", "a", "-X"]
        );
        assert_eq!(args[7], "7b2278223a317d");
    }
}
