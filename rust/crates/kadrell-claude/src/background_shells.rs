//! Erkennt Kommandos aus dem Bash-Werkzeug, die in einer Session noch laufen („1 shell still running“ in Claude Code),
//! und übernimmt Hintergrund-Sessions aus `claude --bg` (Swift: `Services/BackgroundShells.swift` und `offerAdopt` in
//! `App/AppDelegate.swift`).
//!
//! Claude Code startet jedes Bash-Kommando als eigenen Kindprozess seiner selbst, immer als Login-Shell, die zuerst einen
//! Schnappschuss der Umgebung einliest (`/bin/zsh -c source ~/.claude/shell-snapshots/snapshot-…`). Genau daran hängt die
//! Erkennung: die anderen Kinder eines Claude-Prozesses (MCP-Server über npx/uvx/ssh/node, `caffeinate`) tragen den
//! Schnappschuss nicht.

use crate::agent::{self, AgentInfo};
use crate::cli::{ClaudeCli, CliError, CommandRunner};
use crate::paths;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Teil der Kommandozeile jeder Shell, die das Bash-Werkzeug startet.
pub const MARKER: &str = "/shell-snapshots/snapshot-";

/// Prozessinfos, wie sie die Erkennung braucht. Die echte Quelle ist plattformabhängig (macOS: libproc, sonst `ps`).
pub trait ProcessTable {
    /// Direkte Kindprozesse, wie `pgrep -P`.
    fn children(&self, pid: u32) -> Vec<u32>;
    /// Programmname (Dateiname ohne Ordner und ohne `.exe`).
    fn image_name(&self, pid: u32) -> Option<String>;
    /// Kommandozeile am Stück, wie `ps -o args=`; nil, wenn sie nicht lesbar ist.
    fn arguments(&self, pid: u32) -> Option<String>;
}

/// pids der Claude-Prozesse, unter denen noch mindestens ein solches Kommando läuft.
pub fn running(pids: &[u32], table: &dyn ProcessTable) -> HashSet<u32> {
    pids.iter().copied().filter(|pid| table.children(*pid).into_iter().any(|c| is_tool_shell(c, table))).collect()
}

/// Shell-Binary zuerst (billig), die Kommandozeile nur für Shells: MCP-Server sind meist node, Python oder ssh und
/// fallen schon hier raus.
fn is_tool_shell(pid: u32, table: &dyn ProcessTable) -> bool {
    let Some(name) = table.image_name(pid) else { return false };
    if !name.to_lowercase().trim_end_matches(".exe").ends_with("sh") {
        return false;
    }
    table.arguments(pid).is_some_and(|a| paths::fwd(&a).contains(MARKER))
}

/// Eine Momentaufnahme aus `ps -axo pid=,ppid=,args=` (macOS, Linux): ein Aufruf statt einer je Prozess.
#[derive(Default)]
pub struct PsSnapshot {
    rows: HashMap<u32, (u32, String)>,
}

impl PsSnapshot {
    pub const ARGS: [&'static str; 2] = ["-axo", "pid=,ppid=,args="];

    pub fn parse(output: &str) -> PsSnapshot {
        let rows = output
            .lines()
            .filter_map(|line| {
                let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
                let (ppid, args) = rest.trim_start().split_once(char::is_whitespace)?;
                Some((pid.parse().ok()?, (ppid.parse().ok()?, args.trim().to_string())))
            })
            .collect();
        PsSnapshot { rows }
    }

    /// `ps` einmal aufrufen. Leer, wenn es scheitert.
    pub fn take(runner: &dyn CommandRunner) -> PsSnapshot {
        let args: Vec<String> = Self::ARGS.iter().map(|s| s.to_string()).collect();
        match runner.run("ps", &args, None, None, Duration::from_secs(10)) {
            Ok(r) if r.status == 0 => PsSnapshot::parse(&r.stdout),
            _ => PsSnapshot::default(),
        }
    }
}

impl ProcessTable for PsSnapshot {
    fn children(&self, pid: u32) -> Vec<u32> {
        self.rows.iter().filter(|(_, (ppid, _))| *ppid == pid).map(|(p, _)| *p).collect()
    }

    fn image_name(&self, pid: u32) -> Option<String> {
        let args = &self.rows.get(&pid)?.1;
        let first = args.split_whitespace().next()?;
        Some(paths::last_component(first).to_string())
    }

    fn arguments(&self, pid: u32) -> Option<String> {
        self.rows.get(&pid).map(|(_, a)| a.clone())
    }
}

/// Eine laufende Hintergrund-Session, die Kadrell übernehmen kann.
#[derive(Clone, Debug, PartialEq)]
pub struct AdoptCandidate {
    pub agent: AgentInfo,
    /// Arbeitet gerade: Stoppen bricht laufende Arbeit ab.
    pub busy: bool,
}

/// Ergebnis von `plan_adoption`.
#[derive(Clone, Debug, PartialEq)]
pub struct AdoptPlan {
    /// Interaktive Sessions anderswo (tmux, iTerm): nur ein Hinweis im Leerzustand, keine Übernahme.
    pub other_interactive_count: usize,
    pub candidates: Vec<AdoptCandidate>,
}

impl AdoptPlan {
    /// Arbeitende Sessions per blankem Enter zu stoppen wäre destruktiv: sobald eine busy ist, braucht es die
    /// ausdrückliche Bestätigung.
    pub fn destructive(&self) -> bool {
        self.candidates.iter().any(|c| c.busy)
    }
}

/// Fremde Claude-Sessions beim Start erfassen: alles, was Kadrell nicht schon besitzt (`owned` = sessionIds).
/// Übernommen werden nur laufende `--bg`-Sessions und nur im Standardprofil (`default_profile`), sonst stiehlt ein
/// Testprofil sie.
pub fn plan_adoption(agents: &[AgentInfo], owned: &HashSet<String>, default_profile: bool) -> AdoptPlan {
    let elsewhere: Vec<&AgentInfo> = agents.iter().filter(|a| !owned.contains(&a.session_id)).collect();
    let other_interactive_count = elsewhere.iter().filter(|a| a.kind == "interactive").count();
    let candidates = if default_profile {
        elsewhere.into_iter().filter(|a| a.is_running_background()).map(|a| AdoptCandidate { busy: a.status.as_deref() == Some("busy"), agent: a.clone() }).collect()
    } else {
        Vec::new()
    };
    AdoptPlan { other_interactive_count, candidates }
}

/// Die Session, wie sie danach in Kadrell steht. Die Kurz-Id bleibt Schlüssel, damit Gruppen und Auswahl passen; der
/// Auto-Name wird verworfen. Danach startet die Kachel wie jede andere mit `--resume <sessionId>`
/// (`cli::claude_launch` mit `has_transcript`).
#[derive(Clone, Debug, PartialEq)]
pub struct AdoptedSession {
    pub id: String,
    pub cwd: String,
    pub started_at: f64,
    pub session_id: String,
    pub name: String,
}

pub fn adopted_session(agent: &AgentInfo) -> Option<AdoptedSession> {
    Some(AdoptedSession {
        id: agent.short_id.clone()?,
        cwd: agent.cwd.clone(),
        started_at: agent.started_at,
        session_id: agent.session_id.clone(),
        name: if agent::is_auto_name(&agent.name, &agent.cwd) { String::new() } else { agent.name.clone() },
    })
}

/// `claude stop <id>` für jeden Kandidaten, kein `claude rm` (das löscht ggf. den Worktree, in dem die Session arbeitet).
/// Liefert die übernommenen Sessions und eine Fehlerzeile je Fehlschlag (`· <name>: <erste Zeile>`).
pub fn adopt(cli: &ClaudeCli, runner: &dyn CommandRunner, plan: &AdoptPlan) -> (Vec<AdoptedSession>, Vec<String>) {
    let mut adopted = Vec::new();
    let mut failures = Vec::new();
    for c in &plan.candidates {
        let Some(session) = adopted_session(&c.agent) else { continue };
        match cli.stop(runner, &session.id) {
            Ok(()) => adopted.push(session),
            Err(e) => failures.push(format!("· {}: {}", c.agent.name, CliError::first_line(&e.output))),
        }
    }
    (adopted, failures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{RunOutput, StripMode};
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    /// Die Kette nach, die Claude Code aufmacht: ein Prozess startet eine Shell, die zuerst einen Schnappschuss einliest
    /// und dann wartet; daneben MCP-Server und Helfer.
    const PS: &str = "\
    1     0 /sbin/launchd
  100     1 claude --resume abc
  101   100 /bin/zsh -c source /Users/x/.claude/shell-snapshots/snapshot-zsh-1.sh 2>/dev/null || true && sleep 30
  102   100 node /opt/mcp-server.js --path /Users/x/.claude/shell-snapshots/snapshot-fake
  103   100 /usr/bin/caffeinate -i
  200     1 claude --resume def
  201   200 /bin/zsh -c exec sleep 30
 300 1 ohne-pid-spalte-kaputt";

    #[test]
    fn children_and_arguments_come_from_the_snapshot() {
        let t = PsSnapshot::parse(PS);
        let mut kids = t.children(100);
        kids.sort_unstable();
        assert_eq!(kids, [101, 102, 103]);
        assert!(t.arguments(101).unwrap().contains("sleep 30"));
        assert_eq!(t.image_name(101).as_deref(), Some("zsh"));
        assert_eq!(t.image_name(103).as_deref(), Some("caffeinate"));
    }

    #[test]
    fn running_sees_shell_with_snapshot() {
        assert_eq!(running(&[100], &PsSnapshot::parse(PS)), HashSet::from([100]));
    }

    /// MCP-Server und Helfer wie `caffeinate` sind ebenfalls Kinder des Claude-Prozesses, zählen aber nicht.
    #[test]
    fn running_ignores_other_children() {
        assert!(running(&[200], &PsSnapshot::parse(PS)).is_empty());
        let only_others = "100 1 claude\n102 100 node /x/shell-snapshots/snapshot-fake\n103 100 /usr/bin/caffeinate -i";
        assert!(running(&[100], &PsSnapshot::parse(only_others)).is_empty());
    }

    #[test]
    fn running_ignores_unknown_pid() {
        assert!(running(&[999_999], &PsSnapshot::parse(PS)).is_empty());
    }

    #[test]
    fn windows_bash_with_backslashes_counts() {
        // Der Name endet auf `.exe`, nicht auf `sh`, und der Schnappschuss-Pfad hat Backslashes.
        struct Win;
        impl ProcessTable for Win {
            fn children(&self, _: u32) -> Vec<u32> { vec![11] }
            fn image_name(&self, _: u32) -> Option<String> { Some("bash.exe".into()) }
            fn arguments(&self, _: u32) -> Option<String> { Some("bash.exe -c source C:\\Users\\x\\.claude\\shell-snapshots\\snapshot-bash-1.sh".into()) }
        }
        assert_eq!(running(&[10], &Win), HashSet::from([10]));
    }

    struct Fake {
        fail: Vec<&'static str>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl CommandRunner for Fake {
        fn run(&self, _: &str, args: &[String], _: Option<&BTreeMap<String, String>>, _: Option<&str>, _: Duration) -> Result<RunOutput, String> {
            self.calls.borrow_mut().push(args.to_vec());
            if self.fail.iter().any(|f| args.iter().any(|a| a == f)) {
                return Ok(RunOutput { status: 1, stdout: String::new(), stderr: "no such job\nzweite Zeile".into() });
            }
            Ok(RunOutput::default())
        }
    }

    fn agents() -> Vec<AgentInfo> {
        agent::decode_list(
            r#"[{"id":"aa11bb22","pid":5,"cwd":"/p/proj","kind":"background","startedAt":1,"sessionId":"aa11bb22-x","name":"proj-ab","status":"busy"},
                {"id":"cc33dd44","pid":6,"cwd":"/p/proj","kind":"background","startedAt":2,"sessionId":"cc33dd44-x","name":"Mein Fix","status":"idle"},
                {"id":"ee55ff66","cwd":"/p/proj","kind":"background","startedAt":3,"sessionId":"ee55ff66-x","name":"gestoppt","state":"stopped"},
                {"pid":7,"cwd":"/p/proj","kind":"interactive","startedAt":4,"sessionId":"i1","name":"proj-1f","status":"idle"},
                {"id":"bb","pid":8,"cwd":"/p","kind":"background","startedAt":5,"sessionId":"owned-1","name":"schon da","status":"idle"}]"#,
        )
        .unwrap()
    }

    #[test]
    fn plan_lists_running_background_sessions_not_owned() {
        let plan = plan_adoption(&agents(), &HashSet::from(["owned-1".to_string()]), true);
        assert_eq!(plan.candidates.iter().map(|c| c.agent.name.as_str()).collect::<Vec<_>>(), ["proj-ab", "Mein Fix"]);
        assert!(plan.destructive(), "eine arbeitet gerade");
        assert_eq!(plan.other_interactive_count, 1);
        let quiet = plan_adoption(&agents()[1..2], &HashSet::new(), true);
        assert!(!quiet.destructive());
    }

    #[test]
    fn test_profiles_never_adopt() {
        let plan = plan_adoption(&agents(), &HashSet::new(), false);
        assert!(plan.candidates.is_empty());
        assert_eq!(plan.other_interactive_count, 1, "der Hinweis im Leerzustand bleibt");
    }

    #[test]
    fn adopt_stops_each_and_reports_failures_in_one_list() {
        let cli = ClaudeCli::new("claude", BTreeMap::new(), StripMode::Markers);
        let plan = plan_adoption(&agents(), &HashSet::new(), true);
        let fake = Fake { fail: vec!["cc33dd44"], calls: RefCell::default() };
        let (adopted, failures) = adopt(&cli, &fake, &plan);
        assert_eq!(fake.calls.borrow().iter().map(|c| c.join(" ")).collect::<Vec<_>>(), ["stop aa11bb22", "stop cc33dd44", "stop bb"]);
        assert_eq!(adopted.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["aa11bb22", "bb"]);
        assert_eq!(adopted[0].name, "", "Auto-Name wird verworfen");
        assert_eq!(adopted[0].session_id, "aa11bb22-x", "Konversation bleibt für --resume");
        assert_eq!(failures, ["· Mein Fix: no such job"]);
    }
}
