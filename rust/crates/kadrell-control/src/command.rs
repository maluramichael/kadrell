//! `kadrell <befehl>`: Befehle, Flags, Hilfetext und Fehlermeldungen wortgleich zur Mac-App
//! (`app/Kadrell/Services/Control.swift`). Hier nur der reine Teil: parsen, Tastennamen, Hilfetext.
//! Auf beiden Systemen gelten alle Befehle (eine Codebasis), dazu `kill-server` von der Windows-Version.

use kadrell_core::targets::TargetError;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Fehler mit der Meldung, die als `kadrell: <meldung>` auf stderr erscheint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlError(pub String);

impl ControlError {
    pub fn new(message: impl Into<String>) -> ControlError {
        ControlError(message.into())
    }
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ControlError {}

impl From<TargetError> for ControlError {
    fn from(e: TargetError) -> ControlError {
        ControlError(e.0)
    }
}

pub type Result<T> = std::result::Result<T, ControlError>;

/// Anordnung der Kacheln (`LayoutMode` der Mac-App, dieselben Rohwerte wie in `ls --json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    #[default]
    Grid,
    Main,
    Spiral,
    Custom,
    Scroll,
    Row,
    Stack,
}

impl LayoutMode {
    pub const ALL: [LayoutMode; 7] =
        [LayoutMode::Grid, LayoutMode::Main, LayoutMode::Spiral, LayoutMode::Custom, LayoutMode::Scroll, LayoutMode::Row, LayoutMode::Stack];

    pub fn as_str(self) -> &'static str {
        match self {
            LayoutMode::Grid => "grid",
            LayoutMode::Main => "main",
            LayoutMode::Spiral => "spiral",
            LayoutMode::Custom => "custom",
            LayoutMode::Scroll => "scroll",
            LayoutMode::Row => "row",
            LayoutMode::Stack => "stack",
        }
    }

    pub fn from_raw(raw: &str) -> Option<LayoutMode> {
        LayoutMode::ALL.into_iter().find(|m| m.as_str() == raw)
    }
}

/// Alle Angaben von `kadrell new`, nach den Prüfungen der Kombinationen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewSession {
    pub target: Option<String>,
    pub dir: Option<String>,
    pub name: Option<String>,
    pub detached: bool,
    pub prompt: Option<String>,
    pub resume: Option<String>,
    pub host: Option<String>,
    pub tmux: Option<String>,
    pub pi: bool,
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// Zustandsmeldung von außen, Quelle sind die Hooks der Agent-CLIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub target: Option<String>,
    pub state: String,
    pub session_id: Option<String>,
    pub title: Option<String>,
    pub waiting_for: Option<String>,
    pub message: Option<String>,
    pub first_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Help,
    List { json: bool },
    /// Hosts aus `~/.ssh/config` (folgt `Include`) und zuletzt verbundene.
    SshHosts { json: bool },
    NewGroup { dir: String, name: Option<String>, color: Option<String> },
    New(NewSession),
    Select { target: Option<String>, add: bool },
    Layout(LayoutMode),
    Zoom { target: Option<String> },
    Rename { target: Option<String>, name: String },
    Move { target: Option<String>, group: String },
    SetGroup { target: Option<String>, name: Option<String>, color: Option<String>, favorite: Option<bool> },
    Stop { target: Option<String> },
    Resume { target: Option<String> },
    Kill { target: Option<String> },
    KillGroup { target: Option<String> },
    Send { target: Option<String>, text: String, enter: bool, keys: bool },
    Capture { target: Option<String>, all: bool },
    Status(Status),
    /// Von der Windows-Version (wie tmux): Kadrell sauber beenden, ohne das Fenster zu bedienen.
    KillServer,
}

pub const STATES: [&str; 3] = ["working", "waiting", "idle"];
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub const USAGE: &str = "Kadrell fernsteuern (die App muss laufen, sonst wird sie gestartet).

Ziele (-t): Session-Key oder dessen Anfang, sessionId, Titel; Gruppen-Id oder deren Anfang, Gruppenname.
Ohne -t gilt die Session, in der das Kommando läuft, sonst die fokussierte.
Aus einer Session heraus nur Sessions der eigenen Gruppe, außer „Sessions dürfen andere Sessions steuern“ ist an.

  kadrell ls [--json]                                  Gruppen und Sessions auflisten
  kadrell ssh-hosts [--json]                           Hosts aus ~/.ssh/config und zuletzt verbundene
  kadrell new-group <ordner> [--name N] [--color #rrggbb]
                                                       Gruppe anlegen (als Favorit, bleibt auch leer stehen)
  kadrell new [-t gruppe] [-c ordner] [--name N] [-d] [--pi] [--resume sessionId]
              [--model alias|id] [--effort low|medium|high|xhigh|max] [prompt …]
                                                       Session starten, optional mit erster Nachricht;
                                                       -d: im Hintergrund, Auswahl bleibt;
                                                       --model/--effort: nur für diese Claude-Session statt der
                                                       Einstellung (auch mit --resume, nicht mit --pi oder --host);
                                                       --pi: Pi mit dem lokalen Modell der Pi-Sessions;
                                                       --resume: bestehende Konversation übernehmen
                                                       (Claude darf dort nicht mehr laufen, -c = ihr Ordner)
  kadrell new --host H [--tmux S] [-d] [--name N]      Remote-Kachel per ssh zu H (Host aus ~/.ssh/config),
                                                       hängt sich an dessen tmux (--tmux: Session S)
  kadrell select [-t session|gruppe] [-a]              zeigen (-a: zur Auswahl dazu/weg)
  kadrell layout grid|main|spiral|custom|scroll|stack Layout
  kadrell zoom [-t session]                            Zoom ein/aus
  kadrell rename [-t session] <name>                   umbenennen (leer = Titel von Claude Code)
  kadrell move [-t session] <gruppe>                   Session in eine andere Gruppe verschieben, läuft weiter
  kadrell set-group [-t gruppe] [--name N] [--color #rrggbb] [--favorite on|off]
  kadrell stop [-t session]                            Claude beenden, Kachel bleibt
  kadrell resume [-t session]                          Claude wieder starten
  kadrell kill [-t session]                            Session beenden und entfernen
  kadrell kill-group [-t gruppe]                       Gruppe samt Sessions beenden und entfernen
  kadrell send [-t session] [--no-enter] <text …>      Text eingeben und abschicken
  kadrell send [-t session] -k <taste …>               Tasten: Enter Escape Tab Up Down Left Right BSpace C-c C-d
  kadrell capture [-t session] [--all]                 Bildschirm ausgeben (--all: mit Verlauf)
  kadrell status working|waiting|idle [-t session] [--session-id ID] [--title T] [--waiting-for TEXT]
                 [--message TEXT] [--first-prompt TEXT]   Zustand melden (nutzen die Hooks der CLIs)
  kadrell hook claude                                  Claude-Code-Hook: liest das Hook-JSON von stdin, meldet status
  kadrell kill-server                                  Kadrell beenden, alle Sessions enden (beim nächsten Start --resume)

Die Session-Keys stehen in `kadrell ls`. Exit-Code 1 bei Fehlern, Meldung auf stderr.";

/// Tastennamen für `send -k`, angelehnt an tmux `send-keys`.
pub fn key_bytes(name: &str) -> Option<&'static str> {
    Some(match name.to_lowercase().as_str() {
        "enter" => "\r",
        "escape" | "esc" => "\u{1b}",
        "tab" => "\t",
        "bspace" => "\u{7f}",
        "space" => " ",
        "up" => "\u{1b}[A",
        "down" => "\u{1b}[B",
        "right" => "\u{1b}[C",
        "left" => "\u{1b}[D",
        "c-c" => "\u{03}",
        "c-d" => "\u{04}",
        "c-l" => "\u{0c}",
        "c-u" => "\u{15}",
        _ => return None,
    })
}

/// Die Bytes für `send -k`: Tastennamen durch Leerzeichen getrennt, Unbekanntes fällt weg.
pub fn keys_to_bytes(text: &str) -> String {
    text.split(' ').filter(|k| !k.is_empty()).filter_map(key_bytes).collect()
}

/// Flags vor, zwischen und nach den übrigen Argumenten; `--` beendet die Flags.
struct Args {
    values: HashMap<String, String>,
    flags: HashSet<String>,
    positional: Vec<String>,
}

impl Args {
    fn parse(args: &[String], valued: &[&str], bools: &[&str]) -> Result<Args> {
        let mut a = Args { values: HashMap::new(), flags: HashSet::new(), positional: Vec::new() };
        let mut i = 0;
        let mut flags_done = false;
        while i < args.len() {
            let arg = &args[i];
            if !flags_done && arg == "--" {
                flags_done = true;
            } else if !flags_done && valued.contains(&arg.as_str()) {
                let value = args.get(i + 1).ok_or_else(|| ControlError::new(format!("{arg} braucht einen Wert")))?;
                a.values.insert(arg.clone(), value.clone());
                i += 1;
            } else if !flags_done && bools.contains(&arg.as_str()) {
                a.flags.insert(arg.clone());
            } else if !flags_done && arg.starts_with('-') && arg.chars().count() > 1 {
                return Err(ControlError::new(format!("unbekannte Option {arg}")));
            } else {
                a.positional.push(arg.clone());
            }
            i += 1;
        }
        Ok(a)
    }

    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn has(&self, flag: &str) -> bool {
        self.flags.contains(flag)
    }

    fn no_positional(self) -> Result<Args> {
        match self.positional.first() {
            Some(p) => Err(ControlError::new(format!("unerwartetes Argument „{p}“"))),
            None => Ok(self),
        }
    }

    fn target(&self) -> Option<String> {
        self.get("-t")
    }
}

fn alias(cmd: &str) -> &str {
    match cmd {
        "-h" | "--help" => "help",
        "list" => "ls",
        "new-session" => "new",
        "kill-session" => "kill",
        "send-keys" => "send",
        "capture-pane" => "capture",
        other => other,
    }
}

/// Parst `argv` (ohne Programmnamen). Leer heißt Hilfe.
pub fn parse(argv: &[String]) -> Result<Command> {
    let Some(cmd) = argv.first() else { return Ok(Command::Help) };
    let rest = &argv[1..];
    match alias(cmd) {
        "help" => Ok(Command::Help),
        "ls" => parse_json_flag(rest).map(|json| Command::List { json }),
        "ssh-hosts" => parse_json_flag(rest).map(|json| Command::SshHosts { json }),
        "new-group" => parse_new_group(rest),
        "new" => parse_new(rest),
        "layout" => parse_layout(rest),
        "rename" => parse_rename(rest),
        "move" => parse_move(rest),
        "set-group" => parse_set_group(rest),
        "send" => parse_send(rest),
        "capture" => {
            let a = Args::parse(rest, &["-t"], &["--all"])?.no_positional()?;
            Ok(Command::Capture { target: a.target(), all: a.has("--all") })
        }
        "status" => parse_status(rest),
        "kill-server" => Args::parse(rest, &[], &[])?.no_positional().map(|_| Command::KillServer),
        other => parse_target_only(other, rest).ok_or_else(|| ControlError::new(format!("unbekannter Befehl „{cmd}“, siehe kadrell help")))?,
    }
}

/// Befehle, die nur `-t` kennen (und `select` zusätzlich `-a`).
fn parse_target_only(cmd: &str, rest: &[String]) -> Option<Result<Command>> {
    let make: fn(Option<String>) -> Command = match cmd {
        "zoom" => |target| Command::Zoom { target },
        "stop" => |target| Command::Stop { target },
        "resume" => |target| Command::Resume { target },
        "kill" => |target| Command::Kill { target },
        "kill-group" => |target| Command::KillGroup { target },
        "select" => return Some(parse_select(rest)),
        _ => return None,
    };
    Some(Args::parse(rest, &["-t"], &[]).and_then(Args::no_positional).map(|a| make(a.target())))
}

fn parse_select(rest: &[String]) -> Result<Command> {
    let a = Args::parse(rest, &["-t"], &["-a"])?.no_positional()?;
    Ok(Command::Select { target: a.target(), add: a.has("-a") })
}

fn parse_json_flag(rest: &[String]) -> Result<bool> {
    Ok(Args::parse(rest, &[], &["--json"])?.no_positional()?.has("--json"))
}

fn parse_layout(rest: &[String]) -> Result<Command> {
    match rest {
        [mode] => LayoutMode::from_raw(mode).map(Command::Layout),
        _ => None,
    }
    .ok_or_else(|| ControlError::new("layout grid|main|spiral|custom|scroll|row|stack"))
}

fn parse_rename(rest: &[String]) -> Result<Command> {
    let a = Args::parse(rest, &["-t"], &[])?;
    Ok(Command::Rename { target: a.target(), name: a.positional.join(" ") })
}

fn parse_new_group(rest: &[String]) -> Result<Command> {
    let a = Args::parse(rest, &["--name", "--color"], &[])?;
    if a.positional.len() != 1 {
        return Err(ControlError::new("new-group braucht genau einen Ordner"));
    }
    Ok(Command::NewGroup { dir: a.positional[0].clone(), name: a.get("--name"), color: color(a.get("--color"))? })
}

fn parse_move(rest: &[String]) -> Result<Command> {
    let a = Args::parse(rest, &["-t"], &[])?;
    if a.positional.len() != 1 {
        return Err(ControlError::new("move braucht genau eine Zielgruppe"));
    }
    Ok(Command::Move { target: a.target(), group: a.positional[0].clone() })
}

fn parse_set_group(rest: &[String]) -> Result<Command> {
    let a = Args::parse(rest, &["-t", "--name", "--color", "--favorite"], &[])?.no_positional()?;
    let favorite = match a.get("--favorite").as_deref() {
        None => None,
        Some("on") => Some(true),
        Some("off") => Some(false),
        Some(_) => return Err(ControlError::new("--favorite on|off")),
    };
    Ok(Command::SetGroup { target: a.target(), name: a.get("--name"), color: color(a.get("--color"))?, favorite })
}

fn parse_send(rest: &[String]) -> Result<Command> {
    let a = Args::parse(rest, &["-t"], &["--no-enter", "-k"])?;
    if a.positional.is_empty() {
        return Err(ControlError::new("send braucht Text oder Tasten"));
    }
    let keys = a.has("-k");
    if keys && let Some(bad) = a.positional.iter().find(|k| key_bytes(k).is_none()) {
        return Err(ControlError::new(format!("unbekannte Taste „{bad}“")));
    }
    Ok(Command::Send { target: a.target(), text: a.positional.join(" "), enter: !keys && !a.has("--no-enter"), keys })
}

fn parse_status(rest: &[String]) -> Result<Command> {
    let a = Args::parse(rest, &["-t", "--session-id", "--title", "--waiting-for", "--message", "--first-prompt"], &[])?;
    if a.positional.len() != 1 || !STATES.contains(&a.positional[0].as_str()) {
        return Err(ControlError::new("status working|waiting|idle"));
    }
    Ok(Command::Status(Status {
        target: a.target(),
        state: a.positional[0].clone(),
        session_id: a.get("--session-id"),
        title: a.get("--title"),
        waiting_for: a.get("--waiting-for"),
        message: a.get("--message"),
        first_prompt: a.get("--first-prompt"),
    }))
}

fn parse_new(rest: &[String]) -> Result<Command> {
    let a = Args::parse(rest, &["-t", "-c", "--name", "--resume", "--host", "--tmux", "--model", "--effort"], &["-d", "--pi"])?;
    let prompt = a.positional.join(" ");
    check_new_combination(&a, !prompt.is_empty())?;
    Ok(Command::New(NewSession {
        target: a.target(),
        dir: a.get("-c"),
        name: a.get("--name"),
        detached: a.has("-d"),
        prompt: (!prompt.is_empty()).then_some(prompt),
        resume: a.get("--resume"),
        host: a.get("--host"),
        tmux: a.get("--tmux"),
        pi: a.has("--pi"),
        model: model(a.get("--model"))?,
        effort: effort(a.get("--effort"))?,
    }))
}

/// Kombinationen, die sich ausschließen, in der Reihenfolge der Mac-App (die Meldung hängt davon ab).
fn check_new_combination(a: &Args, has_prompt: bool) -> Result<()> {
    let resume = a.get("--resume");
    if resume.is_some() && has_prompt {
        return Err(ControlError::new("--resume und prompt schließen sich aus"));
    }
    if resume.as_deref().is_some_and(|r| !is_uuid(r)) {
        return Err(ControlError::new("--resume braucht eine sessionId (UUID)"));
    }
    if a.get("--host").is_some() && (resume.is_some() || has_prompt || a.get("-c").is_some() || a.get("-t").is_some()) {
        return Err(ControlError::new("--host hat eigenen Ordner und eigene Gruppe, nicht mit -t, -c, --resume oder prompt kombinieren"));
    }
    if a.get("--tmux").is_some() && a.get("--host").is_none() {
        return Err(ControlError::new("--tmux braucht --host"));
    }
    check_non_claude(a)
}

/// Pi und Remote-Kacheln starten kein eigenes `claude`: dort gelten weder `--resume` noch `--model` und `--effort`.
fn check_non_claude(a: &Args) -> Result<()> {
    let host = a.get("--host").is_some();
    if a.has("--pi") && (a.get("--resume").is_some() || host) {
        return Err(ControlError::new("--pi nicht mit --resume oder --host kombinieren"));
    }
    if (a.has("--pi") || host) && (a.get("--model").is_some() || a.get("--effort").is_some()) {
        return Err(ControlError::new("--model und --effort gelten nur für Claude-Sessions, nicht mit --pi oder --host"));
    }
    Ok(())
}

/// Nur die Schreibweise mit Bindestrichen (8-4-4-4-12), wie `UUID(uuidString:)` auf dem Mac.
pub fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    parts.len() == 5 && parts.iter().zip(lens).all(|(p, n)| p.len() == n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Alias (`opus`) oder Modell-Id (`claude-opus-5-5`, auch mit `[1m]`); führendes `-` wäre für claude eine Option.
fn model(m: Option<String>) -> Result<Option<String>> {
    let Some(m) = m else { return Ok(None) };
    let (base, suffix) = m.find('[').map_or((m.as_str(), ""), |i| (&m[..i], &m[i..]));
    let base_ok = base.chars().next().is_some_and(|c| c.is_ascii_alphanumeric()) && base.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    let suffix_ok = suffix.is_empty()
        || (suffix.len() > 2 && suffix.ends_with(']') && suffix[1..suffix.len() - 1].chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
    if base_ok && suffix_ok {
        Ok(Some(m))
    } else {
        Err(ControlError::new("--model braucht einen Alias oder eine Modell-Id, z. B. opus oder claude-opus-5-5"))
    }
}

fn effort(e: Option<String>) -> Result<Option<String>> {
    match e {
        None => Ok(None),
        Some(e) if EFFORTS.contains(&e.as_str()) => Ok(Some(e)),
        Some(_) => Err(ControlError::new(format!("--effort {}", EFFORTS.join("|")))),
    }
}

fn color(c: Option<String>) -> Result<Option<String>> {
    let Some(c) = c else { return Ok(None) };
    let hex = c.strip_prefix('#').unwrap_or(&c);
    if hex.len() == 6 && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(Some(format!("#{}", hex.to_lowercase())))
    } else {
        Err(ControlError::new("Farbe als #rrggbb"))
    }
}

#[cfg(test)]
mod tests {
    /// Hilfetext wie in Swift (`Control.usage`), nur mit der zusätzlichen `kill-server`-Zeile des Vertrags.
    #[test]
    fn usage_ends_like_the_swift_text() {
        assert!(!USAGE.contains("Starten:"));
        assert!(USAGE.ends_with("

Die Session-Keys stehen in `kadrell ls`. Exit-Code 1 bei Fehlern, Meldung auf stderr."));
    }

    use super::*;

    const UUID: &str = "0b7e1c2a-3f4d-4e5f-8a9b-0c1d2e3f4a5b";

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    fn ok(s: &[&str]) -> Command {
        parse(&argv(s)).unwrap_or_else(|e| panic!("{s:?}: {e}"))
    }

    fn err(s: &[&str]) -> String {
        parse(&argv(s)).unwrap_err().0
    }

    fn new(f: impl FnOnce(&mut NewSession)) -> Command {
        let mut n = NewSession::default();
        f(&mut n);
        Command::New(n)
    }

    fn some(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn parse_basics() {
        assert_eq!(ok(&[]), Command::Help);
        assert_eq!(ok(&["list", "--json"]), Command::List { json: true });
        assert_eq!(ok(&["new-group", "~/p", "--name", "API", "--color", "FAB387"]), Command::NewGroup { dir: "~/p".into(), name: some("API"), color: some("#fab387") });
        assert_eq!(ok(&["new", "-d", "-t", "api", "fix", "the", "tests"]), new(|n| {
            n.detached = true;
            n.target = some("api");
            n.prompt = some("fix the tests");
        }));
        assert_eq!(ok(&["new", "--", "-x"]), new(|n| n.prompt = some("-x")));
        assert_eq!(ok(&["new", "-c", "/p", "--resume", UUID]), new(|n| {
            n.dir = some("/p");
            n.resume = some(UUID);
        }));
        assert_eq!(ok(&["set-group", "--favorite", "off"]), Command::SetGroup { target: None, name: None, color: None, favorite: Some(false) });
        assert_eq!(ok(&["send", "-t", "ab", "hallo", "welt"]), Command::Send { target: some("ab"), text: "hallo welt".into(), enter: true, keys: false });
        assert_eq!(ok(&["send-keys", "-k", "C-c", "Enter"]), Command::Send { target: None, text: "C-c Enter".into(), enter: false, keys: true });
        assert_eq!(ok(&["send", "--no-enter", "x"]), Command::Send { target: None, text: "x".into(), enter: false, keys: false });
        assert_eq!(ok(&["kill"]), Command::Kill { target: None });
        assert_eq!(ok(&["kill-session", "-t", "a"]), Command::Kill { target: some("a") });
        assert_eq!(ok(&["move", "-t", "44a3", "acme skills"]), Command::Move { target: some("44a3"), group: "acme skills".into() });
        assert_eq!(ok(&["layout", "stack"]), Command::Layout(LayoutMode::Stack));
        assert_eq!(ok(&["layout", "row"]), Command::Layout(LayoutMode::Row));
        assert_eq!(ok(&["ssh-hosts", "--json"]), Command::SshHosts { json: true });
        assert_eq!(ok(&["ssh-hosts"]), Command::SshHosts { json: false });
        assert_eq!(ok(&["select", "-t", "a", "-a"]), Command::Select { target: some("a"), add: true });
        assert_eq!(ok(&["zoom"]), Command::Zoom { target: None });
        assert_eq!(ok(&["capture-pane", "--all"]), Command::Capture { target: None, all: true });
        assert_eq!(ok(&["rename", "-t", "a", "neuer", "Name"]), Command::Rename { target: some("a"), name: "neuer Name".into() });
        assert_eq!(ok(&["kill-server"]), Command::KillServer);
        assert_eq!(ok(&["--help"]), Command::Help);
    }

    #[test]
    fn new_host_tmux_and_pi() {
        assert_eq!(ok(&["new", "--host", "vault", "--tmux", "main", "-d"]), new(|n| {
            n.host = some("vault");
            n.tmux = some("main");
            n.detached = true;
        }));
        assert_eq!(ok(&["new", "--pi", "-c", "/p", "Idee"]), new(|n| {
            n.pi = true;
            n.dir = some("/p");
            n.prompt = some("Idee");
        }));
        assert_eq!(err(&["new", "--pi", "--host", "vault"]), "--pi nicht mit --resume oder --host kombinieren");
        for bad in [&["new", "--host", "vault", "-c", "/p"][..], &["new", "--host", "vault", "hallo"], &["new", "--host", "vault", "-t", "g"]] {
            assert_eq!(err(bad), "--host hat eigenen Ordner und eigene Gruppe, nicht mit -t, -c, --resume oder prompt kombinieren");
        }
        assert_eq!(err(&["new", "--tmux", "main"]), "--tmux braucht --host");
    }

    #[test]
    fn new_model_and_effort() {
        assert_eq!(ok(&["new", "-d", "--model", "claude-opus-5-5", "--effort", "medium", "Hallo"]), new(|n| {
            n.detached = true;
            n.prompt = some("Hallo");
            n.model = some("claude-opus-5-5");
            n.effort = some("medium");
        }));
        assert_eq!(ok(&["new", "--model", "opus"]), new(|n| n.model = some("opus")));
        assert_eq!(ok(&["new", "--model", "claude-opus-4-8[1m]"]), new(|n| n.model = some("claude-opus-4-8[1m]")));
        assert_eq!(ok(&["new", "--resume", UUID, "--effort", "max"]), new(|n| {
            n.resume = some(UUID);
            n.effort = some("max");
        }));
        for bad in [
            &["--effort", "extreme"][..],
            &["--effort", ""],
            &["--model", ""],
            &["--model", "--dangerously-skip-permissions"],
            &["--model", "a b"],
            &["--model"],
            &["--effort"],
            &["--pi", "--model", "opus"],
            &["--host", "vault", "--effort", "low"],
            &["--model", "-x"],
            &["--model", "opus[1M]"],
            &["--model", "opus[]"],
        ] {
            let mut v = vec!["new"];
            v.extend_from_slice(bad);
            assert!(parse(&argv(&v)).is_err(), "{bad:?}");
        }
        assert_eq!(err(&["new", "--effort", "huge"]), "--effort low|medium|high|xhigh|max");
        assert_eq!(err(&["new", "--pi", "--model", "opus"]), "--model und --effort gelten nur für Claude-Sessions, nicht mit --pi oder --host");
    }

    #[test]
    fn new_resume_rules() {
        assert_eq!(err(&["new", "--resume", UUID, "hallo"]), "--resume und prompt schließen sich aus");
        assert_eq!(err(&["new", "--resume", "--dangerously-skip-permissions"]), "--resume braucht eine sessionId (UUID)");
        assert_eq!(err(&["new", "--resume", "../../x"]), "--resume braucht eine sessionId (UUID)");
        // Foundation-UUID kennt nur die Schreibweise mit Bindestrichen.
        assert_eq!(err(&["new", "--resume", "0b7e1c2a3f4d4e5f8a9b0c1d2e3f4a5b"]), "--resume braucht eine sessionId (UUID)");
        assert_eq!(err(&["new", "--resume", "3f2b8c1e-0000-4000-8000-00000000000g"]), "--resume braucht eine sessionId (UUID)");
        assert!(is_uuid("3F2B8C1E-0000-4000-8000-000000000000"));
        // Die Reihenfolge der Prüfungen bestimmt die Meldung: erst Kombination, dann Modell.
        assert_eq!(err(&["new", "--resume", "x", "--model", "-"]), "--resume braucht eine sessionId (UUID)");
    }

    #[test]
    fn parse_errors() {
        assert_eq!(err(&["frobnicate"]), "unbekannter Befehl „frobnicate“, siehe kadrell help");
        assert_eq!(err(&["ls", "--jsn"]), "unbekannte Option --jsn");
        assert_eq!(err(&["ls", "x"]), "unerwartetes Argument „x“");
        assert_eq!(err(&["new-group"]), "new-group braucht genau einen Ordner");
        assert_eq!(err(&["new", "-t"]), "-t braucht einen Wert");
        assert_eq!(err(&["layout", "tiles"]), "layout grid|main|spiral|custom|scroll|row|stack");
        assert_eq!(err(&["layout"]), "layout grid|main|spiral|custom|scroll|row|stack");
        assert_eq!(err(&["set-group", "--favorite", "ja"]), "--favorite on|off");
        assert_eq!(err(&["new-group", "/p", "--color", "red"]), "Farbe als #rrggbb");
        assert_eq!(err(&["send", "-k", "Hyper"]), "unbekannte Taste „Hyper“");
        assert_eq!(err(&["send"]), "send braucht Text oder Tasten");
        assert_eq!(err(&["move", "-t", "a"]), "move braucht genau eine Zielgruppe");
        assert_eq!(err(&["status", "busy"]), "status working|waiting|idle");
        assert_eq!(err(&["kill", "x"]), "unerwartetes Argument „x“");
        assert_eq!(err(&["kill-server", "x"]), "unerwartetes Argument „x“");
    }

    #[test]
    fn parse_status() {
        let want = Command::Status(Status {
            target: None,
            state: "waiting".into(),
            session_id: some("s1"),
            title: None,
            waiting_for: some("Bash"),
            message: None,
            first_prompt: None,
        });
        assert_eq!(ok(&["status", "waiting", "--waiting-for", "Bash", "--session-id", "s1"]), want);
        let full = ok(&["status", "-t", "ab", "idle", "--title", "T", "--message", "M", "--first-prompt", "F"]);
        let Command::Status(s) = full else { panic!() };
        assert_eq!((s.target.as_deref(), s.title.as_deref(), s.message.as_deref(), s.first_prompt.as_deref()), (Some("ab"), Some("T"), Some("M"), Some("F")));
        for bad in [&["status"][..], &["status", "busy"], &["status", "idle", "extra"], &["status", "idle", "--title"]] {
            assert!(parse(&argv(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn colors_and_keys() {
        assert_eq!(color(Some("ABCDEF".into())).unwrap().as_deref(), Some("#abcdef"));
        assert!(color(Some("#abc".into())).is_err());
        assert_eq!(key_bytes("Enter"), Some("\r"));
        assert_eq!(key_bytes("ESC"), Some("\u{1b}"));
        assert_eq!(keys_to_bytes("C-c Enter bogus Up"), "\u{3}\r\u{1b}[A");
        assert_eq!(LayoutMode::ALL.len(), 7);
        assert_eq!(LayoutMode::from_raw("scroll"), Some(LayoutMode::Scroll));
    }

    #[test]
    fn usage_lists_every_command() {
        for word in ["ls", "ssh-hosts", "new-group", "new", "select", "layout", "zoom", "rename", "move", "set-group", "stop", "resume", "kill", "kill-group", "send", "capture", "status", "hook claude", "kill-server"] {
            assert!(USAGE.contains(&format!("kadrell {word}")), "{word}");
        }
        assert!(!USAGE.contains('\u{2014}'));
    }
}
