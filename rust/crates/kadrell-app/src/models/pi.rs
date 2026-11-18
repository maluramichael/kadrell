//! Startkommando einer Pi-Session (Swift: `AttachManager.piLaunch`, `PiCLI.binary/launch/path`): `pi` mit dem lokalen
//! Modell der Session, ohne Modell mit Pis eigenem Standard. Ohne `pi` startet die Shell mit Hinweis, statt still zu
//! scheitern. Reine Funktionen, die Wahl der Dateien kommt von außen.

use std::collections::BTreeMap;

use kadrell_claude::cli::{LaunchSpec, path_exts, tile_env};
use kadrell_claude::paths;
use kadrell_claude::pi::{self, LocalModelRef};
use kadrell_core::Session;

/// Alles, was der Start einer Pi-Kachel braucht.
pub struct PiStart<'a> {
    pub session: &'a Session,
    /// Umgebung ohne die Marker einer umgebenden Claude-Session (`LaunchEnv::base`).
    pub env: &'a BTreeMap<String, String>,
    pub home: &'a str,
    pub socket: &'a str,
    pub exe: &'a str,
    /// Adresse des lokalen Anbieters, falls schon bekannt (ohne `/v1`).
    pub base_url: Option<&'a str>,
    pub prompt: Option<&'a str>,
}

fn env_path(env: &BTreeMap<String, String>) -> Option<(String, String)> {
    env.iter().find(|(k, _)| k.eq_ignore_ascii_case("PATH")).map(|(k, v)| (k.clone(), v.clone()))
}

/// `pi` im PATH oder unter `~/.local/bin`. Unter Windows nur Dateien mit Endung (`.exe`, `.cmd`, `.bat`): npm legt neben
/// `pi.cmd` ein Shell-Skript `pi` ohne Endung ab, das `CreateProcess` nicht starten kann.
pub fn locate(env: &BTreeMap<String, String>, home: &str, is_file: &dyn Fn(&str) -> bool) -> Option<String> {
    let mut lookup = env.clone();
    if let Some((_, path)) = env_path(env) {
        lookup.insert("PATH".into(), path);
    }
    let found = pi::binary(&lookup, home, is_file)?;
    if !cfg!(windows) || std::path::Path::new(&found).extension().is_some() {
        return Some(found);
    }
    let dirs: Vec<String> = lookup.get("PATH").map(|p| p.split(';').map(String::from).collect()).unwrap_or_default();
    let exts: Vec<String> = path_exts(&lookup).into_iter().chain([".exe".to_string(), ".cmd".to_string()]).collect();
    dirs.iter().chain(std::iter::once(&paths::join(home, ".local/bin"))).filter(|d| !d.is_empty()).find_map(|d| exts.iter().map(|e| format!("{}{e}", paths::join(d, "pi"))).find(|c| is_file(c)))
}

/// Läuft `binary` über `cmd.exe` (Windows, `.cmd`/`.bat`)? Dann deuten `& | < > ^ %` und `"` im Prompt die Kommandozeile um.
pub fn runs_in_cmd(binary: &str) -> bool {
    let lower = binary.to_lowercase();
    cfg!(windows) && (lower.ends_with(".cmd") || lower.ends_with(".bat"))
}

pub fn has_cmd_metachar(prompt: &str) -> bool {
    prompt.chars().any(|c| matches!(c, '&' | '|' | '<' | '>' | '^' | '%' | '"' | '\n' | '\r'))
}

/// PATH der Kachel mit dem Ordner von `pi` vorne; den Schlüssel `Path` oder `PATH` behält die Umgebung bei.
fn with_pi_path(env: &mut BTreeMap<String, String>, binary: &str) {
    let key = env_path(env).map_or_else(|| "PATH".to_string(), |(k, _)| k);
    let path = pi::path_for(binary, env.get(&key).map(String::as_str));
    env.insert(key, path);
}

/// Startkommando mit gefundenem `binary`. Ein Prompt, den `cmd.exe` umdeuten würde, geht nicht mit (die Session startet
/// dann ohne ihn, statt eine fremde Kommandozeile auszuführen).
pub fn launch(start: &PiStart, binary: &str) -> LaunchSpec {
    let model: Option<LocalModelRef> = start.session.model.as_deref().and_then(LocalModelRef::parse);
    let prompt = start.prompt.filter(|p| !(runs_in_cmd(binary) && has_cmd_metachar(p)));
    let launch = pi::launch(start.session.pi_session_id(), model.as_ref(), start.base_url, prompt);
    let mut env = tile_env_of(start);
    env.extend(launch.env);
    with_pi_path(&mut env, binary);
    LaunchSpec { program: binary.to_string(), args: launch.args, cwd: start.session.cwd.clone(), env }
}

fn tile_env_of(start: &PiStart) -> BTreeMap<String, String> {
    let cli = kadrell_claude::cli::ClaudeCli::new(String::new(), start.env.clone(), kadrell_claude::cli::StripMode::Broad);
    tile_env(&cli, &start.session.id, start.socket, start.exe)
}

/// Hinweis statt Pi: eine Shell, die erklärt, was fehlt.
pub fn missing(start: &PiStart, shell: (String, Vec<String>), message: &str) -> LaunchSpec {
    let (program, args) = if cfg!(windows) {
        let quoted = message.replace('\'', "''");
        (shell.0, vec!["-NoLogo".into(), "-NoExit".into(), "-Command".into(), format!("Write-Host '{quoted}'")])
    } else {
        ("/bin/sh".into(), vec!["-c".into(), r#"printf '%s\n' "$1"; exec "${SHELL:-/bin/zsh}" -l"#.into(), "sh".into(), message.into()])
    };
    LaunchSpec { program, args, cwd: start.session.cwd.clone(), env: tile_env_of(start) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(model: Option<&str>) -> Session {
        let mut s = Session::new(&format!("pi-{}", "abc"), "/p", 1.0, "pi-abc", "");
        s.model = model.map(String::from);
        s
    }

    fn start<'a>(s: &'a Session, env: &'a BTreeMap<String, String>, prompt: Option<&'a str>) -> PiStart<'a> {
        PiStart { session: s, env, home: "/h", socket: "/s/k.sock", exe: "/x/kadrell", base_url: Some("http://127.0.0.1:11434"), prompt }
    }

    #[test]
    fn local_model_goes_to_the_provider_with_address_and_marker_env() {
        let env = BTreeMap::from([("PATH".to_string(), "/usr/bin".to_string()), ("CLAUDECODE".to_string(), "1".to_string())]);
        let s = session(Some("ollama|qwen3:4b"));
        let spec = launch(&start(&s, &env, Some("Idee")), "/opt/pi/bin/pi");
        assert_eq!(spec.program, "/opt/pi/bin/pi");
        assert_eq!(spec.args, ["--provider", "kadrell", "--model", "qwen3:4b", "--session-id", "abc", "Idee"]);
        assert_eq!(spec.env["KADRELL_PI_BASE_URL"], "http://127.0.0.1:11434/v1");
        assert_eq!(spec.env["KADRELL_SESSION_KEY"], "pi-abc");
        assert!(!spec.env.contains_key("CLAUDECODE"));
        assert!(spec.env["PATH"].starts_with("/opt/pi/bin"), "{}", spec.env["PATH"]);
        assert_eq!(spec.cwd, "/p");
    }

    #[test]
    fn without_a_model_pi_uses_its_own_default() {
        let env = BTreeMap::new();
        let s = session(None);
        let spec = launch(&start(&s, &env, None), "/opt/pi/bin/pi");
        assert_eq!(spec.args, ["--session-id", "abc"]);
        assert!(!spec.env.contains_key("KADRELL_PI_BASE_URL"));
        assert_eq!(spec.env["KADRELL_PI"], "1");
    }

    #[test]
    fn the_existing_path_key_keeps_its_spelling() {
        let env = BTreeMap::from([("Path".to_string(), "C:\\x".to_string())]);
        let s = session(None);
        let spec = launch(&start(&s, &env, None), "C:/pi/pi.exe");
        assert!(spec.env.contains_key("Path") && !spec.env.contains_key("PATH"), "kein doppelter PATH");
    }

    #[test]
    fn cmd_wrappers_never_get_a_prompt_cmd_could_misread() {
        assert!(has_cmd_metachar("fix A & B"));
        assert!(has_cmd_metachar("echo %PATH%"));
        assert!(!has_cmd_metachar("normale Idee"));
        let env = BTreeMap::new();
        let s = session(None);
        let spec = launch(&start(&s, &env, Some("A & B")), "C:/n/pi.cmd");
        assert_eq!(spec.args.last().map(String::as_str), Some(if cfg!(windows) { "abc" } else { "A & B" }));
        assert_eq!(runs_in_cmd("C:/n/pi.cmd"), cfg!(windows));
        assert!(!runs_in_cmd("C:/n/pi.exe"));
    }

    #[test]
    fn locate_prefers_files_with_an_extension_on_windows() {
        let env = BTreeMap::from([("PATH".to_string(), if cfg!(windows) { "C:/n" } else { "/n" }.to_string())]);
        let dir = if cfg!(windows) { "C:/n" } else { "/n" };
        let bare = format!("{dir}/pi");
        let cmd = format!("{dir}/pi.cmd");
        let all = [bare.clone(), cmd.clone()];
        let found = locate(&env, "/h", &|p| all.iter().any(|a| a == p));
        assert_eq!(found, Some(if cfg!(windows) { cmd.clone() } else { bare.clone() }));
        assert_eq!(locate(&env, "/h", &|_| false), None);
        let only_bare = locate(&env, "/h", &|p| p == bare);
        assert_eq!(only_bare, if cfg!(windows) { None } else { Some(bare) });
    }

    #[test]
    fn missing_pi_opens_a_shell_with_the_message() {
        let env = BTreeMap::new();
        let s = session(None);
        let spec = missing(&start(&s, &env, None), ("pwsh".into(), vec![]), "Pi fehlt");
        if cfg!(windows) {
            assert_eq!(spec.program, "pwsh");
            assert!(spec.args.last().unwrap().contains("Write-Host 'Pi fehlt'"));
        } else {
            assert_eq!(spec.program, "/bin/sh");
            assert_eq!(spec.args.last().map(String::as_str), Some("Pi fehlt"));
        }
    }
}
