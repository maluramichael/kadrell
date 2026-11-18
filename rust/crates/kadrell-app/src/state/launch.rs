//! Startkommando einer Kachel: Claude mit `--session-id` bzw. `--resume`, die Hooks per `--settings`, Umgebung und
//! Marker für den Steuerkanal (`kadrell-claude::cli`), oder ein Terminal ohne Claude.

use std::collections::BTreeMap;

use kadrell_claude::cli::{ClaudeCli, ClaudeStart, LaunchFlags, LaunchSpec, StripMode, claude_launch, find_in_path, locate_binary, path_exts, process_env, tile_env};
use kadrell_claude::transcript;
use kadrell_core::{Session, Settings};

/// Was jede Kachel an Umgebung braucht: das `claude`-Binary (falls gefunden), bereinigte Umgebung, eigener Pfad,
/// Kanal des Profils.
#[derive(Clone)]
pub struct LaunchEnv {
    /// `None`: `claude` nicht gefunden, Claude-Kacheln starten dann nicht (Terminals schon).
    pub claude: Option<ClaudeCli>,
    /// Umgebung für Kacheln ohne Claude (ohne die Marker einer umgebenden Claude-Session).
    pub base: ClaudeCli,
    pub exe: String,
    pub socket: String,
    pub home: String,
}

impl LaunchEnv {
    /// Aus der Umgebung des Prozesses. `override_binary` ist die Einstellung `claudeBinary` (leer: suchen).
    pub fn from_process(override_binary: &str, exe: String, socket: String, home: String) -> LaunchEnv {
        let env = process_env();
        let is_file = |p: &str| std::path::Path::new(p).is_file();
        // Windows nennt die Variable `Path`, die Suche kennt nur `PATH`: nur für die Suche ergänzen.
        let mut search = env.clone();
        if let Some(path) = env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")).map(|(_, v)| v.clone()) {
            search.insert("PATH".into(), path);
        }
        let found = if override_binary.is_empty() { locate_binary(&search, &home, &is_file, &|| None) } else { Some(override_binary.to_string()) };
        let claude = found.map(|b| ClaudeCli::new(b, env.clone(), StripMode::Broad));
        let base = ClaudeCli::new(String::new(), env, StripMode::Broad);
        LaunchEnv { claude, base, exe, socket, home }
    }

    /// Umgebung der Kacheln als Liste, auch für die eigenen Hooks des Nutzers.
    pub fn tile_env_list(&self) -> Vec<(String, String)> {
        self.base.environment.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Datenordner von Claude Code.
    pub fn config_dir(&self) -> String {
        self.claude.as_ref().unwrap_or(&self.base).config_dir(&self.home)
    }
}

/// Flags der Session: Modell und Effort der Session gehen den Einstellungen vor.
pub fn flags(session: &Session, settings: &Settings) -> LaunchFlags {
    LaunchFlags {
        allow_bypass: settings.claude_allow_bypass(),
        mode: settings.claude_mode(),
        model: session.claude_model.clone().unwrap_or_else(|| settings.claude_model()),
        effort: session.claude_effort.clone().unwrap_or_else(|| settings.claude_effort()),
    }
}

/// Programm und Argumente einer Shell ohne Claude: PowerShell unter Windows, die Login-Shell sonst.
pub fn shell_command() -> (String, Vec<String>) {
    if cfg!(windows) {
        let env = process_env();
        let exts = path_exts(&env);
        let is_file = |p: &str| std::path::Path::new(p).is_file();
        let path = env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")).map(|(_, v)| v.clone()).unwrap_or_default();
        let found = ["pwsh", "powershell"].iter().find_map(|n| find_in_path(n, &path, &exts, &is_file));
        (found.unwrap_or_else(|| "powershell.exe".into()), Vec::new())
    } else {
        unix_shell(std::env::var("SHELL").ok())
    }
}

/// Login-Shell des Nutzers (`$SHELL`, sonst `/bin/zsh`) als Login-Shell gestartet. Swift setzt argv[0] auf `-<shell>`
/// wie Terminal.app; die PTY-Schicht kann argv[0] nicht ändern, `-l` bewirkt dasselbe.
fn unix_shell(shell: Option<String>) -> (String, Vec<String>) {
    (shell.filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into()), vec!["-l".into()])
}

/// Startkommando für `session`. `prompt` ist die erste Nachricht einer neuen Konversation.
pub fn spec_for(session: &Session, settings: &Settings, env: &LaunchEnv, prompt: Option<&str>) -> Result<LaunchSpec, String> {
    if session.is_shell() {
        let (program, args) = shell_command();
        let tile = tile_env(&env.base, &session.id, &env.socket, &env.exe);
        return Ok(LaunchSpec { program, args, cwd: session.cwd.clone(), env: tile });
    }
    if session.is_remote() || session.is_pi() {
        return Err("Remote- und Pi-Sessions sind in dieser Version noch nicht verdrahtet".into());
    }
    let Some(cli) = &env.claude else {
        return Err("claude nicht gefunden: installieren (curl -fsSL https://claude.ai/install.sh | bash) oder unter Einstellungen den Pfad angeben".into());
    };
    let has_transcript = transcript::path(&session.session_id, &cli.config_dir(&env.home)).is_some();
    let start = ClaudeStart {
        session_id: &session.session_id,
        cwd: &session.cwd,
        has_transcript,
        flags: flags(session, settings),
        prompt,
        fullscreen: settings.claude_fullscreen(),
        session_key: &session.id,
        socket: &env.socket,
        exe: &env.exe,
    };
    Ok(claude_launch(cli, &start))
}

/// Umgebung als Liste für das Terminal.
pub fn env_pairs(env: &BTreeMap<String, String>) -> Vec<(String, String)> {
    env.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with(claude: bool) -> LaunchEnv {
        let base = ClaudeCli::new(String::new(), BTreeMap::from([("PATH".to_string(), "x".to_string()), ("CLAUDECODE".to_string(), "1".to_string())]), StripMode::Broad);
        let cli = claude.then(|| ClaudeCli::new("claude", base.environment.clone(), StripMode::Broad));
        LaunchEnv { claude: cli, base, exe: "C:\\k\\kadrell.exe".into(), socket: "C:\\p\\kadrell.endpoint".into(), home: "/nonexistent-home".into() }
    }

    #[test]
    fn new_claude_session_uses_session_id_and_hooks_and_marker_env() {
        let s = Session::new("key-1", "/p", 1.0, "key-1", "");
        let spec = spec_for(&s, &Settings::default(), &env_with(true), None).unwrap();
        assert_eq!(spec.program, "claude");
        assert_eq!(&spec.args[..2], ["--session-id", "key-1"]);
        assert!(spec.args.iter().any(|a| a == "--settings"));
        assert_eq!(spec.env["KADRELL_SESSION_KEY"], "key-1");
        assert!(spec.env.contains_key("KADRELL_SOCKET") && spec.env.contains_key("KADRELL"));
        assert!(!spec.env.contains_key("CLAUDECODE"), "Marker der umgebenden Session fliegen raus");
    }

    #[test]
    fn a_shell_tile_uses_the_users_shell_and_falls_back_to_zsh() {
        assert_eq!(unix_shell(Some("/usr/bin/fish".into())), ("/usr/bin/fish".to_string(), vec!["-l".to_string()]));
        assert_eq!(unix_shell(None).0, "/bin/zsh");
        assert_eq!(unix_shell(Some(String::new())).0, "/bin/zsh");
    }

    #[test]
    fn prompt_goes_last_after_double_dash() {
        let s = Session::new("key-1", "/p", 1.0, "key-1", "");
        let spec = spec_for(&s, &Settings::default(), &env_with(true), Some("hallo")).unwrap();
        assert_eq!(spec.args[spec.args.len() - 2..], ["--", "hallo"]);
    }

    #[test]
    fn session_model_and_effort_beat_settings() {
        let mut s = Session::new("k", "/p", 1.0, "k", "");
        s.claude_model = Some("haiku".into());
        let mut settings = Settings::default();
        settings.set_claude_model("opus");
        settings.set_claude_effort("low");
        let f = flags(&s, &settings);
        assert_eq!((f.model.as_str(), f.effort.as_str()), ("haiku", "low"));
    }

    #[test]
    fn missing_claude_and_unwired_kinds_are_errors_but_shells_start() {
        let s = Session::new("k", "/p", 1.0, "k", "");
        assert!(spec_for(&s, &Settings::default(), &env_with(false), None).unwrap_err().contains("claude nicht gefunden"));
        let mut remote = Session::new("ssh-k", "/p", 1.0, "k", "");
        remote.host = Some("box".into());
        assert!(spec_for(&remote, &Settings::default(), &env_with(true), None).is_err());
        let shell = Session::new(&Session::new_shell_id(), "/p", 1.0, "s", "");
        let spec = spec_for(&shell, &Settings::default(), &env_with(false), None).unwrap();
        assert_eq!(spec.env["KADRELL_SESSION_KEY"], shell.id);
        assert!(spec.args.is_empty() || !cfg!(windows));
    }
}
