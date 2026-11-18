//! Das `claude`-Binary: finden, Umgebung bereinigen, Versionsprüfung, Startargumente einer Kachel
//! (Swift: `Services/ClaudeCLI.swift`, der Startteil aus `AttachManager.attach`, dazu `windows/src/app.rs::launch`).

use crate::{agent, hook, paths};
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Ausgabe eines Prozesses. Swift mischt stderr in stdout; hier bleiben beide getrennt, damit eine Warnung auf stderr
/// nie das JSON von `claude agents --json` zerlegt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl RunOutput {
    /// stdout und stderr hintereinander, für Fehlermeldungen.
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Ein Prozessaufruf. Hinter dem Trait, damit Tests Fixtures liefern (`claude agents`, `git`, `zsh -lc env`).
pub trait CommandRunner {
    /// `env`: nil = Umgebung erben, sonst genau diese. Fehler = Prozess ließ sich nicht starten.
    fn run(&self, program: &str, args: &[String], env: Option<&BTreeMap<String, String>>, cwd: Option<&str>, timeout: Duration) -> Result<RunOutput, String>;
}

/// Startet echte Prozesse; nach `timeout` wird der Prozess beendet, die bis dahin gelesene Ausgabe kommt zurück.
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[String], env: Option<&BTreeMap<String, String>>, cwd: Option<&str>, timeout: Duration) -> Result<RunOutput, String> {
        let mut cmd = Command::new(program);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(env) = env {
            cmd.env_clear().envs(env);
        }
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let out = drain(child.stdout.take());
        let err = drain(child.stderr.take());
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(s) => break s.code().unwrap_or(-1),
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break -1;
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        };
        let text = |h: std::thread::JoinHandle<Vec<u8>>| String::from_utf8_lossy(&h.join().unwrap_or_default()).into_owned();
        Ok(RunOutput { status, stdout: text(out), stderr: text(err) })
    }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    })
}

/// `claude …` endete mit Fehler (`CLIError`).
#[derive(Clone, Debug, PartialEq)]
pub struct CliError {
    pub command: String,
    pub status: i32,
    pub output: String,
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} → exit {}: {}", self.command, self.status, self.output.trim())
    }
}

impl CliError {
    /// Erste Zeile der Prozessausgabe, für kurze Fehlermeldungen in der UI.
    pub fn first_line(text: &str) -> String {
        text.trim().lines().next().unwrap_or("").to_string()
    }
}

/// Welche Umgebungsvariablen aus einer umgebenden Claude-Session nicht an die Kachel gehen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripMode {
    /// Swift-Verhalten: nur die sitzungsbezogenen Marker (`SESSION_MARKERS`), Einstellungen wie
    /// `CLAUDE_CODE_MAX_OUTPUT_TOKENS` bleiben. Dazu `AI_AGENT` und ein von Claude Code gesetztes `NO_COLOR`.
    Markers,
    /// Regel des GPUI-Spikes (`inherited_env_to_strip`): alles `CLAUDE_CODE_*` fliegt raus, zusätzlich zu den
    /// Markern von `Markers` (nie weniger als Swift).
    Broad,
}

/// Fest benannte Marker, die Swift entfernt. Claude Code speichert sonst kein Transcript („inherited
/// CLAUDE_CODE_CHILD_SESSION marker“) und `--resume` findet nichts.
pub const SESSION_MARKERS: [&str; 10] = [
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_JOB_DIR",
];

/// Namen aus `names`, die aus der geerbten Umgebung fliegen. Groß-/Kleinschreibung zählt nicht (Windows).
/// `NO_COLOR` nur, wenn `CLAUDECODE` gesetzt ist: dann hat Claude Code es für seine Werkzeug-Prozesse gesetzt, ein
/// vom Nutzer selbst gesetztes bleibt.
pub fn env_names_to_strip(names: &[String], mode: StripMode) -> Vec<String> {
    let upper = |n: &str| n.to_ascii_uppercase();
    let in_claude_session = names.iter().any(|n| upper(n) == "CLAUDECODE");
    names
        .iter()
        .filter(|n| {
            let u = upper(n);
            let marker = SESSION_MARKERS.contains(&u.as_str()) || (mode == StripMode::Broad && u.starts_with("CLAUDE_CODE_"));
            marker || u == "AI_AGENT" || (in_claude_session && u == "NO_COLOR")
        })
        .cloned()
        .collect()
}

/// Zeilen aus `env -0` (nach einem führenden NUL, das Ausgaben des Shell-Profils abtrennt) als Variablen. NUL-getrennt,
/// damit mehrzeilige Werte keine Variablen erfinden. Zeilen ohne `=` oder mit ungültigem Namen fallen weg.
pub fn parse_env0(out: &str) -> Vec<(String, String)> {
    out.split('\0')
        .skip(1)
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let mut chars = key.chars();
            let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
            valid.then(|| (key.to_string(), value.to_string()))
        })
        .collect()
}

/// Login-Shell-Umgebung (`/bin/zsh -lc env -0`) über die geerbte Umgebung gelegt. Nur macOS: dort startet die App aus dem
/// Dock ohne die PATH-Einträge des Nutzers. Unter Windows gilt die Prozessumgebung unverändert.
pub fn login_shell_env(runner: &dyn CommandRunner, base: BTreeMap<String, String>) -> BTreeMap<String, String> {
    let args = ["-lc".to_string(), "printf '\\0'; exec env -0".to_string()];
    let mut env = base;
    if let Ok(out) = runner.run("/bin/zsh", &args, None, None, Duration::from_secs(10)) {
        env.extend(parse_env0(&out.stdout));
    }
    env
}

/// Die Umgebung des eigenen Prozesses als Map.
pub fn process_env() -> BTreeMap<String, String> {
    std::env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))).collect()
}

/// `name` in den Ordnern von `path_var` suchen. `exts`: zusätzliche Endungen (Windows: PATHEXT), `is_file` prüft.
pub fn find_in_path(name: &str, path_var: &str, exts: &[String], is_file: &dyn Fn(&str) -> bool) -> Option<String> {
    let sep = if cfg!(windows) { ';' } else { ':' };
    path_var.split(sep).filter(|d| !d.is_empty()).find_map(|dir| {
        let base = paths::join(dir, name);
        std::iter::once(base.clone()).chain(exts.iter().map(|e| format!("{base}{e}"))).find(|c| is_file(c))
    })
}

/// Endungen aus PATHEXT, nur unter Windows (dort ist `claude` meist `claude.exe` oder `claude.cmd`).
pub fn path_exts(env: &BTreeMap<String, String>) -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let raw = env.get("PATHEXT").cloned().unwrap_or_else(|| ".EXE;.CMD;.BAT".into());
    raw.split(';').filter(|e| !e.is_empty()).map(str::to_lowercase).collect()
}

/// Wo `claude` liegt: der offizielle Installationsort `~/.local/bin/claude`, dann PATH. `is_file` prüft Dateien,
/// `shell_lookup` fragt notfalls die Login-Shell (`command -v claude`, macOS). Unter Windows gilt PATH (mit PATHEXT);
/// Hooks und Befehle laufen dort über Git Bash, `claude` selbst muss aber nur im PATH des Prozesses stehen.
pub fn locate_binary(env: &BTreeMap<String, String>, home: &str, is_file: &dyn Fn(&str) -> bool, shell_lookup: &dyn Fn() -> Option<String>) -> Option<String> {
    let exts = path_exts(env);
    let local = paths::join(&paths::join(home, ".local"), "bin/claude");
    std::iter::once(local.clone())
        .chain(exts.iter().map(|e| format!("{local}{e}")))
        .find(|c| is_file(c))
        .or_else(|| find_in_path("claude", env.get("PATH").map(String::as_str).unwrap_or(""), &exts, is_file))
        .or_else(|| shell_lookup().filter(|p| paths::is_abs(p)))
}

/// Dünner Wrapper um das `claude`-Binary. Erbt nie Shell-Aliase.
#[derive(Clone, Debug)]
pub struct ClaudeCli {
    pub binary: String,
    /// Bereinigte Umgebung für alle `claude`-Prozesse (und Kacheln).
    pub environment: BTreeMap<String, String>,
}

impl ClaudeCli {
    /// Aus einer Claude-Session heraus gestartet, erben die Prozesse deren Marker; die Umgebung wird hier bereinigt
    /// (`mode`) und bekommt `TERM`, `COLORTERM` und notfalls `LANG`.
    pub fn new(binary: impl Into<String>, environment: BTreeMap<String, String>, mode: StripMode) -> ClaudeCli {
        let names: Vec<String> = environment.keys().cloned().collect();
        let strip = env_names_to_strip(&names, mode);
        let mut env: BTreeMap<String, String> = environment.into_iter().filter(|(k, _)| !strip.contains(k)).collect();
        env.insert("TERM".into(), "xterm-256color".into());
        env.insert("COLORTERM".into(), "truecolor".into());
        env.entry("LANG".into()).or_insert_with(|| "en_US.UTF-8".into());
        ClaudeCli { binary: binary.into(), environment: env }
    }

    /// Datenordner von Claude Code (`~/.claude`, per `CLAUDE_CONFIG_DIR` verlegbar).
    pub fn config_dir(&self, home: &str) -> String {
        self.environment.get("CLAUDE_CONFIG_DIR").cloned().unwrap_or_else(|| format!("{home}/.claude"))
    }

    /// Ruft `claude <args>` auf; Exit ≠ 0 ist ein Fehler mit der Ausgabe. Liefert stdout.
    pub fn run(&self, runner: &dyn CommandRunner, args: &[&str], cwd: Option<&str>, timeout: Duration) -> Result<String, CliError> {
        let owned: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let command = format!("claude {}", args.join(" "));
        let r = runner
            .run(&self.binary, &owned, Some(&self.environment), cwd, timeout)
            .map_err(|e| CliError { command: command.clone(), status: -1, output: e })?;
        if r.status == 0 { Ok(r.stdout) } else { Err(CliError { command, status: r.status, output: r.combined() }) }
    }

    /// `claude agents --json --all`.
    pub fn agents(&self, runner: &dyn CommandRunner) -> Result<Vec<agent::AgentInfo>, CliError> {
        let out = self.run(runner, &["agents", "--json", "--all"], None, Duration::from_secs(60))?;
        agent::parse_agents_output(&out).map_err(|e| CliError { command: "claude agents --json --all".into(), status: 0, output: e })
    }

    /// Hält eine Hintergrund-Session aus `claude --bg` an; die Konversation bleibt für `--resume` erhalten.
    pub fn stop(&self, runner: &dyn CommandRunner, id: &str) -> Result<(), CliError> {
        self.run(runner, &["stop", id], None, Duration::from_secs(60)).map(|_| ())
    }

    /// Ergebnis von `check_version`. Zu alt blockiert nicht: Kadrell lief bisher meist auch mit einer knapp älteren CLI,
    /// `agents --json` ist die eigentliche Probe. Nur ein fehlgeschlagener `--version`-Aufruf bleibt ein echter Fehler.
    pub fn check_version(&self, runner: &dyn CommandRunner) -> VersionCheck {
        match self.run(runner, &["--version"], None, Duration::from_secs(60)) {
            Err(e) => VersionCheck::RunFailed { binary: self.binary.clone(), first_line: CliError::first_line(&e.output) },
            Ok(out) => match parse_version(&out) {
                None => VersionCheck::Unparseable { binary: self.binary.clone(), output: out.trim().to_string() },
                Some(v) if v < MIN_VERSION => VersionCheck::TooOld { found: v, tested: MIN_VERSION },
                Some(_) => VersionCheck::Ok,
            },
        }
    }
}

pub type Version = (u32, u32, u32);

/// Zuletzt gegen `docs/kadrell-verifikation.md` geprüfte Version.
pub const MIN_VERSION: Version = (2, 1, 273);
/// Offizieller Installationsweg (docs.claude.com/en/docs/claude-code/setup, Stand 2026-09).
pub const INSTALL_COMMAND: &str = "curl -fsSL https://claude.ai/install.sh | bash";
pub const INSTALL_DOCS_URL: &str = "https://docs.claude.com/en/docs/claude-code/setup";
pub const UPDATE_COMMAND: &str = "claude update";

/// Meldungen trägt die Oberfläche (zweisprachig), hier nur die Fakten.
#[derive(Clone, Debug, PartialEq)]
pub enum VersionCheck {
    Ok,
    TooOld { found: Version, tested: Version },
    RunFailed { binary: String, first_line: String },
    Unparseable { binary: String, output: String },
}

/// Erste Folge `<zahl>.<zahl>.<zahl>` im Text.
pub fn parse_version(output: &str) -> Option<Version> {
    let bytes = output.as_bytes();
    let digits = |from: usize| bytes[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    (0..bytes.len()).filter(|i| bytes[*i].is_ascii_digit()).find_map(|start| {
        let mut parts = [0u32; 3];
        let mut at = start;
        for (n, part) in parts.iter_mut().enumerate() {
            let len = digits(at);
            if len == 0 {
                return None;
            }
            *part = output[at..at + len].parse().ok()?;
            at += len;
            if n < 2 {
                if bytes.get(at) != Some(&b'.') {
                    return None;
                }
                at += 1;
            }
        }
        Some((parts[0], parts[1], parts[2]))
    })
}

/// Neue Session unter vorgegebener Id, bekannte Konversation mit `--resume`. Ohne erste Nachricht gibt es kein
/// Transcript und `--resume` scheitert („No conversation found“), dann gilt `--session-id` mit derselben UUID.
pub fn session_args(session_id: &str, has_transcript: bool) -> Vec<String> {
    vec![if has_transcript { "--resume" } else { "--session-id" }.to_string(), session_id.to_string()]
}

/// Erste Nachricht hinter `--`: sonst liest claude einen Prompt wie `--dangerously-skip-permissions` als Option
/// (geprüft mit claude 2.1.274).
pub fn prompt_args(prompt: &str) -> Vec<String> {
    vec!["--".into(), prompt.into()]
}

/// Start-Flags aus den Einstellungen. Leerer String = Claude-Default, Flag entfällt.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaunchFlags {
    pub allow_bypass: bool,
    pub mode: String,
    pub model: String,
    pub effort: String,
}

pub fn launch_args(flags: &LaunchFlags) -> Vec<String> {
    let mut args = Vec::new();
    if flags.allow_bypass {
        args.push("--allow-dangerously-skip-permissions".to_string());
    }
    for (flag, value) in [("--permission-mode", &flags.mode), ("--model", &flags.model), ("--effort", &flags.effort)] {
        if !value.is_empty() {
            args.extend([flag.to_string(), value.clone()]);
        }
    }
    args
}

/// Alles für einen Prozessstart (Claude, Pi, Shell): Programm, Argumente, Ordner, vollständige Umgebung.
#[derive(Clone, Debug, PartialEq)]
pub struct LaunchSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    /// Die ganze Umgebung (Start ohne geerbte Variablen).
    pub env: BTreeMap<String, String>,
}

/// Was eine Claude-Kachel zum Start braucht.
#[derive(Clone, Debug)]
pub struct ClaudeStart<'a> {
    pub session_id: &'a str,
    pub cwd: &'a str,
    pub has_transcript: bool,
    pub flags: LaunchFlags,
    /// Erste Nachricht, nur für eine neue Konversation.
    pub prompt: Option<&'a str>,
    /// Fullscreen-Renderer (`CLAUDE_CODE_NO_FLICKER`).
    pub fullscreen: bool,
    pub session_key: &'a str,
    /// Socket (Mac) bzw. Endpunktdatei (Windows) des Steuerkanals.
    pub socket: &'a str,
    /// Pfad des eigenen Binaries für die Hook-Zeile.
    pub exe: &'a str,
}

/// Umgebung jeder Kachel: `KADRELL_SESSION_KEY`, `KADRELL_SOCKET`, `KADRELL` (siehe `docs/cli-contract.md`).
/// Unter Windows mit `/`, weil Claude Code die Hook-Zeile in Git Bash ausführt.
pub fn tile_env(cli: &ClaudeCli, session_key: &str, socket: &str, exe: &str) -> BTreeMap<String, String> {
    let shell_path = |p: &str| if cfg!(windows) { paths::fwd(p) } else { p.to_string() };
    let mut env = cli.environment.clone();
    env.insert("KADRELL_SESSION_KEY".into(), session_key.into());
    env.insert("KADRELL_SOCKET".into(), shell_path(socket));
    env.insert("KADRELL".into(), shell_path(exe));
    env
}

/// Argumente und Umgebung einer Claude-Kachel: `--resume`/`--session-id`, Flags aus den Einstellungen, die fünf
/// Hooks per `--settings`, ganz hinten `-- <prompt>`.
pub fn claude_launch(cli: &ClaudeCli, start: &ClaudeStart) -> LaunchSpec {
    let mut args = session_args(start.session_id, start.has_transcript);
    args.extend(launch_args(&start.flags));
    args.extend(hook::launch_args());
    if let Some(prompt) = start.prompt.filter(|_| !start.has_transcript) {
        args.extend(prompt_args(prompt));
    }
    let mut env = tile_env(cli, start.session_key, start.socket, start.exe);
    if start.fullscreen {
        // Claude zeichnet im Alternate-Screen statt den Verlauf in den Scrollback zu streamen.
        env.insert("CLAUDE_CODE_NO_FLICKER".into(), "1".into());
    }
    LaunchSpec { program: cli.binary.clone(), args, cwd: start.cwd.to_string(), env }
}

/// Neue Session-Id (UUID v4, klein geschrieben).
pub fn new_session_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn parse_version_from_typical_output() {
        assert_eq!(parse_version("2.1.274 (Claude Code)"), Some((2, 1, 274)));
        assert_eq!(parse_version("claude 12.3.4.5"), Some((12, 3, 4)));
        assert_eq!(parse_version("v1.2 3.4.5"), Some((3, 4, 5)));
    }

    #[test]
    fn parse_version_fails_on_unknown_format() {
        assert_eq!(parse_version("command not found: claude"), None);
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("1.2"), None);
    }

    #[test]
    fn min_version_comparison() {
        assert!(parse_version("2.1.272").unwrap() < MIN_VERSION);
        assert!(parse_version("2.1.274").unwrap() >= MIN_VERSION);
        assert!(parse_version("2.2.0").unwrap() >= MIN_VERSION);
    }

    struct Fake {
        out: RunOutput,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl Fake {
        fn new(status: i32, stdout: &str, stderr: &str) -> Fake {
            Fake { out: RunOutput { status, stdout: stdout.into(), stderr: stderr.into() }, calls: RefCell::default() }
        }
    }

    impl CommandRunner for Fake {
        fn run(&self, _: &str, args: &[String], _: Option<&BTreeMap<String, String>>, _: Option<&str>, _: Duration) -> Result<RunOutput, String> {
            self.calls.borrow_mut().push(args.to_vec());
            Ok(self.out.clone())
        }
    }

    fn cli() -> ClaudeCli {
        ClaudeCli::new("claude", BTreeMap::new(), StripMode::Markers)
    }

    #[test]
    fn version_check_outcomes() {
        assert_eq!(cli().check_version(&Fake::new(0, "2.1.274 (Claude Code)\n", "")), VersionCheck::Ok);
        assert_eq!(cli().check_version(&Fake::new(0, "2.1.1", "")), VersionCheck::TooOld { found: (2, 1, 1), tested: MIN_VERSION });
        assert!(matches!(cli().check_version(&Fake::new(1, "", "boom\nmehr")), VersionCheck::RunFailed { first_line, .. } if first_line == "boom"));
        assert!(matches!(cli().check_version(&Fake::new(0, "hallo", "")), VersionCheck::Unparseable { .. }));
    }

    #[test]
    fn agents_survive_warnings_on_stderr_and_before_the_array() {
        let fake = Fake::new(0, "hinweis\n[{\"id\":\"ab\",\"pid\":3,\"sessionId\":\"s\",\"name\":\"n\",\"cwd\":\"/p\",\"kind\":\"background\",\"startedAt\":1}]", "warning: [x");
        let agents = cli().agents(&fake).unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(fake.calls.borrow()[0], ["agents", "--json", "--all"]);
        assert!(cli().agents(&Fake::new(2, "", "kaputt")).is_err());
        assert_eq!(cli().stop(&Fake::new(0, "stopped ab", ""), "ab"), Ok(()));
    }

    #[test]
    fn session_and_launch_args() {
        assert_eq!(session_args("u", true), ["--resume", "u"]);
        assert_eq!(session_args("u", false), ["--session-id", "u"]);
        assert!(launch_args(&LaunchFlags::default()).is_empty());
        let flags = LaunchFlags { allow_bypass: true, mode: "bypassPermissions".into(), model: "opus".into(), effort: "high".into() };
        assert_eq!(
            launch_args(&flags),
            ["--allow-dangerously-skip-permissions", "--permission-mode", "bypassPermissions", "--model", "opus", "--effort", "high"]
        );
        assert_eq!(prompt_args("--version"), ["--", "--version"]);
    }

    #[test]
    fn marker_stripping_keeps_settings_and_broad_mode_drops_them() {
        let env: BTreeMap<String, String> = [
            ("PATH", "/bin"),
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_CHILD_SESSION", "1"),
            ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "64000"),
            ("CLAUDE_PID", "5"),
            ("NO_COLOR", "1"),
            ("AI_AGENT", "x"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .into();
        let kept = ClaudeCli::new("claude", env.clone(), StripMode::Markers).environment;
        assert_eq!(kept.keys().map(String::as_str).collect::<Vec<_>>(), ["CLAUDE_CODE_MAX_OUTPUT_TOKENS", "COLORTERM", "LANG", "PATH", "TERM"]);
        let broad = ClaudeCli::new("claude", env, StripMode::Broad).environment;
        assert!(!broad.contains_key("CLAUDE_CODE_MAX_OUTPUT_TOKENS"));
        assert!(!broad.contains_key("CLAUDE_PID"), "Broad entfernt nie weniger als Markers");
    }

    /// Wie im Spike: ein NO_COLOR des Nutzers (keine Claude-Session) bleibt, in einer Claude-Session fliegt es raus.
    #[test]
    fn no_color_only_stripped_inside_a_claude_session() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let in_claude = names(&["PATH", "CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "claude_code_entrypoint", "NO_COLOR", "AI_AGENT", "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE"]);
        assert_eq!(
            env_names_to_strip(&in_claude, StripMode::Broad),
            names(&["CLAUDECODE", "CLAUDE_CODE_CHILD_SESSION", "claude_code_entrypoint", "NO_COLOR", "AI_AGENT"])
        );
        assert!(env_names_to_strip(&names(&["NO_COLOR", "PATH"]), StripMode::Broad).is_empty());
    }

    #[test]
    fn env0_parsing_handles_multiline_values_and_profile_noise() {
        let out = "motd beim Login\0A=1\0MULTI=zeile1\nB=falsch\0kein_gleich\0=leer\09X=nein\0PATH=/usr/bin\0";
        assert_eq!(
            parse_env0(out),
            [("A".to_string(), "1".to_string()), ("MULTI".to_string(), "zeile1\nB=falsch".to_string()), ("PATH".to_string(), "/usr/bin".to_string())]
        );
    }

    #[test]
    fn login_shell_env_overrides_base() {
        let fake = Fake::new(0, "\0PATH=/opt/bin\0NEU=1\0", "");
        let base: BTreeMap<String, String> = [("PATH".to_string(), "/usr/bin".to_string()), ("HOME".to_string(), "/h".to_string())].into();
        let env = login_shell_env(&fake, base);
        assert_eq!(env["PATH"], "/opt/bin");
        assert_eq!(env["NEU"], "1");
        assert_eq!(env["HOME"], "/h");
    }

    #[test]
    fn locate_prefers_local_bin_then_path_then_login_shell() {
        let env: BTreeMap<String, String> = [("PATH".to_string(), if cfg!(windows) { "C:/a;C:/b" } else { "/a:/b" }.to_string())].into();
        let exists = |wanted: &'static str| move |p: &str| p == wanted || (cfg!(windows) && p == format!("{wanted}.exe"));
        let nothing = || None;
        assert!(locate_binary(&env, "/h", &exists("/h/.local/bin/claude"), &nothing).unwrap().starts_with("/h/.local/bin/claude"));
        let on_path = if cfg!(windows) { "C:/b/claude" } else { "/b/claude" };
        assert!(locate_binary(&env, "/h", &exists(on_path), &nothing).unwrap().starts_with(on_path));
        assert_eq!(locate_binary(&env, "/h", &|_| false, &|| Some("/opt/claude".into())).as_deref(), Some("/opt/claude"));
        assert_eq!(locate_binary(&env, "/h", &|_| false, &|| Some("claude".into())), None, "nur absolute Pfade der Shell");
    }

    #[test]
    fn claude_launch_orders_arguments_and_sets_tile_env() {
        let cli = cli();
        let start = ClaudeStart {
            session_id: "u1",
            cwd: "/p",
            has_transcript: false,
            flags: LaunchFlags { model: "opus".into(), ..LaunchFlags::default() },
            prompt: Some("--bitte"),
            fullscreen: true,
            session_key: "k1",
            socket: "/tmp/kadrell.sock",
            exe: "/usr/bin/kadrell",
        };
        let spec = claude_launch(&cli, &start);
        assert_eq!(&spec.args[..4], ["--session-id", "u1", "--model", "opus"]);
        assert_eq!(spec.args[4], "--settings");
        assert_eq!(&spec.args[6..], ["--", "--bitte"]);
        assert_eq!(spec.env["KADRELL_SESSION_KEY"], "k1");
        assert_eq!(spec.env["KADRELL_SOCKET"], "/tmp/kadrell.sock");
        assert_eq!(spec.env["KADRELL"], "/usr/bin/kadrell");
        assert_eq!(spec.env["CLAUDE_CODE_NO_FLICKER"], "1");

        let resumed = claude_launch(&cli, &ClaudeStart { has_transcript: true, fullscreen: false, ..start });
        assert_eq!(&resumed.args[..2], ["--resume", "u1"]);
        assert!(!resumed.args.contains(&"--".to_string()), "kein Prompt bei bekannter Konversation");
        assert!(!resumed.env.contains_key("CLAUDE_CODE_NO_FLICKER"));
    }

    #[test]
    fn config_dir_follows_env_and_new_ids_are_uuids() {
        assert_eq!(cli().config_dir("/h"), "/h/.claude");
        let moved = ClaudeCli::new("c", [("CLAUDE_CONFIG_DIR".to_string(), "/x".to_string())].into(), StripMode::Markers);
        assert_eq!(moved.config_dir("/h"), "/x");
        assert_eq!(new_session_id().len(), 36);
    }

    #[test]
    fn system_runner_runs_a_real_process() {
        let (program, args): (&str, Vec<String>) = if cfg!(windows) { ("cmd", vec!["/C".into(), "echo hallo".into()]) } else { ("sh", vec!["-c".into(), "echo hallo".into()]) };
        let out = SystemRunner.run(program, &args, None, None, Duration::from_secs(10)).unwrap();
        assert_eq!(out.status, 0);
        assert!(out.stdout.contains("hallo"));
        assert!(SystemRunner.run("kadrell-gibt-es-nicht", &[], None, None, Duration::from_secs(1)).is_err());
    }
}
