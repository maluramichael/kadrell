//! Remote-Kacheln: eine kleine ssh-Verbindung zu einem Host, dort `tmux` (Swift: `AttachManager.sshLaunch`,
//! `SSHConfig`, `AppDelegate.startRemote`). Der Befehl ist eine reine Funktion ([`launch`]), die Abfragen (`tmux -V`,
//! `tmux list-sessions`) gehen über das System-ssh (Windows: das mitgelieferte OpenSSH, `ssh.exe`).
//!
//! Die Remote-Shell kann alles sein (PowerShell, cmd, fish), deshalb läuft dort kein Shell-Code: erst fragt eine eigene
//! Verbindung per `tmux -V`, ob es dort tmux gibt, dann verbindet die Kachel mit dem tmux-Befehl oder ganz ohne.
//! `--` vor dem Host: ein Host wie `-oProxyCommand=…` bleibt Hostname und wird keine Option.

pub mod dialog;

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Einstellung mit den zuletzt verbundenen Hosts (neueste vorn), wie in der Mac-App.
pub const RECENT_KEY: &str = "remote.recent";
pub const MAX_RECENT: usize = 20;

/// Zuletzt verbundene Hosts nach einer Verbindung: `host` vorn, ohne Doppelte, höchstens [`MAX_RECENT`].
pub fn record_use(recent: &[String], host: &str) -> Vec<String> {
    let mut out = vec![host.to_string()];
    out.extend(recent.iter().filter(|h| h.as_str() != host).cloned());
    out.truncate(MAX_RECENT);
    out
}

/// Hosts für Auswahl und Palette: zuletzt benutzte vorn, danach der Rest aus `~/.ssh/config`.
pub fn host_choices(recent: &[String], hosts: &[String]) -> Vec<String> {
    let mut out = recent.to_vec();
    out.extend(hosts.iter().filter(|h| !recent.contains(h)).cloned());
    out
}

/// Ein Wort für die Remote-Shell: in `'…'`, jedes `'` als `'\''`.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Befehl auf dem Host: die genannte tmux-Session anlegen oder anhängen, sonst an die laufende tmux hängen und ohne
/// laufende Session `main` anlegen.
pub fn remote_command(tmux: Option<&str>) -> String {
    match tmux {
        Some(name) => format!("tmux new -As {}", shell_quote(name)),
        None => "tmux attach || tmux new -As main".to_string(),
    }
}

/// Programm und Argumente der Kachel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

/// Aufruf der Kachel. `has_tmux` kommt aus [`probe_tmux`]. Ohne tmux öffnet ssh die normale Shell; auf Unix sagt das
/// ein lokales `sh` vorher (`unix`), unter Windows gibt es kein `sh`, dort öffnet ssh ohne Meldung die Shell.
pub fn launch(ssh: &str, host: &str, tmux: Option<&str>, has_tmux: bool, unix: bool, message: &str) -> Launch {
    let connect = ["-t", "-o", "ConnectTimeout=5", "--"];
    if has_tmux {
        let mut args: Vec<String> = connect.iter().map(|s| s.to_string()).collect();
        args.extend([host.to_string(), remote_command(tmux)]);
        return Launch { program: ssh.to_string(), args };
    }
    if unix {
        let script = r#"printf '%s\n' "$2"; exec "$0" -t -o ConnectTimeout=5 -- "$1""#;
        let args = ["-c", script, ssh, host, message].map(String::from).to_vec();
        return Launch { program: "/bin/sh".into(), args };
    }
    Launch { program: ssh.to_string(), args: connect.iter().map(|s| s.to_string()).chain([host.to_string()]).collect() }
}

/// Das ssh des Systems: Windows das mitgelieferte OpenSSH, sonst `/usr/bin/ssh`; fehlt es dort, `ssh` aus dem PATH.
pub fn ssh_binary(is_file: &dyn Fn(&str) -> bool) -> String {
    let system = if cfg!(windows) {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        format!("{root}\\System32\\OpenSSH\\ssh.exe")
    } else {
        "/usr/bin/ssh".to_string()
    };
    if is_file(&system) { system } else { "ssh".to_string() }
}

pub fn system_ssh() -> String {
    ssh_binary(&|p| std::path::Path::new(p).is_file())
}

/// Schlüssel einer Remote-Session: das Präfix macht ihn lesbar, erkannt wird sie am `host`.
pub fn new_key() -> String {
    format!("{}{}", kadrell_core::Session::REMOTE_PREFIX, kadrell_claude::cli::new_session_id())
}

/// Führt `ssh -n -o BatchMode=yes -o ConnectTimeout=5 -- host <command>` aus und gibt die Standardausgabe zurück.
/// `None`: ssh ging nicht oder brauchte länger als 10 s. Geprüft wird die Ausgabe, nicht der Exit-Code (PowerShell macht
/// aus `command -v tmux` ein `Get-Command` mit Exit 0).
pub fn ssh_output(ssh: &str, host: &str, command: &str) -> Option<String> {
    let mut cmd = Command::new(ssh);
    cmd.args(["-n", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5", "--", host, command]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(_) => break,
            None if started.elapsed() > Duration::from_secs(10) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    Some(out)
}

/// Gibt es auf dem Host tmux?
pub fn probe_tmux(ssh: &str, host: &str) -> bool {
    ssh_output(ssh, host, "tmux -V").is_some_and(|out| has_tmux_output(&out))
}

pub fn has_tmux_output(out: &str) -> bool {
    out.trim_start().starts_with("tmux")
}

/// tmux-Sessions auf dem Host. `None`, wenn ssh scheitert; leer, wenn dort kein tmux-Server läuft.
pub fn tmux_sessions(ssh: &str, host: &str) -> Option<Vec<String>> {
    ssh_output(ssh, host, "tmux list-sessions -F '#S' 2>/dev/null || true").map(|out| parse_sessions(&out))
}

pub fn parse_sessions(out: &str) -> Vec<String> {
    out.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

/// Was die Eingabe `host`, `host:` oder `host:text` des Remote-Dialogs bedeutet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    /// Hosts filtern (leer: alle).
    Hosts(String),
    /// tmux-Sessions des Hosts, gefiltert nach `text`; `text` ist zugleich der Name einer neuen Session.
    Sessions { host: String, text: String },
}

pub fn parse_query(q: &str) -> Query {
    match q.split_once(':') {
        None => Query::Hosts(q.trim().to_string()),
        Some((host, text)) => Query::Sessions { host: host.trim().to_string(), text: text.trim().to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recent_hosts_are_newest_first_unique_and_capped() {
        assert_eq!(record_use(&v(&["a", "b"]), "b"), v(&["b", "a"]));
        assert_eq!(record_use(&[], "x"), v(&["x"]));
        let many: Vec<String> = (0..25).map(|i| format!("h{i}")).collect();
        let out = record_use(&many, "neu");
        assert_eq!((out.len(), out[0].as_str()), (MAX_RECENT, "neu"));
    }

    #[test]
    fn choices_put_recent_first_without_duplicates() {
        assert_eq!(host_choices(&v(&["b"]), &v(&["a", "b", "c"])), v(&["b", "a", "c"]));
        assert_eq!(host_choices(&[], &v(&["a"])), v(&["a"]));
    }

    #[test]
    fn quoting_survives_single_quotes() {
        assert_eq!(shell_quote("main"), "'main'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(remote_command(Some("a b")), "tmux new -As 'a b'");
        assert_eq!(remote_command(None), "tmux attach || tmux new -As main");
    }

    #[test]
    fn launch_with_tmux_runs_ssh_t_with_the_tmux_command() {
        let l = launch("ssh", "vault", Some("main"), true, false, "m");
        assert_eq!(l.program, "ssh");
        assert_eq!(l.args, ["-t", "-o", "ConnectTimeout=5", "--", "vault", "tmux new -As 'main'"]);
        let l = launch("ssh", "vault", None, true, true, "m");
        assert_eq!(l.args.last().map(String::as_str), Some("tmux attach || tmux new -As main"));
    }

    #[test]
    fn a_host_that_looks_like_an_option_stays_a_host() {
        let l = launch("ssh", "-oProxyCommand=evil", None, false, false, "m");
        assert_eq!(l.args, ["-t", "-o", "ConnectTimeout=5", "--", "-oProxyCommand=evil"]);
    }

    #[test]
    fn launch_without_tmux_is_a_plain_shell_and_unix_says_so() {
        let windows = launch("ssh.exe", "vault", Some("x"), false, false, "Kein tmux");
        assert_eq!((windows.program.as_str(), windows.args.len()), ("ssh.exe", 5), "kein Shell-Code auf Windows");
        assert!(!windows.args.iter().any(|a| a.contains("tmux")));
        let unix = launch("/usr/bin/ssh", "vault", Some("x"), false, true, "Kein tmux");
        assert_eq!(unix.program, "/bin/sh");
        assert_eq!(&unix.args[2..], ["/usr/bin/ssh", "vault", "Kein tmux"]);
        assert!(unix.args[1].contains("exec \"$0\" -t"));
    }

    #[test]
    fn ssh_binary_prefers_the_system_one() {
        assert!(ssh_binary(&|_| true).to_lowercase().ends_with(if cfg!(windows) { "ssh.exe" } else { "/usr/bin/ssh" }));
        assert_eq!(ssh_binary(&|_| false), "ssh");
    }

    #[test]
    fn tmux_output_is_judged_by_text_not_exit_code() {
        assert!(has_tmux_output("tmux 3.4\n"));
        assert!(has_tmux_output("\r\ntmux next-3.5"));
        assert!(!has_tmux_output("Get-Command : The term 'tmux' is not recognized"));
        assert!(!has_tmux_output(""));
        assert_eq!(parse_sessions("main\r\n\nwork\n"), v(&["main", "work"]));
    }

    #[test]
    fn query_modes() {
        assert_eq!(parse_query(""), Query::Hosts(String::new()));
        assert_eq!(parse_query("va"), Query::Hosts("va".into()));
        assert_eq!(parse_query("vault:"), Query::Sessions { host: "vault".into(), text: String::new() });
        assert_eq!(parse_query("vault: neu"), Query::Sessions { host: "vault".into(), text: "neu".into() });
    }

    #[test]
    fn keys_carry_the_ssh_prefix() {
        let k = new_key();
        assert!(k.starts_with("ssh-") && k.len() == 4 + 36);
        assert_ne!(k, new_key());
    }
}
