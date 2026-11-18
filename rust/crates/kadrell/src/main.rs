//! Kadrell: Einstiegspunkt. Ein Binary für App und Kommandozeile:
//!
//! - `kadrell` (mit oder ohne `--profile <name>`): startet die App.
//! - `kadrell hook claude`: Hook von Claude Code, meldet den Status an die laufende App.
//! - `kadrell ext-host <ordner>`: Host-Prozess der Lua-Extensions (läuft in `kadrell-ext`).
//! - `kadrell <befehl>`: Steuerkanal der laufenden App (`ls`, `new`, `send`, `capture`, ...), startet sie bei Bedarf.

use std::collections::HashMap;

use kadrell_core::Profile;

/// `--profile <name>` und `--extensions-dir <ordner>` (das liest die Extension-Schicht selbst) aus den Argumenten nehmen: (restliche Argumente, Name aus Argument oder `KADRELL_PROFILE`).
fn split_profile(args: Vec<String>) -> (Vec<String>, Option<String>) {
    let name = Profile::name_from_args(&args, std::env::var(kadrell_core::profile::PROFILE_ENV).ok().as_deref());
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        if arg == "--profile" || arg == "--extensions-dir" {
            it.next();
        } else {
            rest.push(arg);
        }
    }
    (rest, name)
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

/// `kadrell ext-host <ordner>`: der Helper einer Lua-Extension; Protokoll auf stdin und stdout.
fn ext_host(args: &[String]) -> std::process::ExitCode {
    let Some(dir) = args.get(1) else {
        eprintln!("kadrell: ext-host braucht den Ordner der Extension");
        return std::process::ExitCode::from(2);
    };
    kadrell_ext::run_host(std::path::Path::new(dir))
}

fn main() -> std::process::ExitCode {
    let (args, profile) = split_profile(std::env::args().skip(1).collect());
    let code = match args.first().map(String::as_str) {
        None => {
            // Vor allen Threads: die Marker einer umgebenden Claude-Session erben die Kacheln sonst.
            kadrell_term::strip_inherited_env();
            detach_console();
            kadrell_app::run(profile)
        }
        Some("ext-host") => return ext_host(&args),
        Some(_) => kadrell_control::client::main_cli(&args, profile, &hook_map),
    };
    std::process::exit(code)
}
