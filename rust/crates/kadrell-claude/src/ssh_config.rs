//! Hosts aus `~/.ssh/config` für Remote-Sessions (Swift: `Services/SSHConfig.swift`). ssh selbst kann keine Hosts
//! auflisten (`ssh -G` löst nur einen auf), deshalb ein kleiner Parser, der `Include` verfolgt: Schlüsselwörter ohne
//! Groß-/Kleinschreibung, Pfade relativ zu `~/.ssh`, Globs wie `config.d/*`. Muster mit `*`, `?`, `!` sind keine Hosts.

use crate::cli::CommandRunner;
use crate::paths;
use std::collections::HashSet;
use std::time::Duration;

/// `~/.ssh/config`.
pub fn default_config(home: &str) -> String {
    format!("{home}/.ssh/config")
}

pub fn hosts(config: &str, home: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let base = paths::parent(config);
    read(config, &base, home, &mut visited, &mut |h| {
        if seen.insert(h.to_string()) {
            out.push(h.to_string());
        }
    });
    out
}

fn read(file: &str, base: &str, home: &str, visited: &mut HashSet<String>, emit: &mut dyn FnMut(&str)) {
    if !visited.insert(paths::standardize(file)) {
        return;
    }
    let Ok(content) = std::fs::read_to_string(file) else { return };
    for raw in content.lines() {
        let line = raw.trim_matches([' ', '\t']);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split([' ', '\t', '=']).filter(|p| !p.is_empty()).collect();
        if parts.len() < 2 {
            continue;
        }
        match parts[0].to_lowercase().as_str() {
            "host" => parts[1..].iter().filter(|h| !h.contains(['*', '?', '!'])).for_each(|h| emit(h)),
            "include" => {
                for pattern in &parts[1..] {
                    let expanded = paths::expand_tilde(pattern, home);
                    let abs = if paths::is_abs(&expanded) { expanded } else { paths::join(base, &expanded) };
                    for path in glob(&abs) {
                        read(&path, base, home, visited, emit);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Glob nur im letzten Pfadbestandteil (`config.d/*`), sortiert. Ohne Platzhalter: der Pfad selbst, falls es ihn gibt.
fn glob(pattern: &str) -> Vec<String> {
    let pattern = paths::fwd(pattern);
    let dir = paths::parent(&pattern);
    let name = paths::last_component(&pattern);
    if !name.contains(['*', '?']) {
        return if std::path::Path::new(&pattern).exists() { vec![pattern] } else { Vec::new() };
    }
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| wildcard(name, n))
        .map(|n| paths::join(&dir, &n))
        .collect();
    found.sort();
    found
}

/// `*` (beliebig viel) und `?` (ein Zeichen), sonst Zeichen für Zeichen.
fn wildcard(pattern: &str, s: &str) -> bool {
    fn go(p: &[char], s: &[char]) -> bool {
        match p.first() {
            None => s.is_empty(),
            Some('*') => (0..=s.len()).any(|i| go(&p[1..], &s[i..])),
            Some('?') => !s.is_empty() && go(&p[1..], &s[1..]),
            Some(c) => s.first() == Some(c) && go(&p[1..], &s[1..]),
        }
    }
    go(&pattern.chars().collect::<Vec<_>>(), &s.chars().collect::<Vec<_>>())
}

/// Zuletzt verbundene Hosts, neueste vorn, höchstens 20. Ersetzt eine Favoritenliste: was man nutzt, steht oben.
/// Die Liste selbst gehört in die Profil-Einstellungen (`remote.recent`).
pub fn record_use(recent: &[String], host: &str) -> Vec<String> {
    std::iter::once(host.to_string()).chain(recent.iter().filter(|h| *h != host).cloned()).take(20).collect()
}

/// Ein Wort für die Remote-Shell: in `'…'`, jedes `'` als `'\''`. tmux-Namen kommen aus Palette und Remote-Ausgabe.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// tmux-Sessions auf dem Host. nil, wenn ssh scheitert; leer, wenn dort kein tmux-Server läuft.
pub fn tmux_sessions(runner: &dyn CommandRunner, host: &str) -> Option<Vec<String>> {
    let args: Vec<String> =
        ["-o", "BatchMode=yes", "-o", "ConnectTimeout=5", "--", host, "tmux list-sessions -F '#S' 2>/dev/null || true"].map(String::from).to_vec();
    let r = runner.run("ssh", &args, None, None, Duration::from_secs(60)).ok().filter(|r| r.status == 0)?;
    Some(r.stdout.lines().filter(|l| !l.is_empty()).map(String::from).collect())
}

/// Der Remote-Befehl der Kachel: benannte tmux-Session anlegen/anhängen, sonst die zuletzt benutzte.
pub fn remote_tmux_command(tmux_session: Option<&str>) -> String {
    tmux_session.map(|s| format!("tmux new -As {}", shell_quote(s))).unwrap_or_else(|| "tmux attach || tmux new -As main".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::testdir::TempDir;

    #[test]
    fn hosts_follow_includes_and_skip_patterns() {
        let dir = TempDir::new("ssh");
        dir.write(
            "config",
            "# Kommentar\nHost 192.168.2.69\n  HostName 192.168.2.69\nHost *\n  ServerAliveInterval 60\nHost examplehost vault\nHost !x\nMatch all\ninclude config.d/*\nHost dup\n",
        );
        dir.write("config.d/a", "host peon\nInclude ../config\nHost dup\n");
        dir.write("config.d/b", "Host b?\n Host zed\n");
        assert_eq!(hosts(&dir.sub("config"), &dir.path()), ["192.168.2.69", "examplehost", "vault", "peon", "dup", "zed"]);
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("main"), "'main'");
        assert_eq!(shell_quote("it's; rm -rf ~"), "'it'\\''s; rm -rf ~'");
    }

    #[test]
    fn missing_config_is_empty() {
        assert!(hosts("/nonexistent/config", "/nonexistent").is_empty());
    }

    #[test]
    fn recent_hosts_move_to_front_and_are_capped() {
        let recent: Vec<String> = (0..25).map(|i| format!("h{i}")).collect();
        let next = record_use(&recent, "h3");
        assert_eq!(next[0], "h3");
        assert_eq!(next.len(), 20);
        assert_eq!(next.iter().filter(|h| *h == "h3").count(), 1);
    }

    #[test]
    fn wildcards_and_tmux_command() {
        assert!(wildcard("config.*", "config.d"));
        assert!(wildcard("a?c", "abc") && !wildcard("a?c", "ac"));
        assert_eq!(remote_tmux_command(Some("it's")), "tmux new -As 'it'\\''s'");
        assert_eq!(remote_tmux_command(None), "tmux attach || tmux new -As main");
    }
}
