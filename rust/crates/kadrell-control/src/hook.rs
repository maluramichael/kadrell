//! `kadrell hook claude`: Claude Code ruft seine Hooks mit JSON auf stdin auf und vererbt ihnen die Umgebung der
//! Kachel (`KADRELL_SOCKET`, `KADRELL_SESSION_KEY`, `CLAUDE_PID`). Hier der Transportweg: stdin lesen, das Ereignis
//! auf `status …` abbilden und an den Kanal schicken. Die Abbildung selbst (`ClaudeHook.statusArgv`) liegt in
//! `kadrell-claude`; das Binary reicht sie als Funktion herein.
//!
//! Der Hook darf Claude weder bremsen noch Fehler zeigen: außerhalb von Kadrell (kein `KADRELL_SOCKET`) oder ohne
//! Antwort ist er stumm und endet mit 0, er startet nie die App und wartet nie lange.

use crate::client::{caller_from_env, cwd_from_env, send};
use crate::protocol::{ControlRequest, MAX_REQUEST};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

/// Der Hook läuft mit `timeout: 5` in den Einstellungen von Claude; kürzer bleiben.
const HOOK_TIMEOUT: Duration = Duration::from_secs(4);

/// Was der Hook aus der Umgebung braucht.
#[derive(Debug, Clone, Default)]
pub struct HookEnv {
    /// `KADRELL_SOCKET`: fehlt es, ist der Hook außerhalb von Kadrell und tut nichts.
    pub socket: Option<PathBuf>,
    /// `KADRELL_SESSION_KEY`.
    pub caller: Option<String>,
    pub cwd: String,
}

impl HookEnv {
    /// Aus der Umgebung des Prozesses. Anders als die CLI gilt hier nur `KADRELL_SOCKET`, nie der Standardpfad des
    /// Profils: ein Hook aus einer fremden Claude-Session darf Kadrell nicht ansprechen.
    pub fn from_env() -> HookEnv {
        let socket = std::env::var_os("KADRELL_SOCKET").filter(|p| !p.is_empty()).map(PathBuf::from);
        HookEnv { socket, caller: caller_from_env(), cwd: cwd_from_env() }
    }
}

/// `argv` ist die ganze Kommandozeile (`["hook", "claude"]`). Gibt den Exit-Code zurück: 1 nur bei falschen
/// Argumenten, sonst immer 0.
pub fn run_hook(argv: &[String], env: &HookEnv, stdin: &mut dyn Read, map: &dyn Fn(&[u8]) -> Option<Vec<String>>, err: &mut dyn Write) -> i32 {
    if argv != ["hook", "claude"] {
        let _ = writeln!(err, "kadrell: hook claude (liest das Hook-JSON von stdin)");
        return 1;
    }
    // Zuerst der Socket: außerhalb von Kadrell wird nicht einmal stdin gelesen.
    let Some(socket) = &env.socket else { return 0 };
    let mut json = Vec::new();
    if stdin.take(MAX_REQUEST as u64).read_to_end(&mut json).is_err() {
        return 0;
    }
    let Some(status) = map(&json) else { return 0 };
    let _ = send(socket, &ControlRequest::new(status, env.cwd.clone(), env.caller.clone()), HOOK_TIMEOUT);
    0
}
