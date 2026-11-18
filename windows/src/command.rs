//! `kadrell <befehl>`: dieselben Befehle, Flags und Fehlermeldungen wie die Mac-App (`Services/Control.swift`).
//! Hier nur der reine Teil: Befehle parsen, Tastennamen, Hilfetext. Windows kennt die Befehle ohne Gegenstück
//! (Layouts, Zoom, ssh, Pi) noch nicht und meldet sie als unbekannt.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlError(pub String);

impl ControlError {
    pub fn new(message: impl Into<String>) -> Self {
        ControlError(message.into())
    }
}

pub type Result<T> = std::result::Result<T, ControlError>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewSession {
    pub target: Option<String>,
    pub dir: Option<String>,
    pub name: Option<String>,
    pub detached: bool,
    pub prompt: Option<String>,
    pub resume: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

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
    NewGroup { dir: String, name: Option<String>, color: Option<String> },
    New(NewSession),
    Select { target: Option<String> },
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
    /// Nur Windows (wie tmux): Kadrell sauber beenden, ohne Fenster bedienen zu müssen.
    KillServer,
}

pub const STATES: [&str; 3] = ["working", "waiting", "idle"];
const EFFORTS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub const USAGE: &str = "Kadrell fernsteuern (die App muss laufen, sonst wird sie gestartet).

Ziele (-t): Session-Key oder dessen Anfang, sessionId, Titel; Gruppen-Id oder deren Anfang, Gruppenname.
Ohne -t gilt die Session, in der das Kommando läuft, sonst die fokussierte.

  kadrell ls [--json]                                  Gruppen und Sessions auflisten
  kadrell new-group <ordner> [--name N] [--color #rrggbb]
                                                       Gruppe anlegen (als Favorit, bleibt auch leer stehen)
  kadrell new [-t gruppe] [-c ordner] [--name N] [-d] [--resume sessionId]
              [--model alias|id] [--effort low|medium|high|xhigh|max] [prompt …]
                                                       Session starten, optional mit erster Nachricht;
                                                       -d: im Hintergrund, Auswahl bleibt;
                                                       --model/--effort: nur für diese Session statt der Einstellung;
                                                       --resume: bestehende Konversation übernehmen
  kadrell select [-t session]                          Session zeigen
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

Starten: kadrell [--profile name|tmp] öffnet das Fenster. Die Session-Keys stehen in `kadrell ls`.
Exit-Code 1 bei Fehlern, Meldung auf stderr.";

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

pub fn parse(argv: &[String]) -> Result<Command> {
    let Some(cmd) = argv.first() else { return Ok(Command::Help) };
    let rest = &argv[1..];
    let only_target = |rest: &[String]| Args::parse(rest, &["-t"], &[]).and_then(Args::no_positional).map(|a| a.target());
    match alias(cmd) {
        "help" => Ok(Command::Help),
        "ls" => Ok(Command::List { json: Args::parse(rest, &[], &["--json"])?.no_positional()?.has("--json") }),
        "new-group" => parse_new_group(rest),
        "new" => parse_new(rest),
        "select" => Ok(Command::Select { target: only_target(rest)? }),
        "rename" => {
            let a = Args::parse(rest, &["-t"], &[])?;
            Ok(Command::Rename { target: a.target(), name: a.positional.join(" ") })
        }
        "move" => parse_move(rest),
        "set-group" => parse_set_group(rest),
        "stop" => Ok(Command::Stop { target: only_target(rest)? }),
        "resume" => Ok(Command::Resume { target: only_target(rest)? }),
        "kill" => Ok(Command::Kill { target: only_target(rest)? }),
        "kill-group" => Ok(Command::KillGroup { target: only_target(rest)? }),
        "send" => parse_send(rest),
        "capture" => {
            let a = Args::parse(rest, &["-t"], &["--all"])?.no_positional()?;
            Ok(Command::Capture { target: a.target(), all: a.has("--all") })
        }
        "status" => parse_status(rest),
        "kill-server" => Args::parse(rest, &[], &[])?.no_positional().map(|_| Command::KillServer),
        _ => Err(ControlError::new(format!("unbekannter Befehl „{cmd}“, siehe kadrell help"))),
    }
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
    if keys {
        if let Some(bad) = a.positional.iter().find(|k| key_bytes(k).is_none()) {
            return Err(ControlError::new(format!("unbekannte Taste „{bad}“")));
        }
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
    let a = Args::parse(rest, &["-t", "-c", "--name", "--resume", "--model", "--effort"], &["-d"])?;
    let prompt = a.positional.join(" ");
    if a.get("--resume").is_some() && !prompt.is_empty() {
        return Err(ControlError::new("--resume und prompt schließen sich aus"));
    }
    if let Some(r) = a.get("--resume") {
        if uuid::Uuid::parse_str(&r).is_err() {
            return Err(ControlError::new("--resume braucht eine sessionId (UUID)"));
        }
    }
    Ok(Command::New(NewSession {
        target: a.target(),
        dir: a.get("-c"),
        name: a.get("--name"),
        detached: a.has("-d"),
        prompt: (!prompt.is_empty()).then_some(prompt),
        resume: a.get("--resume"),
        model: model(a.get("--model"))?,
        effort: effort(a.get("--effort"))?,
    }))
}

/// Alias (`opus`) oder Modell-Id (`claude-opus-5-5`, auch mit `[1m]`); führendes `-` wäre für claude eine Option.
fn model(m: Option<String>) -> Result<Option<String>> {
    let Some(m) = m else { return Ok(None) };
    let (base, suffix) = match m.find('[') {
        Some(i) => (&m[..i], &m[i..]),
        None => (m.as_str(), ""),
    };
    let base_ok = base.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && base.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    let suffix_ok = suffix.is_empty()
        || (suffix.len() > 2
            && suffix.ends_with(']')
            && suffix[1..suffix.len() - 1].chars().all(|c| c.is_ascii_digit() || c.is_ascii_lowercase()));
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
    use super::*;

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn new_with_flags_between_prompt_words() {
        let cmd = parse(&argv(&["new", "-t", "g1", "-c", "C:/x", "-d", "--name", "M", "hallo", "welt"])).unwrap();
        assert_eq!(
            cmd,
            Command::New(NewSession {
                target: Some("g1".into()),
                dir: Some("C:/x".into()),
                name: Some("M".into()),
                detached: true,
                prompt: Some("hallo welt".into()),
                ..Default::default()
            })
        );
    }

    #[test]
    fn prompt_after_double_dash_keeps_dashes() {
        let Command::New(n) = parse(&argv(&["new", "--", "--version"])).unwrap() else { panic!() };
        assert_eq!(n.prompt.as_deref(), Some("--version"));
    }

    #[test]
    fn send_keys_and_text() {
        assert_eq!(
            parse(&argv(&["send", "-t", "k", "-k", "Escape"])).unwrap(),
            Command::Send { target: Some("k".into()), text: "Escape".into(), enter: false, keys: true }
        );
        assert_eq!(
            parse(&argv(&["send-keys", "-t", "k", "/model", "opus"])).unwrap(),
            Command::Send { target: Some("k".into()), text: "/model opus".into(), enter: true, keys: false }
        );
        assert!(parse(&argv(&["send", "-k", "Hyper"])).is_err());
    }

    #[test]
    fn errors_match_mac_messages() {
        assert_eq!(parse(&argv(&["foo"])).unwrap_err().0, "unbekannter Befehl „foo“, siehe kadrell help");
        assert_eq!(parse(&argv(&["ls", "--xml"])).unwrap_err().0, "unbekannte Option --xml");
        assert_eq!(parse(&argv(&["move", "-t", "a"])).unwrap_err().0, "move braucht genau eine Zielgruppe");
        assert_eq!(parse(&argv(&["new", "--effort", "huge"])).unwrap_err().0, "--effort low|medium|high|xhigh|max");
        assert_eq!(parse(&argv(&["set-group", "--favorite", "ja"])).unwrap_err().0, "--favorite on|off");
    }

    #[test]
    fn model_and_color_validation() {
        assert!(parse(&argv(&["new", "--model", "claude-opus-4-8[1m]"])).is_ok());
        assert!(parse(&argv(&["new", "--model", "-x"])).is_err());
        assert_eq!(color(Some("ABCDEF".into())).unwrap().as_deref(), Some("#abcdef"));
        assert!(color(Some("#abc".into())).is_err());
    }

    #[test]
    fn status_and_resume() {
        assert!(parse(&argv(&["status", "busy"])).is_err());
        assert_eq!(parse(&argv(&["kill-server"])).unwrap(), Command::KillServer);
        assert!(parse(&argv(&["new", "--resume", "nope"])).is_err());
        assert!(parse(&argv(&["new", "--resume", "3f2b8c1e-0000-4000-8000-000000000000", "hi"])).is_err());
    }
}
