//! `login_shell_env` gegen die echte Login-Shell des Macs. Verglichen werden nur Namen, nie Werte: die Umgebung des
//! Nutzers enthält Schlüssel und Tokens, die in keinem Testlog landen dürfen.
#![cfg(target_os = "macos")]

use kadrell_claude::cli::{login_shell_env, parse_env0, SystemRunner};
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// Rohausgabe von `zsh -lc env -0` (ohne den `printf '\0'`-Trenner der Produktion), wie die Shell sie liefert.
fn raw_login_env() -> String {
    let out = Command::new("/bin/zsh").args(["-lc", "env -0"]).output().expect("zsh startet");
    assert!(out.status.success(), "zsh -lc env -0 endet mit {:?}", out.status.code());
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn names(env: &BTreeMap<String, String>) -> BTreeSet<&str> {
    env.keys().map(String::as_str).collect()
}

#[test]
fn login_shell_env_has_the_same_names_as_a_plain_login_shell() {
    let real = login_shell_env(&SystemRunner, BTreeMap::new());
    // Gleiche Bedeutung wie `printf '\0'; exec env -0`: Profilausgabe vor dem ersten NUL fällt weg.
    let expected: BTreeMap<String, String> = parse_env0(&format!("\0{}", raw_login_env())).into_iter().collect();
    // `_` und `SHLVL` ändern sich je nach Aufrufform, nur ihre Namen zählen, nicht ihr Vorhandensein.
    let volatile = ["_", "SHLVL", "OLDPWD"];
    let got: BTreeSet<&str> = names(&real).into_iter().filter(|n| !volatile.contains(n)).collect();
    let want: BTreeSet<&str> = names(&expected).into_iter().filter(|n| !volatile.contains(n)).collect();
    assert_eq!(got, want, "Namen weichen ab: nur links {:?}, nur rechts {:?}", got.difference(&want).collect::<Vec<_>>(), want.difference(&got).collect::<Vec<_>>());
}

#[test]
fn login_shell_env_provides_what_claude_needs() {
    let real = login_shell_env(&SystemRunner, BTreeMap::new());
    for must in ["PATH", "HOME", "USER", "SHELL"] {
        assert!(real.contains_key(must), "{must} fehlt in der Login-Umgebung");
    }
    assert!(real["PATH"].split(':').count() >= 3, "PATH wirkt zu kurz für eine Login-Shell");
    assert!(real.keys().all(|k| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')), "ungültiger Variablenname");
}

#[test]
fn login_shell_env_keeps_base_entries_the_shell_does_not_set() {
    let base: BTreeMap<String, String> = [("KADRELL_TEST_NUR_IN_BASE".to_string(), "1".to_string())].into();
    let real = login_shell_env(&SystemRunner, base);
    assert_eq!(real.get("KADRELL_TEST_NUR_IN_BASE").map(String::as_str), Some("1"));
    assert!(real.contains_key("PATH"));
}

#[test]
fn multiline_values_do_not_invent_variables() {
    // Ein Wert mit Zeilenumbruch und `NAME=`-Zeile darin muss ein einziger Eintrag bleiben.
    let out = Command::new("/bin/zsh")
        .args(["-lc", "printf '\\0'; exec env -0"])
        .env("KADRELL_TEST_MULTI", "a\nKADRELL_TEST_FALSCH=b")
        .output()
        .expect("zsh startet");
    let parsed: BTreeMap<String, String> = parse_env0(&String::from_utf8_lossy(&out.stdout)).into_iter().collect();
    assert_eq!(parsed.get("KADRELL_TEST_MULTI").map(String::as_str), Some("a\nKADRELL_TEST_FALSCH=b"));
    assert!(!parsed.contains_key("KADRELL_TEST_FALSCH"));
}
