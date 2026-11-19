//! Kadrell: Einstiegspunkt. Ein Binary für App und Kommandozeile:
//!
//! - `kadrell` (mit oder ohne `--profile <name>`): startet die App.
//! - `kadrell hook claude`: Hook von Claude Code, meldet den Status an die laufende App.
//! - `kadrell ext-host <ordner>`: Host-Prozess der Lua-Extensions (läuft in `kadrell-ext`).
//! - `kadrell <befehl>`: Steuerkanal der laufenden App (`ls`, `new`, `send`, `capture`, ...), startet sie bei Bedarf.

use std::collections::HashMap;

use kadrell_core::Profile;

/// Profilname aus `--profile <name>` oder `KADRELL_PROFILE`. Die Argumente bleiben unangetastet: `--profile` und
/// `--extensions-dir` sind nur Startargumente der App (Swift: `Profile.name`), in einem Befehl sind sie eine unbekannte
/// Option, in `send`/`new`-Text und nach `--` gewöhnlicher Text.
fn profile_name(args: &[String]) -> Option<String> {
    Profile::name_from_args(args, std::env::var(kadrell_core::profile::PROFILE_ENV).ok().as_deref())
}

/// Hook-Abbildung: Hook-JSON von Claude Code auf die Argumente von `kadrell status`.
fn hook_map(json: &[u8]) -> Option<Vec<String>> {
    let env: HashMap<String, String> = std::env::vars().collect();
    kadrell_claude::hook::status_argv(json, &env, &kadrell_claude::paths::home_dir())
}

/// Die App braucht kein Konsolenfenster: unter Windows vom Terminal lösen, sofern nicht zum Debuggen gewünscht.
#[cfg(windows)]
fn detach_console() {
    if std::env::var_os("KADRELL_CONSOLE").is_some() || std::env::var_os("KADRELL_DEV_KEYLOG").is_some() {
        return;
    }
    // SAFETY: `FreeConsole` hat keine Voraussetzungen; ohne Konsole tut es nichts.
    unsafe { windows_sys::Win32::System::Console::FreeConsole() };
}

#[cfg(not(windows))]
fn detach_console() {}

/// `kadrell ext-host <ordner>`: der Helper einer Lua-Extension; Protokoll auf stdin und stdout. Nur bei genau zwei
/// Argumenten, sonst ist `ext-host` wie in Swift ein unbekannter Befehl.
fn ext_host_dir(args: &[String]) -> Option<&str> {
    match args {
        [cmd, dir] if cmd == "ext-host" => Some(dir),
        _ => None,
    }
}

/// Kommandozeile oder App? Das erste Argument ohne „-“ (oder `-h`/`--help`) ist ein Befehl; Optionen mit „-“ (Xcode,
/// LaunchServices, `--profile tmp`) starten die App (Swift: `main.swift`).
fn runs_cli(first: &str) -> bool {
    !first.starts_with('-') || first == "-h" || first == "--help"
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(dir) = ext_host_dir(&args) {
        return kadrell_ext::run_host(std::path::Path::new(dir));
    }
    let profile = profile_name(&args);
    let code = match args.first().map(String::as_str) {
        Some(first) if runs_cli(first) => kadrell_control::client::main_cli(&args, profile, &hook_map),
        _ => {
            // Vor allen Threads: die Marker einer umgebenden Claude-Session erben die Kacheln sonst.
            kadrell_term::strip_inherited_env();
            detach_console();
            kadrell_app::run(profile)
        }
    };
    std::process::exit(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_run_the_cli_and_options_start_the_app() {
        for command in ["ls", "new", "help", "hook", "kill-server"] {
            assert!(runs_cli(command), "{command}");
        }
        assert!(runs_cli("-h") && runs_cli("--help"));
        for option in ["--profile", "-psn_0_12345", "-NSDocumentRevisionsDebugMode", "--verbose"] {
            assert!(!runs_cli(option), "{option}");
        }
    }

    #[test]
    fn profile_arguments_stay_in_the_arguments() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(profile_name(&args(&["--profile", "tmp"])).as_deref(), Some("tmp"));
        assert_eq!(profile_name(&args(&["--profile", "tmp", "ls"])).as_deref(), Some("tmp"));
        // Als Befehl ist `--profile` keiner: die App startet, `ls` danach wird nicht ausgeführt.
        assert!(!runs_cli("--profile"));
        assert!(!runs_cli("--extensions-dir"));
    }

    #[test]
    fn ext_host_needs_exactly_two_arguments() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(ext_host_dir(&args(&["ext-host", "/x"])), Some("/x"));
        assert_eq!(ext_host_dir(&args(&["ext-host"])), None);
        assert_eq!(ext_host_dir(&args(&["ext-host", "/x", "y"])), None);
        assert!(runs_cli("ext-host"), "sonst: unbekannter Befehl in der CLI");
    }
}
